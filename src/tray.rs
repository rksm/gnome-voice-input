use crate::AppState;
use ksni::{self, menu::StandardItem, MenuItem, Tray, TrayService};
use std::sync::Arc;
use tokio::runtime::Handle;
use tracing::info;

pub struct VoiceInputTray {
    app_state: AppState,
    handle: Handle,
    service_handle: Arc<std::sync::Mutex<Option<ksni::Handle<VoiceInputTray>>>>,
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
                    tray.app_state
                        .shutdown
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    if let Ok(mut handle) = tray.service_handle.lock() {
                        if let Some(h) = handle.take() {
                            h.shutdown();
                        }
                    }
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
    let service_handle = Arc::new(std::sync::Mutex::new(None));

    let tray = VoiceInputTray {
        app_state: app_state.clone(),
        handle,
        service_handle: service_handle.clone(),
    };

    let service = TrayService::new(tray);
    let shutdown = app_state.shutdown.clone();

    std::thread::spawn(move || {
        let handle = service.handle();
        if let Ok(mut h) = service_handle.lock() {
            *h = Some(handle.clone());
        }

        // Check for shutdown in a separate thread
        let shutdown_clone = shutdown.clone();
        let handle_clone = handle.clone();
        std::thread::spawn(move || {
            while !shutdown_clone.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            handle_clone.shutdown();
        });

        let _ = service.run();
    });
}
