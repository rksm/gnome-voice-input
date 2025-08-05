use crate::AppState;
use dbus::blocking::Connection;
use ksni::{self, menu::StandardItem, MenuItem, Tray, TrayService};
use std::time::Duration;
use tokio::runtime::Handle;
use tracing::{error, info, warn};

pub struct VoiceInputTray {
    app_state: AppState,
    handle: Handle,
}

impl Tray for VoiceInputTray {
    fn title(&self) -> String {
        "Voice Input".to_string()
    }

    fn icon_name(&self) -> String {
        // Use system theme icon for microphone
        "audio-input-microphone".to_string()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Toggle Recording".to_string(),
                icon_name: "media-record".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let app_state = tray.app_state.clone();
                    tray.handle.spawn(async move {
                        crate::toggle_recording(app_state).await;
                    });
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "About".to_string(),
                icon_name: "help-about".to_string(),
                activate: Box::new(|_| {
                    info!("GNOME Voice Input v{}", env!("CARGO_PKG_VERSION"));
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    info!("Quit requested from tray");
                    tray.app_state
                        .shutdown
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }),
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

pub fn create_tray(app_state: AppState) -> eyre::Result<Option<TrayService<VoiceInputTray>>> {
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
