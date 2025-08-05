use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use eyre::{Result, WrapErr};
use tracing::debug;

pub fn type_text(text: &str) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).wrap_err("Failed to initialize Enigo")?;

    debug!("Typing text: {}", text);

    enigo.text(text).wrap_err("Failed to type text")?;

    Ok(())
}

#[allow(dead_code)]
pub fn press_key(key: Key) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).wrap_err("Failed to initialize Enigo")?;

    enigo
        .key(key, Direction::Click)
        .wrap_err("Failed to press key")?;

    Ok(())
}
