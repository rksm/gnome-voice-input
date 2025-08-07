use deepgram::{
    common::options::{Encoding, Language, Model, Options},
    Deepgram,
};
use eyre::Result;
use futures::stream::StreamExt;
use tokio::sync::mpsc;

use crate::transcription_utils::{handle_full_response, TranscriptionResult};
use crate::{audio_utils::create_audio_stream, config::TranscriptionConfig};

pub struct Transcriber {
    client: Deepgram,
    config: TranscriptionConfig,
    _debug: bool,
}

impl Transcriber {
    pub fn new(api_key: String, config: TranscriptionConfig, debug: bool) -> Self {
        let client = Deepgram::new(&api_key).expect("Failed to create Deepgram client");
        Self {
            client,
            config,
            _debug: debug,
        }
    }

    pub async fn transcribe_stream(
        self: std::sync::Arc<Self>,
        audio_rx: mpsc::Receiver<Vec<u8>>,
        sample_rate: u32,
    ) -> Result<mpsc::Receiver<TranscriptionResult>> {
        debug!("Creating transcription stream");
        let (text_tx, text_rx) = mpsc::channel(10);

        // Configure options for the base request
        let mut options_builder = Options::builder()
            .punctuate(self.config.punctuate)
            .smart_format(self.config.smart_format);

        // Set language based on config
        options_builder = match self.config.language.as_str() {
            "multi" => options_builder.language(Language::multi),
            "en" => options_builder.language(Language::en),
            "es" => options_builder.language(Language::es),
            "fr" => options_builder.language(Language::fr),
            "de" => options_builder.language(Language::de),
            "it" => options_builder.language(Language::it),
            "pt" => options_builder.language(Language::pt),
            "nl" => options_builder.language(Language::nl),
            "ja" => options_builder.language(Language::ja),
            "ko" => options_builder.language(Language::ko),
            "zh" => options_builder.language(Language::zh),
            "ru" => options_builder.language(Language::ru),
            "uk" => options_builder.language(Language::uk),
            "sv" => options_builder.language(Language::sv),
            other => {
                warn!("Unknown language '{other}', trying it anyway",);
                options_builder.language(Language::Other(other.to_string()))
            }
        };

        // Set model based on config
        options_builder = match self.config.model.as_str() {
            "nova-3" => options_builder.model(Model::Nova3),
            "nova-2" => options_builder.model(Model::Nova2),
            "nova" => options_builder.model(Model::Nova2),
            "enhanced" => options_builder.model(Model::Nova2),
            "base" => options_builder.model(Model::Nova2),
            _ => {
                warn!("Unknown model '{}', defaulting to Nova3", self.config.model);
                options_builder.model(Model::Nova3)
            }
        };

        let options = options_builder.build();

        debug!("Starting WebSocket task with options: {:?}", options);
        tokio::spawn(async move {
            match self
                .start_websocket_stream(options, audio_rx, text_tx, sample_rate)
                .await
            {
                Ok(_) => info!("WebSocket stream completed"),
                Err(e) => error!("WebSocket stream error: {}", e),
            }
        });

        Ok(text_rx)
    }

    async fn start_websocket_stream(
        &self,
        options: Options,
        audio_rx: mpsc::Receiver<Vec<u8>>,
        text_tx: mpsc::Sender<TranscriptionResult>,
        sample_rate: u32,
    ) -> Result<()> {
        info!("Starting WebSocket connection to Deepgram");

        // Convert the audio receiver into a stream that produces Result<Bytes, _>
        let audio_stream = create_audio_stream(audio_rx);

        // Create WebSocket stream with specific audio settings
        let mut stream = self
            .client
            .transcription()
            .stream_request_with_options(options)
            .encoding(Encoding::Linear16)
            .sample_rate(sample_rate)
            .channels(1)
            .keep_alive() // Enable keep-alive
            .stream(audio_stream)
            .await?;

        info!(
            "WebSocket stream created, request_id: {}",
            stream.request_id()
        );

        // Process transcription results
        let mut result_count = 0;
        while let Some(result) = stream.next().await {
            result_count += 1;
            debug!("Received result #{}: {:?}", result_count, result);

            match result {
                Ok(response) => {
                    if let Err(e) = self.handle_stream_response(response, &text_tx).await {
                        error!("Error handling response: {}", e);
                    }
                }
                Err(e) => {
                    error!("Stream error: {:?}", e);
                }
            }
        }

        info!("Transcription stream ended after {} results", result_count);
        Ok(())
    }

    async fn handle_stream_response(
        &self,
        response: deepgram::common::stream_response::StreamResponse,
        text_tx: &mpsc::Sender<TranscriptionResult>,
    ) -> Result<()> {
        if let Some(result) = handle_full_response(response, self.config.use_interim_results) {
            if text_tx.send(result).await.is_err() {
                error!("Failed to send transcript - receiver dropped");
                return Err(eyre!("Text receiver dropped"));
            }
        }

        Ok(())
    }
}
