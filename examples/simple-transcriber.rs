#[macro_use]
extern crate tracing;

use cpal::traits::StreamTrait;
use deepgram::{
    common::options::{Encoding, Language, Model, Options},
    Deepgram,
};
use eyre::{Result, WrapErr};
use futures::stream::StreamExt;
use gnome_voice_input::audio_utils::{
    create_audio_stream, init_simple_audio_capture, process_simple_audio,
};
use gnome_voice_input::transcription_utils::{handle_simple_response, TranscriptionResult};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

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

        // Initialize audio capture
        let audio_capture = init_simple_audio_capture()?;

        // Start audio capture in blocking task
        let recording_audio = recording.clone();
        tokio::task::spawn_blocking(move || {
            if let Err(e) = process_simple_audio(audio_capture.sample_rx, audio_tx, recording_audio)
            {
                warn!("Audio capture failed: {e}");
            }
        });

        // Start the audio stream
        audio_capture.stream.play()?;

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
                    if let Some(transcript_result) = handle_simple_response(response) {
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
