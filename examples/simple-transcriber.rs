#[macro_use]
extern crate tracing;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use deepgram::{
    common::options::{Encoding, Language, Model, Options},
    Deepgram,
};
use eyre::{OptionExt, Result, WrapErr};
use futures::stream::{Stream, StreamExt};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum TranscriptionResult {
    Interim(String),
    Final(String),
}

fn main() -> Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    // Get Deepgram API key from environment
    let api_key =
        env::var("DEEPGRAM_API_KEY").wrap_err("DEEPGRAM_API_KEY environment variable not set")?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        println!("Starting simple transcriber...");
        println!("Press Ctrl+C to stop");

        let recording = Arc::new(AtomicBool::new(true));
        let recording_clone = recording.clone();

        // Handle Ctrl+C
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.unwrap();
            println!("\nStopping transcription...");
            recording_clone.store(false, Ordering::Relaxed);
        });

        // Create audio channel
        let (audio_tx, audio_rx) = mpsc::channel(100);

        // Start audio capture in blocking task
        let recording_audio = recording.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = capture_audio(audio_tx, recording_audio) {
                warn!("Audio capture failed: {e}");
            }
        });

        // Start transcription
        let client = Deepgram::new(&api_key).expect("Failed to create Deepgram client");

        let options = Options::builder()
            .language(Language::en)
            .model(Model::Nova3)
            .punctuate(true)
            .smart_format(true)
            .build();

        let audio_stream = create_audio_stream(audio_rx);

        let mut stream = client
            .transcription()
            .stream_request_with_options(options)
            .encoding(Encoding::Linear16)
            .sample_rate(16000)
            .channels(1)
            .keep_alive()
            .stream(audio_stream)
            .await?;

        println!("Transcription started. Speak into your microphone...\n");

        // Process transcription results
        while let Some(result) = stream.next().await {
            if !recording.load(Ordering::Relaxed) {
                break;
            }

            match result {
                Ok(response) => {
                    if let Some(transcript_result) = handle_response(response) {
                        match transcript_result {
                            TranscriptionResult::Interim(text) => {
                                info!("\rInterim: {}", text);
                                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                            }
                            TranscriptionResult::Final(text) => {
                                info!("\nFinal: {}", text);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Stream error: {:?}", e);
                }
            }
        }

        info!("\nTranscription stopped.");
        Ok(())
    })
}

fn capture_audio(audio_tx: mpsc::Sender<Vec<u8>>, recording: Arc<AtomicBool>) -> Result<()> {
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
        _ => {
            return Err(eyre::eyre!(
                "Unsupported sample format: {:?}",
                sample_format
            ))
        }
    };

    stream.play()?;

    // Process audio in chunks
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

// Convert mpsc::Receiver to a Stream that produces Result<Bytes, Error>
fn create_audio_stream(
    mut audio_rx: mpsc::Receiver<Vec<u8>>,
) -> impl Stream<Item = Result<bytes::Bytes, std::io::Error>> {
    futures::stream::poll_fn(move |cx| match audio_rx.poll_recv(cx) {
        std::task::Poll::Ready(Some(data)) => {
            std::task::Poll::Ready(Some(Ok(bytes::Bytes::from(data))))
        }
        std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
        std::task::Poll::Pending => std::task::Poll::Pending,
    })
}

fn handle_response(
    response: deepgram::common::stream_response::StreamResponse,
) -> Option<TranscriptionResult> {
    use deepgram::common::stream_response::StreamResponse;

    if let StreamResponse::TranscriptResponse {
        is_final, channel, ..
    } = response
    {
        if let Some(alternative) = channel.alternatives.into_iter().next() {
            let transcript = alternative.transcript.trim();
            if !transcript.is_empty() {
                return Some(if is_final {
                    TranscriptionResult::Final(transcript.to_string())
                } else {
                    TranscriptionResult::Interim(transcript.to_string())
                });
            }
        }
    }

    None
}
