use crate::{config::Config, hotkey, state::AppState, tray};
use eyre::Result;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Represents all the running components of the application that need to be
/// managed during lifecycle events (startup, reload, shutdown)
pub struct AppComponents {
    pub hotkey_manager: Arc<tokio::sync::Mutex<GlobalHotKeyManager>>,
    pub registered_hotkey: Arc<tokio::sync::Mutex<HotKey>>,
    pub hotkey_handle: JoinHandle<()>,
    pub hotkey_rx_handle: JoinHandle<()>,
    pub tray_handle: Option<std::thread::JoinHandle<()>>,
    /// The shutdown token used for these components (child of main token)
    pub components_shutdown_token: CancellationToken,
}

impl AppComponents {
    /// Tears down all components gracefully
    /// Only tears down components, does NOT cancel the main app shutdown token
    pub async fn teardown_for_reload(self) -> Result<()> {
        info!("Tearing down application components for reload");

        // Cancel only the components' shutdown token, not the main app token
        self.components_shutdown_token.cancel();

        // Wait for async tasks to complete
        let shutdown_timeout = tokio::time::timeout(tokio::time::Duration::from_secs(3), async {
            let _ = self.hotkey_handle.await;
            let _ = self.hotkey_rx_handle.await;

            // Wait for the tray thread
            if let Some(handle) = self.tray_handle {
                let tray_result = tokio::task::spawn_blocking(move || handle.join()).await;

                match tray_result {
                    Ok(Ok(())) => info!("Tray thread joined successfully"),
                    Ok(Err(_)) => warn!("Tray thread panicked during teardown"),
                    Err(e) => warn!("Failed to join tray thread: {}", e),
                }
            }
        })
        .await;

        match shutdown_timeout {
            Ok(_) => {
                info!("All components torn down gracefully");
            }
            Err(_) => {
                warn!("Some components did not shut down within timeout");
            }
        }

        // Unregister hotkey
        let manager = self.hotkey_manager.lock().await;
        let hotkey = self.registered_hotkey.lock().await;
        if let Err(e) = manager.unregister(*hotkey) {
            warn!("Failed to unregister hotkey during teardown: {}", e);
        } else {
            info!("Hotkey unregistered successfully");
        }

        Ok(())
    }
}

/// Initialize all application components with the given configuration
/// Uses a child token of the main shutdown token so components can be torn down independently
pub async fn initialize_app_components(
    config: Config,
    app_state: AppState,
    parent_shutdown_token: &CancellationToken,
) -> Result<AppComponents> {
    info!("Initializing application components");

    // Create a child token for these components
    let components_shutdown_token = parent_shutdown_token.child_token();

    // Setup hotkeys
    let (hotkey_manager, registered_hotkey) = hotkey::setup_hotkeys(&config)?;
    info!("Hotkey registered: {:?}", registered_hotkey);

    // Setup tray with the child token
    let tray_handle = tray::setup_tray(&config, app_state.clone(), &components_shutdown_token);

    // Convert to Arc for sharing
    let hotkey_manager_arc = Arc::new(tokio::sync::Mutex::new(hotkey_manager));
    let registered_hotkey_arc = Arc::new(tokio::sync::Mutex::new(registered_hotkey));

    // Setup hotkey handlers with the child token
    let (hotkey_handle, hotkey_rx_handle) =
        hotkey::setup_hotkey_handlers(app_state.clone(), &components_shutdown_token);

    Ok(AppComponents {
        hotkey_manager: hotkey_manager_arc,
        registered_hotkey: registered_hotkey_arc,
        hotkey_handle,
        hotkey_rx_handle,
        tray_handle,
        components_shutdown_token,
    })
}

/// Reload the application with a new configuration
/// This tears down all components except the config watcher and rebuilds them
pub async fn reload_application(
    new_config: Config,
    app_state: &AppState,
    current_components: AppComponents,
    parent_shutdown_token: &CancellationToken,
) -> Result<AppComponents> {
    info!("Starting application reload");

    // Stop recording if active before teardown
    app_state
        .recording
        .store(false, std::sync::atomic::Ordering::Relaxed);

    // First, teardown existing components (but don't cancel main shutdown token)
    current_components.teardown_for_reload().await?;

    // Update the app state with new config
    {
        let mut config = app_state.config.write().unwrap();
        *config = new_config.clone();
    }

    // Update transcriber with new config
    let new_transcriber = Arc::new(crate::transcription::Transcriber::new(
        new_config.deepgram_api_key.clone(),
        new_config.transcription.clone(),
        app_state.debug,
    ));
    {
        let mut transcriber = app_state.transcriber.write().unwrap();
        *transcriber = new_transcriber;
    }

    // Re-initialize all components with the new configuration
    // Use the main shutdown token as parent so they respond to app shutdown
    match initialize_app_components(new_config, app_state.clone(), parent_shutdown_token).await {
        Ok(components) => {
            info!("Application reload completed successfully");
            Ok(components)
        }
        Err(e) => {
            error!("Failed to reload application components: {}", e);
            Err(e)
        }
    }
}
