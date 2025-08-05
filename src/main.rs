#[macro_use]
extern crate tracing;

use anyhow::Result;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod audio;
mod config;
mod hotkey;
mod keyboard;
mod transcription;
mod tray;

use config::Config;

#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)]
    config: Config,
    recording: Arc<Mutex<bool>>,
    transcriber: Arc<transcription::Transcriber>,
    shutdown: Arc<AtomicBool>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gnome_voice_input=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting GNOME Voice Input");

    let config = Config::load()?;
    let transcriber = Arc::new(transcription::Transcriber::new(
        config.deepgram_api_key.clone(),
    ));

    let shutdown = Arc::new(AtomicBool::new(false));

    let app_state = AppState {
        config: config.clone(),
        recording: Arc::new(Mutex::new(false)),
        transcriber,
        shutdown: shutdown.clone(),
    };

    let _hotkey_manager = hotkey::setup_hotkeys(&config)?;

    tray::create_tray(app_state.clone());

    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel(10);
    let shutdown_hotkey = shutdown.clone();

    std::thread::spawn(move || loop {
        if shutdown_hotkey.load(Ordering::Relaxed) {
            info!("Hotkey handler shutting down");
            break;
        }

        match GlobalHotKeyEvent::receiver().recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(event) => {
                if event.state == HotKeyState::Pressed {
                    info!("Hotkey pressed");
                    if hotkey_tx.blocking_send(()).is_err() {
                        break;
                    }
                }
            }
            Err(_) => continue,
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
    {
        let mut recording = app_state.recording.lock().unwrap();
        *recording = false;
    }

    // Signal all components to shut down
    shutdown.store(true, Ordering::Relaxed);

    // Give components time to shut down gracefully
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Force exit if components haven't shut down
    std::process::exit(0);
}

async fn toggle_recording(app_state: AppState) {
    let mut recording = app_state.recording.lock().unwrap();
    *recording = !*recording;

    if *recording {
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
    let (audio_tx, audio_rx) = tokio::sync::mpsc::channel(100);

    let app_state_audio = app_state.clone();
    std::thread::spawn(move || {
        if let Err(e) = audio::capture_audio(
            audio_tx,
            app_state_audio.recording.clone(),
            app_state_audio.shutdown.clone(),
        ) {
            error!("Audio capture error: {}", e);
        }
    });

    let mut transcription_rx = app_state.transcriber.transcribe_stream(audio_rx).await?;

    while let Some(text) = transcription_rx.recv().await {
        if !text.trim().is_empty() {
            info!("Transcribed: {}", text);
            keyboard::type_text(&text)?;
        }

        let recording = app_state.recording.lock().unwrap();
        if !*recording {
            break;
        }
    }

    Ok(())
}
