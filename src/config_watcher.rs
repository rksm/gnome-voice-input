use eyre::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct ConfigWatcher {
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
