use crate::AppState;
use ksni::{self, menu::StandardItem, MenuItem, Tray, TrayService};
use tokio::runtime::Handle;
use tracing::info;

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
                activate: Box::new(|_| {
                    std::process::exit(0);
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

    let tray = VoiceInputTray { app_state, handle };

    let service = TrayService::new(tray);

    std::thread::spawn(move || {
        let _ = service.run();
    });
}
