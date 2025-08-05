use crate::{config::Config, state::AppState};
use dbus::blocking::Connection;
use ksni::{self, menu::StandardItem, MenuItem, Tray, TrayService};
use std::path::Path;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

struct VoiceInputTray {
    app_state: AppState,
    handle: Handle,
    config: Config,
}

/// Check if an icon exists in common icon theme directories
fn icon_exists(icon_name: &str) -> bool {
    let icon_dirs = vec![
        "/usr/share/icons/hicolor",
        "/usr/share/icons/Adwaita",
        "/usr/share/icons/gnome",
        "/usr/share/pixmaps",
    ];

    for dir in icon_dirs {
        // Check various common sizes and formats
        let patterns = vec![
            format!("{}/16x16/status/{}.png", dir, icon_name),
            format!("{}/22x22/status/{}.png", dir, icon_name),
            format!("{}/24x24/status/{}.png", dir, icon_name),
            format!("{}/48x48/status/{}.png", dir, icon_name),
            format!("{}/scalable/status/{}.svg", dir, icon_name),
            format!("{}/16x16/devices/{}.png", dir, icon_name),
            format!("{}/22x22/devices/{}.png", dir, icon_name),
            format!("{}/24x24/devices/{}.png", dir, icon_name),
            format!("{}/48x48/devices/{}.png", dir, icon_name),
            format!("{}/scalable/devices/{}.svg", dir, icon_name),
            format!("{}/{}.png", dir, icon_name),
            format!("{}/{}.svg", dir, icon_name),
        ];

        for pattern in patterns {
            if Path::new(&pattern).exists() {
                return true;
            }
        }
    }
    false
}

impl Tray for VoiceInputTray {
    fn title(&self) -> String {
        "Voice Input".to_string()
    }

    fn icon_name(&self) -> String {
        // Try multiple common icon names for better compatibility
        // First try specific microphone icons, then fallback to generic audio
        let icon_candidates = vec![
            "audio-input-microphone",
            "microphone",
            "audio-card",
            "media-record",
            "audio-x-generic",
            "application-x-executable",
        ];

        for icon in &icon_candidates {
            if icon_exists(icon) {
                info!("Using icon: {}", icon);
                return icon.to_string();
            }
        }

        // If no icon found, use a name that should exist
        warn!("No suitable icon found in system theme, using fallback");
        info!("Tried icons: {:?}", icon_candidates);
        "application-x-executable".to_string()
    }

    fn id(&self) -> String {
        "gnome-voice-input".to_string()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        use std::sync::atomic::Ordering;

        // Get current recording status
        let is_recording = self.app_state.recording.load(Ordering::Relaxed);
        let status_label = if is_recording {
            "🔴 Recording Active"
        } else {
            "⚪ Recording Inactive"
        };

        // Format the hotkey display string from config
        let hotkey_str = format!(
            "{} + {}",
            self.config
                .hotkey
                .modifiers
                .iter()
                .map(|m| {
                    // Capitalize first letter of modifier
                    let mut chars = m.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_uppercase().chain(chars).collect(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" + "),
            self.config.hotkey.key.to_uppercase()
        );

        vec![
            // Status indicator (non-interactive)
            StandardItem {
                label: status_label.to_string(),
                icon_name: if is_recording {
                    "media-record".to_string()
                } else {
                    "media-playback-stop".to_string()
                },
                activate: Box::new(|_tray: &mut Self| {
                    // Non-interactive, do nothing
                }),
                enabled: false, // Disabled makes it non-interactive
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: format!("Toggle Recording ({hotkey_str})"),
                icon_name: "media-record".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    info!("Toggle recording requested from tray menu");
                    let app_state = tray.app_state.clone();
                    tray.handle.spawn(async move {
                        crate::toggle_recording(app_state).await;
                    });
                }),
                enabled: true,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    info!("Quit requested from tray");
                    tray.app_state.shutdown_token.cancel();
                }),
                enabled: true,
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Check if StatusNotifierWatcher is available on D-Bus
fn check_status_notifier_support() -> bool {
    match Connection::new_session() {
        Ok(conn) => {
            let proxy = conn.with_proxy(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                Duration::from_millis(500),
            );

            // Check if StatusNotifierWatcher service is available
            let result: Result<(Vec<String>,), _> =
                proxy.method_call("org.freedesktop.DBus", "ListNames", ());

            match result {
                Ok((names,)) => {
                    let has_watcher = names.iter().any(|name| {
                        name == "org.kde.StatusNotifierWatcher"
                            || name.contains("StatusNotifierWatcher")
                    });

                    if !has_watcher {
                        warn!("StatusNotifierWatcher not found on D-Bus");
                        warn!("App indicators won't appear in GNOME without the AppIndicator extension");
                        warn!("Install it from: https://extensions.gnome.org/extension/615/appindicator-support/");
                    }

                    has_watcher
                }
                Err(e) => {
                    error!("Failed to list D-Bus names: {}", e);
                    false
                }
            }
        }
        Err(e) => {
            error!("Failed to connect to D-Bus session bus: {}", e);
            false
        }
    }
}

/// Get the current desktop environment
fn detect_desktop_environment() -> &'static str {
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        if desktop.to_lowercase().contains("gnome") {
            return "GNOME";
        } else if desktop.to_lowercase().contains("kde")
            || desktop.to_lowercase().contains("plasma")
        {
            return "KDE/Plasma";
        } else if desktop.to_lowercase().contains("xfce") {
            return "XFCE";
        }
    }

    if let Ok(session) = std::env::var("DESKTOP_SESSION") {
        if session.to_lowercase().contains("gnome") {
            return "GNOME";
        } else if session.to_lowercase().contains("plasma") {
            return "KDE/Plasma";
        }
    }

    "Unknown"
}

pub fn setup_tray(
    config: &Config,
    app_state: AppState,
    shutdown_token: &CancellationToken,
) -> Option<std::thread::JoinHandle<()>> {
    if !config.ui.show_tray_icon {
        info!("System tray icon disabled in configuration");
        return None;
    }

    match create_tray(app_state, config.clone()) {
        Ok(Some(tray)) => {
            info!("System tray service started successfully");

            // Create a channel for shutdown signaling
            let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();
            let tray_shutdown_token = shutdown_token.child_token();

            // Spawn a task to monitor the cancellation token and send shutdown signal
            tokio::spawn(async move {
                tray_shutdown_token.cancelled().await;
                info!("Tray shutdown requested, sending signal");
                let _ = shutdown_tx.send(());
            });

            Some(std::thread::spawn(move || {
                info!("Starting tray service thread");

                let handle = tray.handle();
                let shutdown_handle = handle.clone();

                // Spawn the shutdown monitor thread and keep its handle
                let monitor_thread = std::thread::spawn(move || {
                    // Block on receiving shutdown signal instead of polling
                    match shutdown_rx.recv() {
                        Ok(()) => {
                            info!("Received shutdown signal, stopping tray service");
                            shutdown_handle.shutdown();
                        }
                        Err(_) => {
                            // Channel disconnected, shutdown anyway
                            warn!("Shutdown channel disconnected, stopping tray service");
                            shutdown_handle.shutdown();
                        }
                    }
                    info!("Shutdown monitor thread exiting");
                });

                // Run the tray service - this blocks until shutdown() is called
                match tray.run() {
                    Ok(()) => {
                        info!("Tray service completed gracefully");
                        // Ensure the handle is dropped to allow shutdown
                        drop(handle);
                    }
                    Err(e) => {
                        error!("Tray service error: {}", e);
                        drop(handle);
                    }
                }

                // Wait for the monitor thread to finish
                if let Err(e) = monitor_thread.join() {
                    warn!("Monitor thread panicked: {:?}", e);
                }

                info!("Tray service thread exiting");
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
}

fn create_tray(
    app_state: AppState,
    config: Config,
) -> eyre::Result<Option<TrayService<VoiceInputTray>>> {
    let desktop = detect_desktop_environment();
    info!("Detected desktop environment: {}", desktop);

    // Check for StatusNotifierWatcher support
    let has_support = check_status_notifier_support();

    if desktop == "GNOME" && !has_support {
        let separator = "=".repeat(70);
        warn!("{}", separator);
        warn!("GNOME SYSTEM TRAY SETUP REQUIRED");
        warn!("{}", separator);
        warn!("GNOME doesn't support app indicators natively.");
        warn!("To see the system tray icon, you need to:");
        warn!("");
        warn!("1. Install the AppIndicator extension:");
        warn!("   https://extensions.gnome.org/extension/615/appindicator-support/");
        warn!("");
        warn!("2. Or install via package manager:");
        warn!("   Ubuntu/Debian: sudo apt install gnome-shell-extension-appindicator");
        warn!("   Fedora: sudo dnf install gnome-shell-extension-appindicator");
        warn!("   Arch: sudo pacman -S gnome-shell-extension-appindicator");
        warn!("");
        warn!("3. Log out and log back in after installation");
        warn!("");
        warn!("The app will continue to work via hotkey (Super+V)");
        warn!("{}", separator);

        // Still try to create the tray - it will be ready if user installs extension
    }

    let handle = Handle::current();
    let tray = VoiceInputTray {
        app_state: app_state.clone(),
        handle,
        config,
    };

    let service = TrayService::new(tray);
    info!("System tray service created successfully");
    if desktop == "GNOME" && !has_support {
        info!(
            "Tray icon registered but won't be visible until AppIndicator extension is installed"
        );
    }
    Ok(Some(service))
}
