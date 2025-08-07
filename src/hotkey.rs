use crate::{config::Config, state::AppState};
use eyre::{Result, WrapErr};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use tokio_util::sync::CancellationToken;

/// Parse hotkey configuration into a HotKey without registering it
pub fn parse_hotkey(config: &Config) -> Result<HotKey> {
    let mut modifiers = Modifiers::empty();

    for modifier in &config.hotkey.modifiers {
        match modifier.to_lowercase().as_str() {
            "super" | "meta" | "cmd" => modifiers |= Modifiers::SUPER,
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            _ => bail!("Unknown modifier: {}", modifier),
        }
    }

    let code = match config.hotkey.key.to_lowercase().as_str() {
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "space" => Code::Space,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        _ => bail!("Unknown key: {}", config.hotkey.key),
    };

    let hotkey = HotKey::new(Some(modifiers), code);
    Ok(hotkey)
}

pub fn setup_hotkeys(config: &Config) -> Result<(GlobalHotKeyManager, HotKey)> {
    let manager = GlobalHotKeyManager::new().wrap_err("Failed to create hotkey manager")?;
    let hotkey = parse_hotkey(config)?;

    manager
        .register(hotkey)
        .wrap_err("Failed to register hotkey")?;

    info!(
        "Registered hotkey: {} + {}",
        config.hotkey.modifiers.join("+"),
        config.hotkey.key
    );

    Ok((manager, hotkey))
}

pub fn setup_hotkey_handlers(
    app_state: AppState,
    shutdown_token: &CancellationToken,
) -> (tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>) {
    let (hotkey_tx, mut hotkey_rx) = tokio::sync::mpsc::channel(10);
    let hotkey_shutdown_token = shutdown_token.child_token();

    let hotkey_handle = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Handle::current();

        loop {
            if hotkey_shutdown_token.is_cancelled() {
                info!("Hotkey handler shutting down");
                break;
            }

            match GlobalHotKeyEvent::receiver().recv_timeout(std::time::Duration::from_millis(100))
            {
                Ok(event) => {
                    if event.state == HotKeyState::Pressed {
                        info!("Hotkey pressed");
                        let tx = hotkey_tx.clone();
                        runtime.spawn(async move {
                            let _ = tx.send(()).await;
                        });
                    }
                }
                Err(_) => continue,
            }
        }
    });

    let hotkey_rx_shutdown_token = shutdown_token.child_token();
    let hotkey_rx_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(()) = hotkey_rx.recv() => {
                    crate::toggle_recording(app_state.clone()).await;
                }
                _ = hotkey_rx_shutdown_token.cancelled() => {
                    info!("Hotkey receiver shutting down");
                    break;
                }
            }
        }
    });

    (hotkey_handle, hotkey_rx_handle)
}
