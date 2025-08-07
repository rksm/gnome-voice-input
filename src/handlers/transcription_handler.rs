use crate::transcription_utils::TranscriptionResult;
use async_trait::async_trait;
use eyre::Result;

/// Trait for handling transcription results from the speech-to-text system
#[async_trait]
pub trait TranscriptionHandler: Send + Sync {
    /// Called when an interim (temporary) transcription result is received
    /// These results may change as more audio is processed
    async fn on_interim_result(&mut self, text: String) -> Result<()>;

    /// Called when a final transcription result is received
    /// These results are stable and will not change
    async fn on_final_result(&mut self, text: String) -> Result<()>;

    /// Called when transcription starts (optional hook)
    async fn on_transcription_start(&mut self) -> Result<()> {
        Ok(())
    }

    /// Called when transcription ends (optional hook)
    async fn on_transcription_end(&mut self) -> Result<()> {
        Ok(())
    }

    /// Called when transcription encounters an error (optional hook)
    async fn on_transcription_error(&mut self, error: String) -> Result<()> {
        error!("Transcription error: {}", error);
        Ok(())
    }
}

/// Process transcription results using a handler
pub async fn process_transcription_with_handler<H>(
    mut transcription_rx: tokio::sync::mpsc::Receiver<TranscriptionResult>,
    mut handler: H,
) -> Result<()>
where
    H: TranscriptionHandler,
{
    handler.on_transcription_start().await?;

    while let Some(result) = transcription_rx.recv().await {
        match result {
            TranscriptionResult::Interim(text) => {
                if let Err(e) = handler.on_interim_result(text).await {
                    let error_msg = format!("Error handling interim result: {e}");
                    handler.on_transcription_error(error_msg).await?;
                }
            }
            TranscriptionResult::Final(text) => {
                if let Err(e) = handler.on_final_result(text).await {
                    let error_msg = format!("Error handling final result: {e}");
                    handler.on_transcription_error(error_msg).await?;
                }
            }
        }
    }

    handler.on_transcription_end().await?;
    Ok(())
}
