use crate::config::AudioConfig;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use eyre::{OptionExt, Result, WrapErr};
use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapRb,
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info};

pub fn capture_audio(
    audio_tx: mpsc::Sender<Vec<u8>>,
    recording: Arc<Mutex<bool>>,
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

    let (mut producer, mut consumer) = HeapRb::<f32>::new(8192).split();

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

    let mut buffer = Vec::with_capacity(1024);

    loop {
        if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            info!("Audio capture shutting down");
            break;
        }

        let is_recording = recording.lock().unwrap();
        if !*is_recording {
            break;
        }
        drop(is_recording);

        while let Some(sample) = consumer.try_pop() {
            let bytes = sample.to_le_bytes();
            buffer.extend_from_slice(&bytes);

            if buffer.len() >= 1024 {
                if audio_tx.blocking_send(buffer.clone()).is_err() {
                    break;
                }
                buffer.clear();
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
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
