use crate::{
    app_manager::{reload_application, AppComponents},
    config::Config,
    state::AppState,
};
use eyre::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{timeout, Instant};
use tokio_util::sync::CancellationToken;

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
    initial_components: AppComponents,
    shutdown_token: &CancellationToken,
) -> Result<(tokio::task::JoinHandle<()>, ConfigWatcher)> {
    let (config_reload_tx, mut config_reload_rx) = tokio::sync::mpsc::channel(10);
    let config_watcher =
        ConfigWatcher::new(config_path, config_reload_tx, shutdown_token.child_token())?;

    let shutdown_token_clone = shutdown_token.child_token();

    // Wrap components in Arc<Mutex> to allow updates during reload
    let components = Arc::new(Mutex::new(Some(initial_components)));

    let handle = tokio::spawn(async move {
        let mut last_reload = Instant::now();
        const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

        loop {
            tokio::select! {
                _ = shutdown_token_clone.cancelled() => {
                    info!("Config reload handler shutting down");

                    // Take ownership of components and teardown
                    let mut components_guard = components.lock().await;
                    if let Some(app_components) = components_guard.take() {
                        info!("Tearing down components during shutdown");
                        if let Err(e) = app_components.teardown_for_reload().await {
                            error!("Error tearing down components during shutdown: {}", e);
                        }
                    }

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
                            // Take current components
                            let mut components_guard = components.lock().await;
                            if let Some(current_components) = components_guard.take() {
                                // Reload application with new config
                                // Pass the main shutdown token so reloaded components respond to app shutdown
                                match reload_application(new_config, &app_state, current_components, &shutdown_token_clone).await {
                                    Ok(new_components) => {
                                        // Store new components
                                        *components_guard = Some(new_components);
                                        info!("Configuration and application reloaded successfully");
                                    }
                                    Err(e) => {
                                        error!("Failed to reload application: {}", e);
                                        error!("Application components have been torn down. Manual restart required.");
                                        // At this point the app is in a broken state
                                        // We could try to recover by loading the old config
                                        // but for now we'll just log the error
                                    }
                                }
                            } else {
                                error!("No components available for reload");
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
