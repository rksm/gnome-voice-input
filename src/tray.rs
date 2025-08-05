use crate::AppState;
use tracing::info;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

pub struct VoiceInputTray {
    _tray_icon: TrayIcon,
}

impl VoiceInputTray {
    pub fn new(app_state: AppState) -> eyre::Result<Self> {
        // Initialize GTK if not already initialized
        if !gtk::is_initialized() {
            gtk::init().map_err(|e| eyre!("Failed to initialize GTK: {}", e))?;
        }

        // Create menu
        let menu = Menu::new();

        let toggle_item = MenuItem::new("Toggle Recording", true, None);
        let about_item = MenuItem::new("About", true, None);
        let quit_item = MenuItem::new("Quit", true, None);

        menu.append(&toggle_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&about_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&quit_item)?;

        // Use a simple icon - we'll use a default microphone icon
        // In a real app, you'd load an icon from resources
        let icon = Icon::from_rgba(vec![255; 32 * 32 * 4], 32, 32)?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("GNOME Voice Input")
            .with_icon(icon)
            .build()?;

        // Handle menu events in a separate thread
        let app_state_clone = app_state.clone();
        let toggle_id = toggle_item.id().clone();
        let about_id = about_item.id().clone();
        let quit_id = quit_item.id().clone();

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();

            loop {
                if app_state_clone
                    .shutdown
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    info!("Tray event handler shutting down");
                    break;
                }

                if let Ok(event) =
                    MenuEvent::receiver().recv_timeout(std::time::Duration::from_millis(100))
                {
                    if event.id == toggle_id {
                        info!("Toggle recording from tray");
                        let app_state = app_state_clone.clone();
                        runtime.spawn(async move {
                            crate::toggle_recording(app_state).await;
                        });
                    } else if event.id == about_id {
                        info!("GNOME Voice Input v{}", env!("CARGO_PKG_VERSION"));
                    } else if event.id == quit_id {
                        info!("Quit requested from tray");
                        app_state_clone
                            .shutdown
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        });

        Ok(Self {
            _tray_icon: tray_icon,
        })
    }
}

pub fn create_tray(app_state: AppState) -> eyre::Result<VoiceInputTray> {
    let tray = VoiceInputTray::new(app_state)?;
    Ok(tray)
}
