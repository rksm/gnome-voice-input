#[macro_use]
extern crate tracing;

#[macro_use]
extern crate eyre;

use clap::Parser;
use eyre::Result;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod audio;
mod config;
mod config_watcher;
mod hotkey;
mod keyboard;
mod transcription;
mod tray;

use config::Config;
use config_watcher::ConfigWatcher;
use std::sync::RwLock;
use transcription::TranscriptionResult;

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

#[derive(Clone)]
pub struct AppState {
    config: Arc<RwLock<Config>>,
    recording: Arc<AtomicBool>,
    transcriber: Arc<RwLock<Arc<transcription::Transcriber>>>,
    shutdown_token: CancellationToken,
    debug: bool,
    custom_config_path: Option<std::path::PathBuf>,
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

    let config = Config::load(args.config.clone())?;
    let config_path = Config::get_config_path(args.config.clone())?;

    let transcriber = Arc::new(transcription::Transcriber::new(
        config.deepgram_api_key.clone(),
        config.transcription.clone(),
        args.debug,
    ));

    let shutdown_token = CancellationToken::new();

    let app_state = AppState {
        config: Arc::new(RwLock::new(config.clone())),
        recording: Arc::new(AtomicBool::new(false)),
        transcriber: Arc::new(RwLock::new(transcriber)),
        shutdown_token: shutdown_token.clone(),
        debug: args.debug,
        custom_config_path: args.config.clone(),
    };

    let (hotkey_manager, registered_hotkey) = hotkey::setup_hotkeys(&config)?;

    // Try to create tray if enabled in config
    let tray_handle = if config.ui.show_tray_icon {
        match tray::create_tray(app_state.clone(), config.clone()) {
            Ok(Some(tray)) => {
                info!("System tray service started successfully");
                // Run the tray service in a separate thread
                let tray_shutdown_token = shutdown_token.child_token();
                Some(std::thread::spawn(move || {
                    info!("Starting tray service thread");

                    // Run the tray service with periodic checks for shutdown
                    let handle = tray.handle();
                    std::thread::spawn(move || {
                        while !tray_shutdown_token.is_cancelled() {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        info!("Tray shutdown requested, stopping service");
                        handle.shutdown();
                    });

                    match tray.run() {
                        Ok(()) => info!("Tray service thread completed gracefully"),
                        Err(e) => error!("Tray service error: {}", e),
                    }
                }))
            }
            Ok(None) => {
                warn!("System tray service not available - app will continue without tray icon");
                None
            }
            Err(e) => {
                warn!("Failed to create system tray: {}", e);
                warn!("The app will continue to work via hotkey (Super+V)");
                None
            }
        }
    } else {
        info!("System tray icon disabled in configuration");
        None
    };

    // Set up config file watcher
    let (config_reload_tx, mut config_reload_rx) = tokio::sync::mpsc::channel(10);
    let _config_watcher = ConfigWatcher::new(
        config_path.clone(),
        config_reload_tx,
        shutdown_token.child_token(),
    )?;

    // Spawn config reload handler
    let app_state_reload = app_state.clone();
    let hotkey_manager_arc = Arc::new(tokio::sync::Mutex::new(hotkey_manager));
    let registered_hotkey_arc = Arc::new(tokio::sync::Mutex::new(registered_hotkey));

    let hotkey_manager_arc_clone = hotkey_manager_arc.clone();
    let registered_hotkey_arc_clone = registered_hotkey_arc.clone();

    let config_reload_handle = tokio::spawn(async move {
        while let Some(()) = config_reload_rx.recv().await {
            info!("Reloading configuration...");

            // Load new config
            match Config::load(app_state_reload.custom_config_path.clone()) {
                Ok(new_config) => {
                    // Update config
                    {
                        let mut config = app_state_reload.config.write().unwrap();
                        *config = new_config.clone();
                    }

                    // Recreate transcriber with new config
                    let new_transcriber = Arc::new(transcription::Transcriber::new(
                        new_config.deepgram_api_key.clone(),
                        new_config.transcription.clone(),
                        app_state_reload.debug,
                    ));
                    {
                        let mut transcriber = app_state_reload.transcriber.write().unwrap();
                        *transcriber = new_transcriber;
                    }

                    // Re-register hotkey if changed
                    match hotkey::setup_hotkeys(&new_config) {
                        Ok((new_manager, new_hotkey)) => {
                            let mut manager = hotkey_manager_arc_clone.lock().await;
                            let mut hotkey = registered_hotkey_arc_clone.lock().await;

                            // Unregister old hotkey
                            if let Err(e) = manager.unregister(*hotkey) {
                                warn!("Failed to unregister old hotkey: {}", e);
                            }

                            // Update with new hotkey
                            *manager = new_manager;
                            *hotkey = new_hotkey;

                            info!("Configuration reloaded successfully");
                        }
                        Err(e) => {
                            error!("Failed to setup new hotkeys: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to reload config: {}", e);
                }
            }
        }
    });

    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel(10);
    let hotkey_shutdown_token = shutdown_token.child_token();

    // Use tokio's spawn_blocking for the hotkey handler
    let hotkey_handle = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Handle::current();

        loop {
            if hotkey_shutdown_token.is_cancelled() {
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
    let hotkey_rx_shutdown_token = shutdown_token.child_token();
    let hotkey_rx_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(()) = hotkey_rx.recv() => {
                    toggle_recording(app_state_hotkey.clone()).await;
                }
                _ = hotkey_rx_shutdown_token.cancelled() => {
                    info!("Hotkey receiver shutting down");
                    break;
                }
            }
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("Shutting down GNOME Voice Input");

    // Stop any ongoing recording
    app_state.recording.store(false, Ordering::Relaxed);

    // Signal all components to shut down
    shutdown_token.cancel();

    // Wait for tasks to complete with a timeout
    let shutdown_timeout = tokio::time::timeout(tokio::time::Duration::from_secs(3), async {
        // Wait for all async tasks to complete
        let _ = hotkey_handle.await;
        let _ = hotkey_rx_handle.await;
        let _ = config_reload_handle.await;

        // Wait for tray thread if it exists
        if let Some(handle) = tray_handle {
            tokio::task::spawn_blocking(move || {
                let _ = handle.join();
            })
            .await
            .ok();
        }
    })
    .await;

    match shutdown_timeout {
        Ok(_) => {
            info!("All tasks shut down gracefully");
        }
        Err(_) => {
            warn!("Some tasks did not shut down within timeout, forcing exit");
        }
    }

    // Unregister hotkeys before exiting
    let manager = hotkey_manager_arc.lock().await;
    let hotkey = registered_hotkey_arc.lock().await;
    if let Err(e) = manager.unregister(*hotkey) {
        warn!("Failed to unregister hotkey: {}", e);
    } else {
        info!("Hotkey unregistered successfully");
    }

    Ok(())
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

    let audio_config = app_state.config.read().unwrap().audio.clone();
    let app_state_audio = app_state.clone();
    tokio::task::spawn_blocking(move || {
        debug!("Audio capture task started");
        if let Err(e) = audio::capture_audio(
            audio_tx,
            app_state_audio.recording.clone(),
            app_state_audio.shutdown_token.child_token(),
            audio_config,
        ) {
            error!("Audio capture error: {}", e);
        }
        debug!("Audio capture task ended");
    });

    debug!("Creating transcription stream");
    let transcriber = app_state.transcriber.read().unwrap().clone();
    let mut transcription_rx = transcriber.transcribe_stream(audio_rx).await?;
    debug!("Transcription stream created, waiting for transcriptions");

    let use_interim_results = app_state
        .config
        .read()
        .unwrap()
        .transcription
        .use_interim_results;
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
