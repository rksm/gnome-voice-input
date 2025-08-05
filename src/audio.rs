use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapRb,
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info};

pub fn capture_audio(audio_tx: mpsc::Sender<Vec<u8>>, recording: Arc<Mutex<bool>>) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("No input device available")?;

    info!("Using input device: {}", device.name()?);

    let mut supported_configs_range = device
        .supported_input_configs()
        .context("Failed to get supported configs")?;

    let supported_config = supported_configs_range
        .next()
        .context("No supported config")?
        .with_max_sample_rate();

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
            anyhow::bail!("Unsupported sample format: U16");
        }
        _ => anyhow::bail!("Unsupported sample format"),
    };

    stream.play()?;

    let runtime = tokio::runtime::Runtime::new()?;

    runtime.block_on(async {
        let mut buffer = Vec::with_capacity(1024);

        loop {
            let is_recording = recording.lock().await;
            if !*is_recording {
                break;
            }
            drop(is_recording);

            while let Some(sample) = consumer.try_pop() {
                let bytes = sample.to_le_bytes();
                buffer.extend_from_slice(&bytes);

                if buffer.len() >= 1024 {
                    if audio_tx.send(buffer.clone()).await.is_err() {
                        return;
                    }
                    buffer.clear();
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    });

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
