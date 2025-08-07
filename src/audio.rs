use crate::{config::AudioConfig, handlers::KeyboardTranscriptionHandler, state::AppState};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use eyre::{OptionExt, Result, WrapErr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn determine_audio_sample_rate(audio_config: &AudioConfig) -> Result<u32> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_eyre("No input device available")?;

    let supported_configs_range = device
        .supported_input_configs()
        .wrap_err("Failed to get supported configs")?;

    // Find the best matching config with priority for 16kHz, fallback to any available rate
    let supported_config =
        find_best_config_with_priority(supported_configs_range, audio_config.channels)?;

    Ok(supported_config.config().sample_rate.0)
}

fn capture_audio_with_rate(
    audio_tx: mpsc::Sender<Vec<u8>>,
    recording: Arc<AtomicBool>,
    shutdown_token: CancellationToken,
    audio_config: AudioConfig,
    sample_rate: u32,
) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_eyre("No input device available")?;

    info!("Using input device: {}", device.name()?);
    info!(
        "Audio config: {} channels, {} Hz",
        audio_config.channels, sample_rate
    );

    let supported_configs_range = device
        .supported_input_configs()
        .wrap_err("Failed to get supported configs")?;

    // Find the best matching config with priority for 16kHz, fallback to any available rate
    let supported_config =
        find_best_config_with_priority(supported_configs_range, audio_config.channels)?;

    let config = supported_config.config();
    let sample_format = supported_config.sample_format();

    info!(
        "Audio config: {} channels, {} Hz, {:?}",
        config.channels, config.sample_rate.0, sample_format
    );

    // Calculate samples per chunk based on actual sample rate
    let samples_per_chunk = (sample_rate * audio_config.audio_chunk_ms / 1000) as usize;

    let err_fn = |err| error!("Audio stream error: {}", err);

    // Create channel for audio samples
    let (sample_tx, sample_rx) = std::sync::mpsc::channel::<f32>();

    let stream = match sample_format {
        SampleFormat::F32 => {
            build_input_stream::<f32>(&device, &config, sample_tx.clone(), err_fn)?
        }
        SampleFormat::I16 => {
            let (tx_i16, rx_i16) = std::sync::mpsc::channel::<i16>();
            let stream = build_input_stream::<i16>(&device, &config, tx_i16, err_fn)?;

            let tx_f32 = sample_tx.clone();
            std::thread::spawn(move || {
                while let Ok(sample) = rx_i16.recv() {
                    let normalized = sample.to_float_sample();
                    if tx_f32.send(normalized).is_err() {
                        break;
                    }
                }
            });

            stream
        }
        SampleFormat::U16 => {
            let (tx_u16, rx_u16) = std::sync::mpsc::channel::<u16>();
            let stream = build_input_stream::<u16>(&device, &config, tx_u16, err_fn)?;

            let tx_f32 = sample_tx.clone();
            std::thread::spawn(move || {
                while let Ok(sample) = rx_u16.recv() {
                    let normalized = sample.to_float_sample();
                    if tx_f32.send(normalized).is_err() {
                        break;
                    }
                }
            });

            stream
        }
        SampleFormat::U8 => {
            let (tx_u8, rx_u8) = std::sync::mpsc::channel::<u8>();
            let stream = build_input_stream::<u8>(&device, &config, tx_u8, err_fn)?;

            let tx_f32 = sample_tx.clone();
            std::thread::spawn(move || {
                while let Ok(sample) = rx_u8.recv() {
                    // Convert U8 (0-255) to f32 (-1.0 to 1.0)
                    let normalized = (sample as f32 / 128.0) - 1.0;
                    if tx_f32.send(normalized).is_err() {
                        break;
                    }
                }
            });

            stream
        }
        SampleFormat::I32 => {
            let (tx_i32, rx_i32) = std::sync::mpsc::channel::<i32>();
            let stream = build_input_stream::<i32>(&device, &config, tx_i32, err_fn)?;

            let tx_f32 = sample_tx.clone();
            std::thread::spawn(move || {
                while let Ok(sample) = rx_i32.recv() {
                    let normalized = sample.to_float_sample();
                    if tx_f32.send(normalized).is_err() {
                        break;
                    }
                }
            });

            stream
        }
        _ => bail!("Unsupported sample format: {:?}", sample_format),
    };

    stream.play()?;

    // Buffer for collecting samples before conversion
    let mut sample_buffer = Vec::with_capacity(samples_per_chunk);
    let mut total_samples_sent = 0u64;
    let mut chunks_sent = 0u64;

    loop {
        if shutdown_token.is_cancelled() {
            info!("Audio capture shutting down");
            break;
        }

        if !recording.load(std::sync::atomic::Ordering::Relaxed) {
            debug!("Recording stopped in audio capture");
            break;
        }

        // Use recv_timeout to avoid busy-waiting
        match sample_rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(sample) => {
                sample_buffer.push(sample);

                // Continue collecting samples up to chunk size
                while sample_buffer.len() < samples_per_chunk {
                    match sample_rx.try_recv() {
                        Ok(s) => sample_buffer.push(s),
                        Err(_) => break,
                    }
                }

                // Send chunk if we have enough samples
                if sample_buffer.len() >= samples_per_chunk {
                    chunks_sent += 1;

                    // Convert f32 samples to i16 (Linear16) format
                    let mut i16_buffer = Vec::with_capacity(sample_buffer.len() * 2);
                    for &f32_sample in &sample_buffer {
                        // Convert f32 (-1.0 to 1.0) to i16 (-32768 to 32767)
                        let i16_sample = (f32_sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        i16_buffer.extend_from_slice(&i16_sample.to_le_bytes());
                    }

                    total_samples_sent += sample_buffer.len() as u64;
                    trace!(
                        "Sending audio chunk #{}: {} samples ({} bytes), total sent: {} samples",
                        chunks_sent,
                        sample_buffer.len(),
                        i16_buffer.len(),
                        total_samples_sent
                    );

                    if audio_tx.blocking_send(i16_buffer).is_err() {
                        info!("Audio receiver dropped, stopping capture");
                        break;
                    }
                    sample_buffer.clear();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Normal timeout, continue loop
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                info!("Audio sample channel disconnected");
                break;
            }
        }
    }

    // Send any remaining samples
    if !sample_buffer.is_empty() {
        let mut i16_buffer = Vec::with_capacity(sample_buffer.len() * 2);
        for &f32_sample in &sample_buffer {
            let i16_sample = (f32_sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            i16_buffer.extend_from_slice(&i16_sample.to_le_bytes());
        }
        let _ = audio_tx.blocking_send(i16_buffer);
    }

    Ok(())
}


pub async fn start_recording(app_state: AppState) -> Result<()> {
    debug!("Starting recording process");
    let (audio_tx, audio_rx) = tokio::sync::mpsc::channel(100);

    let audio_config = app_state.config.read().unwrap().audio.clone();
    let app_state_audio = app_state.clone();

    // First, determine the actual sample rate that will be used
    let actual_sample_rate = determine_audio_sample_rate(&audio_config)?;
    info!("Audio will use {} Hz sample rate", actual_sample_rate);

    // Start audio capture task
    tokio::task::spawn_blocking(move || {
        debug!("Audio capture task started");
        if let Err(e) = capture_audio_with_rate(
            audio_tx,
            app_state_audio.recording.clone(),
            app_state_audio.shutdown_token.child_token(),
            audio_config,
            actual_sample_rate,
        ) {
            error!("Audio capture error: {}", e);
        }
        debug!("Audio capture task ended");
    });

    debug!(
        "Creating transcription stream with {} Hz sample rate",
        actual_sample_rate
    );
    let transcriber = app_state.transcriber.read().unwrap().clone();
    let transcription_rx = transcriber
        .transcribe_stream(audio_rx, actual_sample_rate)
        .await?;
    debug!("Transcription stream created, waiting for transcriptions");

    let use_interim_results = app_state
        .config
        .read()
        .unwrap()
        .transcription
        .use_interim_results;

    let handler = KeyboardTranscriptionHandler::new(use_interim_results);

    // Use a select loop to handle both transcription results and recording state
    tokio::select! {
        result = crate::handlers::process_transcription_with_handler(transcription_rx, handler) => {
            if let Err(e) = result {
                error!("Transcription processing error: {}", e);
            }
        }
        _ = async {
            while app_state.recording.load(Ordering::Relaxed) {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        } => {
            debug!("Recording stopped, breaking loop");
        }
    }

    debug!("Transcription loop ended");
    Ok(())
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sender: std::sync::mpsc::Sender<T>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: Sample + Send + 'static + cpal::SizedSample,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            for &sample in data {
                if sender.send(sample).is_err() {
                    break;
                }
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn find_best_config_with_priority(
    configs: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
    target_channels: u16,
) -> Result<cpal::SupportedStreamConfig> {
    let mut best_config = None;
    let mut best_score = f32::MAX;
    let preferred_sample_rate = 16000u32; // Priority for 16kHz

    for config_range in configs {
        // Check if this config supports our channel count
        if config_range.channels() != target_channels {
            continue;
        }

        let min_rate = config_range.min_sample_rate().0;
        let max_rate = config_range.max_sample_rate().0;

        // Try preferred rate first (16kHz)
        let sample_rate = if preferred_sample_rate >= min_rate && preferred_sample_rate <= max_rate
        {
            cpal::SampleRate(preferred_sample_rate)
        } else {
            // Fallback: use the rate closest to 16kHz within the available range
            if preferred_sample_rate < min_rate {
                config_range.min_sample_rate()
            } else {
                config_range.max_sample_rate()
            }
        };

        // Calculate score (lower is better)
        // Heavily prioritize 16kHz, but allow fallbacks
        let rate_diff = (sample_rate.0 as f32 - preferred_sample_rate as f32).abs();
        let rate_score = if sample_rate.0 == preferred_sample_rate {
            0.0 // Perfect match gets best score
        } else {
            rate_diff / 1000.0 // Fallback rates get penalized based on distance from 16kHz
        };

        let format_score = match config_range.sample_format() {
            SampleFormat::F32 => 0.0,  // Preferred
            SampleFormat::I16 => 10.0, // Good
            SampleFormat::I32 => 15.0, // Good but more processing
            SampleFormat::U16 => 20.0, // Acceptable
            SampleFormat::U8 => 30.0,  // Less preferred but supported
            _ => 1000.0,               // Not supported
        };

        let score = rate_score + format_score;

        if score < best_score {
            best_score = score;
            best_config = Some(config_range.with_sample_rate(sample_rate));
        }
    }

    let config = best_config.ok_or_eyre("No compatible audio configuration found")?;
    info!(
        "Selected audio configuration: {} Hz (preferred: {} Hz)",
        config.config().sample_rate.0,
        preferred_sample_rate
    );
    Ok(config)
}

