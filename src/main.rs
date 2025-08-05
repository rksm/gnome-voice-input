#[macro_use]
extern crate tracing;

#[macro_use]
extern crate eyre;

use clap::Parser;
use eyre::Result;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod audio;
mod config;
mod hotkey;
mod keyboard;
mod transcription;
mod tray;

use config::Config;
use transcription::TranscriptionResult;

#[derive(Parser, Debug)]
#[command(name = "gnome-voice-input")]
#[command(about = "Voice input utility for GNOME desktop using Deepgram", long_about = None)]
struct Args {
    /// Enable debug mode to save WAV files of audio sent to Deepgram
    #[arg(long, default_value_t = false)]
    debug: bool,
}

#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)]
    config: Config,
    recording: Arc<AtomicBool>,
    transcriber: Arc<transcription::Transcriber>,
    shutdown: Arc<AtomicBool>,
    #[allow(dead_code)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                if args.debug {
                    "gnome_voice_input=debug".into()
                } else {
                    "gnome_voice_input=info".into()
                }
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting GNOME Voice Input");
    if args.debug {
        info!("Debug mode enabled - will save WAV files to current directory");
    }

    let config = Config::load()?;
    let transcriber = Arc::new(transcription::Transcriber::new(
        config.deepgram_api_key.clone(),
        args.debug,
    ));

    let shutdown = Arc::new(AtomicBool::new(false));

    let app_state = AppState {
        config: config.clone(),
        recording: Arc::new(AtomicBool::new(false)),
        transcriber,
        shutdown: shutdown.clone(),
        debug: args.debug,
    };

    let _hotkey_manager = hotkey::setup_hotkeys(&config)?;

    // Try to create tray, but don't fail if it doesn't work
    match tray::create_tray(app_state.clone()) {
        Ok(Some(tray)) => {
            info!("System tray service started successfully");
            // Run the tray service in a separate thread
            std::thread::spawn(move || {
                let _ = tray.run();
            });
        }
        Ok(None) => {
            warn!("System tray service not available - app will continue without tray icon");
        }
        Err(e) => {
            warn!("Failed to create system tray: {}", e);
            warn!("The app will continue to work via hotkey (Super+V)");
        }
    }

    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel(10);
    let shutdown_hotkey = shutdown.clone();

    // Use tokio's spawn_blocking for the hotkey handler
    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Handle::current();

        loop {
            if shutdown_hotkey.load(Ordering::Relaxed) {
                info!("Hotkey handler shutting down");
                break;
            }

            match GlobalHotKeyEvent::receiver().recv_timeout(std::time::Duration::from_millis(100))
            {
                Ok(event) => {
                    if event.state == HotKeyState::Pressed {
                        info!("Hotkey pressed");
                        let tx = hotkey_tx.clone();
                        runtime.spawn(async move {
                            let _ = tx.send(()).await;
                        });
                    }
                }
                Err(_) => continue,
            }
        }
    });

    let app_state_hotkey = app_state.clone();
    tokio::spawn(async move {
        while let Some(()) = hotkey_rx.recv().await {
            toggle_recording(app_state_hotkey.clone()).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("Shutting down GNOME Voice Input");

    // Stop any ongoing recording
    app_state.recording.store(false, Ordering::Relaxed);

    // Signal all components to shut down
    shutdown.store(true, Ordering::Relaxed);

    // Give components time to shut down gracefully
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Force exit if components haven't shut down
    std::process::exit(0);
}

pub async fn toggle_recording(app_state: AppState) {
    let was_recording = app_state.recording.fetch_xor(true, Ordering::Relaxed);
    let is_recording = !was_recording;

    if is_recording {
        info!("Starting recording");
        let app_state_clone = app_state.clone();
        tokio::spawn(async move {
            if let Err(e) = start_recording(app_state_clone).await {
                error!("Recording error: {}", e);
            }
        });
    } else {
        info!("Stopping recording");
    }
}

async fn start_recording(app_state: AppState) -> Result<()> {
    debug!("Starting recording process");
    let (audio_tx, audio_rx) = tokio::sync::mpsc::channel(100);

    let app_state_audio = app_state.clone();
    tokio::task::spawn_blocking(move || {
        debug!("Audio capture task started");
        if let Err(e) = audio::capture_audio(
            audio_tx,
            app_state_audio.recording.clone(),
            app_state_audio.shutdown.clone(),
            app_state_audio.config.audio.clone(),
        ) {
            error!("Audio capture error: {}", e);
        }
        debug!("Audio capture task ended");
    });

    debug!("Creating transcription stream");
    let mut transcription_rx = app_state.transcriber.transcribe_stream(audio_rx).await?;
    debug!("Transcription stream created, waiting for transcriptions");

    let use_interim_results = app_state.config.transcription.use_interim_results;
    let mut last_interim_length = 0;

    while let Some(result) = transcription_rx.recv().await {
        match result {
            TranscriptionResult::Interim(text) => {
                debug!("Received interim transcription: '{}'", text);
                if use_interim_results && !text.trim().is_empty() {
                    // Delete previous interim text by sending backspaces
                    if last_interim_length > 0 {
                        for _ in 0..last_interim_length {
                            keyboard::press_key(enigo::Key::Backspace)?;
                        }
                    }

                    // Type new interim text
                    keyboard::type_text(&text)?;
                    last_interim_length = text.chars().count();
                }
            }
            TranscriptionResult::Final(text) => {
                debug!("Received final transcription: '{}'", text);
                if !text.trim().is_empty() {
                    // Delete previous interim text if any
                    if use_interim_results && last_interim_length > 0 {
                        for _ in 0..last_interim_length {
                            keyboard::press_key(enigo::Key::Backspace)?;
                        }
                        last_interim_length = 0;
                    }

                    info!("Final transcribed: {}", text);
                    keyboard::type_text(&text)?;

                    // Add a space after final transcription for better flow
                    keyboard::type_text(" ")?;
                }
            }
        }

        if !app_state.recording.load(Ordering::Relaxed) {
            debug!("Recording stopped, breaking loop");
            break;
        }
    }

    debug!("Transcription loop ended");
    Ok(())
}
