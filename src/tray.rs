use crate::{config::Config, AppState};
use dbus::blocking::Connection;
use gtk4::{glib, prelude::*, AboutDialog, Application, License};
use ksni::{self, menu::StandardItem, MenuItem, Tray, TrayService};
use std::path::Path;
use std::time::Duration;
use tokio::runtime::Handle;
use tracing::{error, info, warn};

pub struct VoiceInputTray {
    app_state: AppState,
    handle: Handle,
    config: Config,
}

/// Show the native GTK about dialog
fn show_about_dialog() {
    std::thread::spawn(move || {
        // Initialize GTK if needed
        let app = Application::builder()
            .application_id("org.gnome.VoiceInput.About")
            .build();

        app.connect_activate(move |app| {
            let about = AboutDialog::builder()
                .modal(true)
                .program_name("GNOME Voice Input")
                .version(env!("CARGO_PKG_VERSION"))
                .comments(env!("CARGO_PKG_DESCRIPTION"))
                .authors(vec![env!("CARGO_PKG_AUTHORS")])
                .license_type(License::MitX11)
                .website("https://github.com/gnome-voice-input")
                .website_label("GitHub Repository")
                .logo_icon_name("audio-input-microphone")
                .build();

            // Add additional credits
            about.add_credit_section("Powered by", &["Deepgram - Advanced Speech Recognition"]);

            about.present();

            // Keep the dialog open and quit app when closed
            let app_clone = app.clone();
            about.connect_close_request(move |_| {
                app_clone.quit();
                glib::Propagation::Proceed
            });
        });

        // Run the GTK application
        let empty: Vec<String> = vec![];
        let exit_code = app.run_with_args(&empty);

        if exit_code != glib::ExitCode::SUCCESS {
            error!("GTK about dialog exited with code: {:?}", exit_code);

            // Fallback to console output
            eprintln!("\n===== About GNOME Voice Input =====");
            eprintln!("Version: {}", env!("CARGO_PKG_VERSION"));
            eprintln!("Description: {}", env!("CARGO_PKG_DESCRIPTION"));
            eprintln!("Authors: {}", env!("CARGO_PKG_AUTHORS"));
            eprintln!("License: MIT");
            eprintln!("Powered by Deepgram");
            eprintln!("====================================\n");
        }
    });
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
                label: "About".to_string(),
                icon_name: "help-about".to_string(),
                activate: Box::new(|_| {
                    info!("About dialog opened");
                    show_about_dialog();
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
                    tray.app_state
                        .shutdown
                        .store(true, std::sync::atomic::Ordering::Relaxed);
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

pub fn create_tray(
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
