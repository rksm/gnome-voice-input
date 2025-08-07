use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Sample, SampleFormat};
use eyre::{OptionExt, Result};
use futures::stream::Stream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::config::AudioConfig;

pub struct AudioCapture {
    pub sample_rx: std::sync::mpsc::Receiver<f32>,
    pub stream: cpal::Stream,
}

pub struct SimpleAudioCapture {
    pub sample_rx: std::sync::mpsc::Receiver<f32>,
    pub stream: cpal::Stream,
}

/// Initialize audio capture with the given configuration
pub fn init_audio_capture(audio_config: &AudioConfig) -> Result<AudioCapture> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_eyre("No input device available")?;

    info!("Using input device: {}", device.name()?);
    info!(
        "Requested config: {} channels, {} Hz",
        audio_config.channels, audio_config.sample_rate
    );

    let supported_configs_range = device.supported_input_configs()?;

    // Find the best matching config based on our requirements
    let supported_config = find_best_config(
        supported_configs_range,
        audio_config.sample_rate,
        audio_config.channels,
    )?;

    let config = supported_config.config();
    let sample_format = supported_config.sample_format();

    info!(
        "Audio config: {} channels, {} Hz, {:?}",
        config.channels, config.sample_rate.0, sample_format
    );

    let err_fn = |err| error!("Audio stream error: {}", err);
    let (sample_tx, sample_rx) = std::sync::mpsc::channel::<f32>();

    let stream = build_stream_for_format(sample_format, &device, &config, sample_tx, err_fn)?;

    Ok(AudioCapture { sample_rx, stream })
}

/// Initialize simple audio capture with default settings
pub fn init_simple_audio_capture() -> Result<SimpleAudioCapture> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_eyre("No input device available")?;

    println!("Using input device: {}", device.name()?);

    let config = device.default_input_config()?;
    let sample_format = config.sample_format();
    let config = config.config();

    println!(
        "Audio config: {} channels, {} Hz, {:?}",
        config.channels, config.sample_rate.0, sample_format
    );

    let err_fn = |err| error!("Audio stream error: {}", err);
    let (sample_tx, sample_rx) = std::sync::mpsc::channel::<f32>();

    let stream = build_stream_for_format(sample_format, &device, &config, sample_tx, err_fn)?;

    Ok(SimpleAudioCapture { sample_rx, stream })
}

/// Build appropriate input stream based on sample format
fn build_stream_for_format(
    sample_format: SampleFormat,
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_tx: std::sync::mpsc::Sender<f32>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static + Clone,
) -> Result<cpal::Stream> {
    match sample_format {
        SampleFormat::F32 => build_input_stream::<f32>(device, config, sample_tx, err_fn),
        SampleFormat::I16 => {
            let (tx_i16, rx_i16) = std::sync::mpsc::channel::<i16>();
            let stream = build_input_stream::<i16>(device, config, tx_i16, err_fn)?;

            let tx_f32 = sample_tx;
            std::thread::spawn(move || {
                while let Ok(sample) = rx_i16.recv() {
                    let normalized = sample.to_float_sample();
                    if tx_f32.send(normalized).is_err() {
                        break;
                    }
                }
            });

            Ok(stream)
        }
        SampleFormat::U16 => {
            let (tx_u16, rx_u16) = std::sync::mpsc::channel::<u16>();
            let stream = build_input_stream::<u16>(device, config, tx_u16, err_fn)?;

            let tx_f32 = sample_tx;
            std::thread::spawn(move || {
                while let Ok(sample) = rx_u16.recv() {
                    let normalized = sample.to_float_sample();
                    if tx_f32.send(normalized).is_err() {
                        break;
                    }
                }
            });

            Ok(stream)
        }
        SampleFormat::U8 => {
            let (tx_u8, rx_u8) = std::sync::mpsc::channel::<u8>();
            let stream = build_input_stream::<u8>(device, config, tx_u8, err_fn)?;

            let tx_f32 = sample_tx;
            std::thread::spawn(move || {
                while let Ok(sample) = rx_u8.recv() {
                    // Convert U8 (0-255) to f32 (-1.0 to 1.0)
                    let normalized = (sample as f32 / 128.0) - 1.0;
                    if tx_f32.send(normalized).is_err() {
                        break;
                    }
                }
            });

            Ok(stream)
        }
        SampleFormat::I32 => {
            let (tx_i32, rx_i32) = std::sync::mpsc::channel::<i32>();
            let stream = build_input_stream::<i32>(device, config, tx_i32, err_fn)?;

            let tx_f32 = sample_tx;
            std::thread::spawn(move || {
                while let Ok(sample) = rx_i32.recv() {
                    let normalized = sample.to_float_sample();
                    if tx_f32.send(normalized).is_err() {
                        break;
                    }
                }
            });

            Ok(stream)
        }
        _ => Err(eyre::eyre!(
            "Unsupported sample format: {:?}",
            sample_format
        )),
    }
}

/// Capture and process audio samples into chunks
pub fn process_audio_chunks(
    sample_rx: std::sync::mpsc::Receiver<f32>,
    audio_tx: mpsc::Sender<Vec<u8>>,
    recording: Arc<AtomicBool>,
    samples_per_chunk: usize,
) -> Result<()> {
    let mut sample_buffer = Vec::with_capacity(samples_per_chunk);
    let mut total_samples_sent = 0u64;
    let mut chunks_sent = 0u64;

    loop {
        if !recording.load(Ordering::Relaxed) {
            break;
        }

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
                    debug!(
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

/// Build input stream for a specific sample type
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

/// Find best audio configuration based on requirements
fn find_best_config(
    configs: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
    target_sample_rate: u32,
    target_channels: u16,
) -> Result<cpal::SupportedStreamConfig> {
    let mut best_config = None;
    let mut best_score = f32::MAX;

    for config_range in configs {
        // Check if this config supports our channel count
        if config_range.channels() != target_channels {
            continue;
        }

        // Check if target sample rate is in range
        let min_rate = config_range.min_sample_rate().0;
        let max_rate = config_range.max_sample_rate().0;

        let sample_rate = if target_sample_rate >= min_rate && target_sample_rate <= max_rate {
            cpal::SampleRate(target_sample_rate)
        } else if target_sample_rate < min_rate {
            config_range.min_sample_rate()
        } else {
            config_range.max_sample_rate()
        };

        // Calculate score (lower is better)
        let rate_diff = (sample_rate.0 as f32 - target_sample_rate as f32).abs();
        let format_score = match config_range.sample_format() {
            SampleFormat::F32 => 0.0,  // Preferred
            SampleFormat::I16 => 10.0, // Good
            SampleFormat::I32 => 15.0, // Good but more processing
            SampleFormat::U16 => 20.0, // Acceptable
            SampleFormat::U8 => 30.0,  // Less preferred but supported
            _ => 1000.0,               // Not supported
        };

        let score = rate_diff / 1000.0 + format_score;

        if score < best_score {
            best_score = score;
            best_config = Some(config_range.with_sample_rate(sample_rate));
        }
    }

    best_config.ok_or_eyre("No compatible audio configuration found")
}

/// Process audio samples for simple use cases
pub fn process_simple_audio(
    sample_rx: std::sync::mpsc::Receiver<f32>,
    audio_tx: mpsc::Sender<Vec<u8>>,
    recording: Arc<AtomicBool>,
) -> Result<()> {
    let samples_per_chunk = (16000 * 25 / 1000) as usize; // 25ms chunks at 16kHz
    let mut sample_buffer = Vec::with_capacity(samples_per_chunk);

    loop {
        if !recording.load(Ordering::Relaxed) {
            break;
        }

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
                    // Convert f32 samples to i16 (Linear16) format
                    let mut i16_buffer = Vec::with_capacity(sample_buffer.len() * 2);
                    for &f32_sample in &sample_buffer {
                        let i16_sample = (f32_sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        i16_buffer.extend_from_slice(&i16_sample.to_le_bytes());
                    }

                    if audio_tx.blocking_send(i16_buffer).is_err() {
                        break;
                    }
                    sample_buffer.clear();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

/// Convert mpsc::Receiver to a Stream that produces Result<Bytes, Error>
pub fn create_audio_stream(
    mut audio_rx: mpsc::Receiver<Vec<u8>>,
) -> impl Stream<Item = Result<bytes::Bytes, std::io::Error>> {
    futures::stream::poll_fn(move |cx| match audio_rx.poll_recv(cx) {
        std::task::Poll::Ready(Some(data)) => {
            debug!("Audio stream produced {} bytes", data.len());
            std::task::Poll::Ready(Some(Ok(bytes::Bytes::from(data))))
        }
        std::task::Poll::Ready(None) => {
            debug!("Audio stream ended");
            std::task::Poll::Ready(None)
        }
        std::task::Poll::Pending => std::task::Poll::Pending,
    })
}
