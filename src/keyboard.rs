use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use eyre::{Result, WrapErr};
use std::time::Duration;

pub fn type_text(text: &str) -> Result<()> {
    debug!("Typing text: {}", text);

    // Add a small delay before creating Enigo to ensure the system is ready
    std::thread::sleep(Duration::from_millis(20));

    let mut enigo = Enigo::new(&Settings::default()).wrap_err("Failed to initialize Enigo")?;

    // Add another small delay after initialization
    std::thread::sleep(Duration::from_millis(30));

    // Type the text character by character with small delays to prevent loss
    for ch in text.chars() {
        let ch_str = ch.to_string();
        enigo.text(&ch_str).wrap_err("Failed to type character")?;
        // Tiny delay between characters to ensure they're all captured
        std::thread::sleep(Duration::from_millis(2));
    }

    Ok(())
}

pub fn press_key(key: Key) -> Result<()> {
    // Add a small delay before creating Enigo
    std::thread::sleep(Duration::from_millis(10));

    let mut enigo = Enigo::new(&Settings::default()).wrap_err("Failed to initialize Enigo")?;

    // Small delay after initialization
    std::thread::sleep(Duration::from_millis(10));

    enigo
        .key(key, Direction::Click)
        .wrap_err("Failed to press key")?;

    Ok(())
}
