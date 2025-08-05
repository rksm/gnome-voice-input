#[macro_use]
extern crate tracing;

#[macro_use]
extern crate eyre;

use clap::Parser;
use eyre::Result;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod audio;
mod config;
mod config_watcher;
mod hotkey;
mod keyboard;
mod state;
mod transcription;
mod tray;

use config::Config;
use state::{AppState, ShutdownHandles};

#[derive(Parser, Debug)]
#[command(name = "gnome-voice-input")]
#[command(about = "Voice input utility for GNOME desktop using Deepgram", long_about = None)]
struct Args {
    /// Enable debug mode to save WAV files of audio sent to Deepgram
    #[arg(long, default_value_t = false)]
    debug: bool,

    /// Path to custom configuration file
    #[arg(short, long, value_name = "FILE")]
    config: Option<std::path::PathBuf>,
}

fn init_logging(debug: bool) {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                if debug {
                    "gnome_voice_input=debug".into()
                } else {
                    "gnome_voice_input=info".into()
                }
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting GNOME Voice Input");
    if debug {
        info!("Debug mode enabled - will save WAV files to current directory");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let args = Args::parse();

    init_logging(args.debug);

    let config = Config::load(args.config.clone())?;
    let config_path = Config::get_config_path(args.config.clone())?;
    let shutdown_token = CancellationToken::new();

    let app_state = AppState::new(
        config.clone(),
        args.debug,
        args.config.clone(),
        shutdown_token.clone(),
    );

    let (hotkey_manager, registered_hotkey) = hotkey::setup_hotkeys(&config)?;

    let tray_handle = tray::setup_tray(&config, app_state.clone(), &shutdown_token);

    let hotkey_manager_arc = Arc::new(tokio::sync::Mutex::new(hotkey_manager));
    let registered_hotkey_arc = Arc::new(tokio::sync::Mutex::new(registered_hotkey));

    let (config_reload_handle, _config_watcher) = config_watcher::setup_config_reload_handler(
        config_path,
        app_state.clone(),
        hotkey_manager_arc.clone(),
        registered_hotkey_arc.clone(),
        &shutdown_token,
    )?;

    let (hotkey_handle, hotkey_rx_handle) =
        hotkey::setup_hotkey_handlers(app_state.clone(), &shutdown_token);

    tokio::signal::ctrl_c().await?;

    let handles = ShutdownHandles {
        hotkey_handle,
        hotkey_rx_handle,
        config_reload_handle,
        tray_handle,
        hotkey_manager_arc,
        registered_hotkey_arc,
    };

    handles.shutdown_app(app_state, shutdown_token).await
}

pub async fn toggle_recording(app_state: AppState) {
    let was_recording = app_state.recording.fetch_xor(true, Ordering::Relaxed);
    let is_recording = !was_recording;

    if is_recording {
        info!("Starting recording");
        let app_state_clone = app_state.clone();
        tokio::spawn(async move {
            if let Err(e) = audio::start_recording(app_state_clone).await {
                error!("Recording error: {}", e);
            }
        });
    } else {
        info!("Stopping recording");
    }
}
