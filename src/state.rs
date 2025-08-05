use crate::{config::Config, transcription};
use eyre::Result;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub recording: Arc<AtomicBool>,
    pub transcriber: Arc<RwLock<Arc<transcription::Transcriber>>>,
    pub shutdown_token: CancellationToken,
    pub debug: bool,
    pub custom_config_path: Option<std::path::PathBuf>,
}

impl AppState {
    pub(crate) fn new(
        config: Config,
        debug: bool,
        custom_config_path: Option<std::path::PathBuf>,
        shutdown_token: CancellationToken,
    ) -> Self {
        let transcriber = Arc::new(transcription::Transcriber::new(
            config.deepgram_api_key.clone(),
            config.transcription.clone(),
            debug,
        ));

        Self {
            config: Arc::new(RwLock::new(config)),
            recording: Arc::new(AtomicBool::new(false)),
            transcriber: Arc::new(RwLock::new(transcriber)),
            shutdown_token,
            debug,
            custom_config_path,
        }
    }
}

pub(crate) struct ShutdownHandles {
    pub(crate) hotkey_handle: tokio::task::JoinHandle<()>,
    pub(crate) hotkey_rx_handle: tokio::task::JoinHandle<()>,
    pub(crate) config_reload_handle: tokio::task::JoinHandle<()>,
    pub(crate) tray_handle: Option<std::thread::JoinHandle<()>>,
    pub(crate) hotkey_manager_arc: Arc<tokio::sync::Mutex<GlobalHotKeyManager>>,
    pub(crate) registered_hotkey_arc: Arc<tokio::sync::Mutex<HotKey>>,
}

impl ShutdownHandles {
    pub(crate) async fn shutdown_app(
        self,
        app_state: AppState,
        shutdown_token: CancellationToken,
    ) -> Result<()> {
        info!("Shutting down GNOME Voice Input");

        app_state.recording.store(false, Ordering::Relaxed);
        shutdown_token.cancel();

        let shutdown_timeout = tokio::time::timeout(tokio::time::Duration::from_secs(5), async {
            // Wait for async tasks first
            let _ = self.hotkey_handle.await;
            let _ = self.hotkey_rx_handle.await;
            let _ = self.config_reload_handle.await;

            // Wait for the tray thread
            if let Some(handle) = self.tray_handle {
                let tray_result = tokio::task::spawn_blocking(move || handle.join()).await;

                match tray_result {
                    Ok(Ok(())) => info!("Tray thread joined successfully"),
                    Ok(Err(_)) => warn!("Tray thread panicked during shutdown"),
                    Err(e) => warn!("Failed to join tray thread: {}", e),
                }
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

        let manager = self.hotkey_manager_arc.lock().await;
        let hotkey = self.registered_hotkey_arc.lock().await;
        if let Err(e) = manager.unregister(*hotkey) {
            warn!("Failed to unregister hotkey: {}", e);
        } else {
            info!("Hotkey unregistered successfully");
        }

        Ok(())
    }
}
