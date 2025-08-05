use crate::{config::Config, hotkey, state::AppState, transcription};
use eyre::Result;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{timeout, Instant};
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

    let shutdown_token_clone = shutdown_token.child_token();
    let handle = tokio::spawn(async move {
        let mut last_reload = Instant::now();
        const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

        loop {
            tokio::select! {
                _ = shutdown_token_clone.cancelled() => {
                    info!("Config reload handler shutting down");
                    break;
                }
                Some(()) = config_reload_rx.recv() => {
                    // Debounce: ignore events that come too quickly after the last reload
                    let now = Instant::now();
                    if now.duration_since(last_reload) < DEBOUNCE_DURATION {
                        // Drain any additional events that might be queued
                        while let Ok(Some(())) = timeout(Duration::from_millis(50), config_reload_rx.recv()).await {
                            // Just consume the events
                        }
                        continue;
                    }

                    info!("Reloading configuration...");
                    last_reload = now;

                    match Config::load(app_state.custom_config_path.clone()) {
                        Ok(new_config) => {
                            // Update config
                            {
                                let mut config = app_state.config.write().unwrap();
                                *config = new_config.clone();
                            }

                            // Update transcriber
                            let new_transcriber = Arc::new(transcription::Transcriber::new(
                                new_config.deepgram_api_key.clone(),
                                new_config.transcription.clone(),
                                app_state.debug,
                            ));
                            {
                                let mut transcriber = app_state.transcriber.write().unwrap();
                                *transcriber = new_transcriber;
                            }

                            // Update hotkey - reuse existing manager
                            match hotkey::parse_hotkey(&new_config) {
                                Ok(new_hotkey) => {
                                    let manager = hotkey_manager_arc.lock().await;
                                    let mut old_hotkey = registered_hotkey_arc.lock().await;

                                    // Only update if the hotkey actually changed
                                    if *old_hotkey != new_hotkey {
                                        // Unregister old hotkey
                                        if let Err(e) = manager.unregister(*old_hotkey) {
                                            warn!("Failed to unregister old hotkey: {}", e);
                                        }

                                        // Register new hotkey with the same manager
                                        match manager.register(new_hotkey) {
                                            Ok(()) => {
                                                *old_hotkey = new_hotkey;
                                                info!("Hotkey updated successfully");
                                            }
                                            Err(e) => {
                                                error!("Failed to register new hotkey: {}", e);
                                                // Try to re-register the old hotkey
                                                if let Err(e) = manager.register(*old_hotkey) {
                                                    error!("Failed to restore old hotkey: {}", e);
                                                }
                                            }
                                        }
                                    } else {
                                        info!("Hotkey unchanged, skipping update");
                                    }

                                    info!("Configuration reloaded successfully");
                                }
                                Err(e) => {
                                    error!("Failed to parse new hotkey: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to reload config: {}", e);
                        }
                    }
                }
            }
        }
    });

    Ok((handle, config_watcher))
}
