use crate::AppState;
use ksni::{self, menu::StandardItem, MenuItem, Tray, TrayService};
use tokio::runtime::Handle;

pub struct VoiceInputTray {
    app_state: AppState,
    handle: Handle,
}

impl Tray for VoiceInputTray {
    fn title(&self) -> String {
        "Voice Input".to_string()
    }

    fn icon_name(&self) -> String {
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

    fn id(&self) -> String {
        "gnome-voice-input".to_string()
    }
}

pub fn create_tray(app_state: AppState) {
    let handle = Handle::current();

    let tray = VoiceInputTray {
        app_state: app_state.clone(),
        handle,
    };

    let service = TrayService::new(tray);
    let shutdown = app_state.shutdown.clone();

    std::thread::spawn(move || {
        let handle = service.handle();

        // Check for shutdown in a separate thread
        let shutdown_clone = shutdown.clone();
        let handle_clone = handle.clone();
        std::thread::spawn(move || {
            while !shutdown_clone.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            info!("Shutting down tray service");
            handle_clone.shutdown();
        });

        let _ = service.run();
    });
}
