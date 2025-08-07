use crate::keyboard;
use async_trait::async_trait;
use eyre::Result;

use super::transcription_handler::TranscriptionHandler;

/// Handler that types transcription results using keyboard simulation
pub struct KeyboardTranscriptionHandler {
    use_interim_results: bool,
    last_interim_length: usize,
}

impl KeyboardTranscriptionHandler {
    pub fn new(use_interim_results: bool) -> Self {
        Self {
            use_interim_results,
            last_interim_length: 0,
        }
    }
}

#[async_trait]
impl TranscriptionHandler for KeyboardTranscriptionHandler {
    async fn on_interim_result(&mut self, text: String) -> Result<()> {
        debug!("Received interim transcription: '{}'", text);

        if self.use_interim_results && !text.trim().is_empty() {
            // Delete previous interim text by sending backspaces
            if self.last_interim_length > 0 {
                for _ in 0..self.last_interim_length {
                    keyboard::press_key(enigo::Key::Backspace)?;
                }
            }

            // Type new interim text
            keyboard::type_text(&text)?;
            self.last_interim_length = text.chars().count();
        }

        Ok(())
    }

    async fn on_final_result(&mut self, text: String) -> Result<()> {
        debug!("Received final transcription: '{}'", text);

        if !text.trim().is_empty() {
            // Delete previous interim text if any
            if self.use_interim_results && self.last_interim_length > 0 {
                for _ in 0..self.last_interim_length {
                    keyboard::press_key(enigo::Key::Backspace)?;
                }
                self.last_interim_length = 0;
            }

            info!("Final transcribed: {}", text);
            keyboard::type_text(&text)?;

            // Add a space after final transcription for better flow
            keyboard::type_text(" ")?;
        }

        Ok(())
    }
}
