use anyhow::{Context, Result};
use deepgram::{
    common::{
        audio_source::AudioSource,
        options::{Language, Model, Options},
    },
    Deepgram,
};
use tokio::sync::mpsc;
use tracing::error;

pub struct Transcriber {
    client: Deepgram,
}

impl Transcriber {
    pub fn new(api_key: String) -> Self {
        let client = Deepgram::new(&api_key).expect("Failed to create Deepgram client");
        Self { client }
    }

    pub async fn transcribe_stream(
        self: std::sync::Arc<Self>,
        mut audio_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<mpsc::Receiver<String>> {
        let (text_tx, text_rx) = mpsc::channel(10);

        let mut audio_buffer = Vec::new();
        let buffer_duration_ms = 500;
        let bytes_per_ms = 16 * 2 / 1000;
        let buffer_size = buffer_duration_ms * bytes_per_ms;

        tokio::spawn(async move {
            while let Some(audio_data) = audio_rx.recv().await {
                audio_buffer.extend_from_slice(&audio_data);

                if audio_buffer.len() >= buffer_size as usize {
                    let chunk = audio_buffer.clone();
                    audio_buffer.clear();

                    match self.transcribe_chunk(chunk).await {
                        Ok(text) => {
                            debug!(?text);
                            if !text.trim().is_empty() && text_tx.send(text).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Transcription error: {}", e);
                        }
                    }
                }
            }

            if !audio_buffer.is_empty() {
                match self.transcribe_chunk(audio_buffer).await {
                    Ok(text) => {
                        if !text.trim().is_empty() {
                            let _ = text_tx.send(text).await;
                        }
                    }
                    Err(e) => {
                        error!("Final transcription error: {}", e);
                    }
                }
            }
        });

        Ok(text_rx)
    }

    async fn transcribe_chunk(&self, audio_data: Vec<u8>) -> Result<String> {
        let source = AudioSource::from_buffer_with_mime_type(audio_data, "audio/wav");

        let options = Options::builder()
            .punctuate(true)
            .language(Language::en)
            .model(Model::Nova3)
            .build();

        let response = self
            .client
            .transcription()
            .prerecorded(source, &options)
            .await
            .context("Failed to transcribe audio")?;

        let transcript = response
            .results
            .channels
            .first()
            .and_then(|channel| channel.alternatives.first())
            .map(|alt| alt.transcript.clone())
            .unwrap_or_default();

        Ok(transcript)
    }
}
