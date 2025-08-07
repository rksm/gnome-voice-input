#[macro_use]
extern crate tracing;

use cpal::traits::StreamTrait;
use eyre::Result;
use gnome_voice_input::audio_utils::{init_simple_audio_capture, process_simple_audio};
use gnome_voice_input::{
    process_transcription_with_handler, AppState, Config, ConsoleTranscriptionHandler,
};
use std::env;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

fn main() -> Result<()> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        println!("Starting simple transcriber...");
        println!("Press Ctrl+C to stop");

        // Create config from environment or default
        let config = create_simple_config()?;

        // Create shared app state
        let shutdown_token = CancellationToken::new();
        let app_state = AppState::new(config, true, None, shutdown_token.clone());

        // Handle Ctrl+C
        let shutdown_for_signal = shutdown_token.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.unwrap();
            println!("\nStopping transcription...");
            shutdown_for_signal.cancel();
        });

        // Start recording using shared state and custom output handler
        app_state.recording.store(true, Ordering::Relaxed);

        if let Err(e) = start_transcription_only(app_state.clone()).await {
            error!("Transcription failed: {}", e);
        }

        println!("Transcription stopped.");
        Ok(())
    })
}

/// Create a simple config for the example
fn create_simple_config() -> Result<Config> {
    // Try to get API key from environment, or use a placeholder for testing config creation
    let api_key = env::var("DEEPGRAM_API_KEY").unwrap_or_else(|_| {
        println!(
            "Warning: DEEPGRAM_API_KEY not set. Using placeholder - transcription will not work."
        );
        "placeholder".to_string()
    });

    let mut config = Config {
        deepgram_api_key: api_key,
        ..Default::default()
    };

    // Enable interim results for more interactive experience
    config.transcription.use_interim_results = true;

    Ok(config)
}

/// Start transcription using shared state but with stdout output instead of keyboard typing
async fn start_transcription_only(app_state: AppState) -> Result<()> {
    debug!("Starting transcription-only process");
    let (audio_tx, audio_rx) = tokio::sync::mpsc::channel(100);

    // Initialize audio capture - this will fail if device doesn't support 16kHz
    let audio_capture = init_simple_audio_capture()?;

    // Start audio capture in blocking task
    let recording = app_state.recording.clone();
    let shutdown_token = app_state.shutdown_token.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = process_simple_audio(audio_capture.sample_rx, audio_tx, recording) {
            error!("Audio capture error: {}", e);
        }
    });

    // Start the audio stream
    audio_capture.stream.play()?;

    debug!("Creating transcription stream with 16000 Hz sample rate");
    let transcriber = app_state.transcriber.read().unwrap().clone();
    let transcription_rx = transcriber.transcribe_stream(audio_rx, 16000).await?;
    debug!("Transcription stream created, waiting for transcriptions");

    let handler = ConsoleTranscriptionHandler::new();

    tokio::select! {
        result = process_transcription_with_handler(transcription_rx, handler) => {
            if let Err(e) = result {
                error!("Transcription processing error: {}", e);
            }
        }
        _ = shutdown_token.cancelled() => {
            debug!("Shutdown requested, breaking transcription loop");
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
