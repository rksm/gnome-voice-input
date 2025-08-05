#[macro_use]
extern crate tracing;

#[macro_use]
extern crate eyre;

use clap::Parser;
use eyre::Result;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod app_manager;
mod audio;
mod config;
mod config_watcher;
mod hotkey;
mod keyboard;
mod state;
mod transcription;
mod tray;

use app_manager::initialize_app_components;
use config::Config;
use state::AppState;

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

    // Initialize all application components
    let components =
        initialize_app_components(config.clone(), app_state.clone(), &shutdown_token).await?;

    // Setup config watcher with access to components for reload
    let (config_reload_handle, _config_watcher) = config_watcher::setup_config_reload_handler(
        config_path,
        app_state.clone(),
        components,
        &shutdown_token,
    )?;

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    info!("Shutting down GNOME Voice Input");
    shutdown_token.cancel();

    // Wait for config reload handler to finish
    let _ = config_reload_handle.await;

    Ok(())
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
