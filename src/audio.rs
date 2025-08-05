use crate::config::AudioConfig;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use eyre::{OptionExt, Result, WrapErr};
use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapRb,
};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;

pub fn capture_audio(
    audio_tx: mpsc::Sender<Vec<u8>>,
    recording: Arc<AtomicBool>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    audio_config: AudioConfig,
) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_eyre("No input device available")?;

    info!("Using input device: {}", device.name()?);
    info!(
        "Requested config: {} channels, {} Hz",
        audio_config.channels, audio_config.sample_rate
    );

    let supported_configs_range = device
        .supported_input_configs()
        .wrap_err("Failed to get supported configs")?;

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

    // Use smaller buffer for lower latency streaming
    let (mut producer, mut consumer) = HeapRb::<f32>::new(4096).split();

    let stream = match sample_format {
        SampleFormat::F32 => build_input_stream::<f32, _>(&device, &config, producer, err_fn)?,
        SampleFormat::I16 => {
            let (producer_i16, consumer_i16) = HeapRb::<i16>::new(8192).split();
            let stream = build_input_stream::<i16, _>(&device, &config, producer_i16, err_fn)?;

            std::thread::spawn(move || {
                let mut consumer_i16 = consumer_i16;
                loop {
                    while let Some(sample) = consumer_i16.try_pop() {
                        let normalized = sample.to_float_sample();
                        if producer.try_push(normalized).is_err() {
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });

            stream
        }
        SampleFormat::U16 => {
            let (producer_u16, consumer_u16) = HeapRb::<u16>::new(8192).split();
            let stream = build_input_stream::<u16, _>(&device, &config, producer_u16, err_fn)?;

            std::thread::spawn(move || {
                let mut consumer_u16 = consumer_u16;
                loop {
                    while let Some(sample) = consumer_u16.try_pop() {
                        let normalized = sample.to_float_sample();
                        if producer.try_push(normalized).is_err() {
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });

            stream
        }
        SampleFormat::U8 => {
            let (producer_u8, consumer_u8) = HeapRb::<u8>::new(8192).split();
            let stream = build_input_stream::<u8, _>(&device, &config, producer_u8, err_fn)?;

            std::thread::spawn(move || {
                let mut consumer_u8 = consumer_u8;
                loop {
                    while let Some(sample) = consumer_u8.try_pop() {
                        // Convert U8 (0-255) to f32 (-1.0 to 1.0)
                        let normalized = (sample as f32 / 128.0) - 1.0;
                        if producer.try_push(normalized).is_err() {
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });

            stream
        }
        SampleFormat::I32 => {
            let (producer_i32, consumer_i32) = HeapRb::<i32>::new(8192).split();
            let stream = build_input_stream::<i32, _>(&device, &config, producer_i32, err_fn)?;

            std::thread::spawn(move || {
                let mut consumer_i32 = consumer_i32;
                loop {
                    while let Some(sample) = consumer_i32.try_pop() {
                        let normalized = sample.to_float_sample();
                        if producer.try_push(normalized).is_err() {
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });

            stream
        }
        _ => bail!("Unsupported sample format: {:?}", sample_format),
    };

    stream.play()?;

    // Calculate samples per chunk based on config
    let samples_per_chunk =
        (audio_config.sample_rate * audio_config.audio_chunk_ms / 1000) as usize;

    // Buffer for collecting samples before conversion
    let mut sample_buffer = Vec::with_capacity(samples_per_chunk);
    let mut total_samples_sent = 0u64;
    let mut chunks_sent = 0u64;

    loop {
        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            info!("Audio capture shutting down");
            break;
        }

        if !recording.load(std::sync::atomic::Ordering::Relaxed) {
            debug!("Recording stopped in audio capture");
            break;
        }

        // Collect samples from the ring buffer
        let mut samples_collected = 0;
        while let Some(sample) = consumer.try_pop() {
            sample_buffer.push(sample);
            samples_collected += 1;

            // Send chunks based on configured size
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

                // Save first chunk for debugging
                if chunks_sent == 1 {
                    if let Err(e) = std::fs::write("debug_audio_chunk.raw", &i16_buffer) {
                        error!("Failed to save debug audio chunk: {}", e);
                    } else {
                        info!("Saved first audio chunk to debug_audio_chunk.raw for analysis");
                    }
                }

                if audio_tx.blocking_send(i16_buffer).is_err() {
                    info!("Audio receiver dropped, stopping capture");
                    break;
                }
                sample_buffer.clear();
            }
        }

        if samples_collected > 0 {
            debug!("Collected {} samples from ring buffer", samples_collected);
        }

        // Shorter sleep for more responsive streaming
        std::thread::sleep(std::time::Duration::from_millis(5));
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

fn build_input_stream<T, P>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut producer: P,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: Sample + Send + 'static + cpal::SizedSample,
    P: Producer<Item = T> + Send + 'static,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            for &sample in data {
                if producer.try_push(sample).is_err() {
                    break;
                }
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

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
