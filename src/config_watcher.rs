use crate::{config::Config, hotkey, state::AppState, transcription};
use eyre::Result;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

pub(crate) struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    _config_path: PathBuf,
}

impl ConfigWatcher {
    pub fn new(
        config_path: PathBuf,
        reload_tx: mpsc::Sender<()>,
        _shutdown_token: CancellationToken,
    ) -> Result<Self> {
        let config_path_clone = config_path.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    // Only react to modify and create events on the config file
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            if event.paths.iter().any(|p| p == &config_path_clone) {
                                info!("Config file changed, triggering reload");
                                let _ = reload_tx.blocking_send(());
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => error!("File watcher error: {}", e),
            }
        })?;

        // Watch the parent directory to catch file replacements (common with editors)
        if let Some(parent) = config_path.parent() {
            watcher.watch(parent, RecursiveMode::NonRecursive)?;
            info!("Watching config directory: {}", parent.display());
        }

        // Also watch the file directly
        if config_path.exists() {
            watcher.watch(&config_path, RecursiveMode::NonRecursive)?;
            info!("Watching config file: {}", config_path.display());
        }

        Ok(Self {
            _watcher: watcher,
            _config_path: config_path,
        })
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        info!("Stopping config file watcher");
    }
}

pub fn setup_config_reload_handler(
    config_path: PathBuf,
    app_state: AppState,
    hotkey_manager_arc: Arc<tokio::sync::Mutex<GlobalHotKeyManager>>,
    registered_hotkey_arc: Arc<tokio::sync::Mutex<HotKey>>,
    shutdown_token: &CancellationToken,
) -> Result<(tokio::task::JoinHandle<()>, ConfigWatcher)> {
    let (config_reload_tx, mut config_reload_rx) = tokio::sync::mpsc::channel(10);
    let config_watcher =
        ConfigWatcher::new(config_path, config_reload_tx, shutdown_token.child_token())?;

    let handle = tokio::spawn(async move {
        while let Some(()) = config_reload_rx.recv().await {
            info!("Reloading configuration...");

            match Config::load(app_state.custom_config_path.clone()) {
                Ok(new_config) => {
                    {
                        let mut config = app_state.config.write().unwrap();
                        *config = new_config.clone();
                    }

                    let new_transcriber = Arc::new(transcription::Transcriber::new(
                        new_config.deepgram_api_key.clone(),
                        new_config.transcription.clone(),
                        app_state.debug,
                    ));
                    {
                        let mut transcriber = app_state.transcriber.write().unwrap();
                        *transcriber = new_transcriber;
                    }

                    match hotkey::setup_hotkeys(&new_config) {
                        Ok((new_manager, new_hotkey)) => {
                            let mut manager = hotkey_manager_arc.lock().await;
                            let mut hotkey = registered_hotkey_arc.lock().await;

                            if let Err(e) = manager.unregister(*hotkey) {
                                warn!("Failed to unregister old hotkey: {}", e);
                            }

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

    Ok((handle, config_watcher))
}
