use async_trait::async_trait;
use eyre::Result;
use std::io::Write;

use super::TranscriptionHandler;

/// Handler that prints transcription results to stdout
#[derive(Default)]
pub struct ConsoleTranscriptionHandler;

impl ConsoleTranscriptionHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TranscriptionHandler for ConsoleTranscriptionHandler {
    async fn on_interim_result(&mut self, text: String) -> Result<()> {
        print!("\rInterim: {}", text);
        std::io::stdout().flush()?;
        Ok(())
    }

    async fn on_final_result(&mut self, text: String) -> Result<()> {
        println!("\nFinal: {}", text);
        Ok(())
    }

    async fn on_transcription_start(&mut self) -> Result<()> {
        println!("Transcription started. Speak into your microphone...\n");
        Ok(())
    }

    async fn on_transcription_end(&mut self) -> Result<()> {
        println!("\nTranscription stopped.");
        Ok(())
    }
}
