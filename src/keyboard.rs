use anyhow::{Context, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use tracing::debug;

pub fn type_text(text: &str) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).context("Failed to initialize Enigo")?;

    debug!("Typing text: {}", text);

    enigo.text(text).context("Failed to type text")?;

    Ok(())
}

#[allow(dead_code)]
pub fn press_key(key: Key) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).context("Failed to initialize Enigo")?;

    enigo
        .key(key, Direction::Click)
        .context("Failed to press key")?;

    Ok(())
}
