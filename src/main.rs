#[macro_use]
extern crate tracing;

use anyhow::Result;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use std::sync::Arc;
use tokio::sync::Mutex;
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

    let app_state = AppState {
        config: config.clone(),
        recording: Arc::new(Mutex::new(false)),
        transcriber,
    };

    let _hotkey_manager = hotkey::setup_hotkeys(&config)?;

    tray::create_tray(app_state.clone());

    let app_state_hotkey = app_state.clone();
    tokio::spawn(async move {
        loop {
            if let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
                if event.state == HotKeyState::Pressed {
                    info!("Hotkey pressed");
                    toggle_recording(app_state_hotkey.clone()).await;
                }
            }
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("Shutting down GNOME Voice Input");

    // Tray service will be cleaned up automatically
    Ok(())
}

async fn toggle_recording(app_state: AppState) {
    let mut recording = app_state.recording.lock().await;
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
        if let Err(e) = audio::capture_audio(audio_tx, app_state_audio.recording.clone()) {
            error!("Audio capture error: {}", e);
        }
    });

    let mut transcription_rx = app_state.transcriber.transcribe_stream(audio_rx).await?;

    while let Some(text) = transcription_rx.recv().await {
        if !text.trim().is_empty() {
            info!("Transcribed: {}", text);
            keyboard::type_text(&text)?;
        }

        let recording = app_state.recording.lock().await;
        if !*recording {
            break;
        }
    }

    Ok(())
}
