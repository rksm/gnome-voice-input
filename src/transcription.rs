use deepgram::{
    common::options::{Encoding, Language, Model, Options},
    Deepgram,
};
use eyre::Result;
use futures::stream::{Stream, StreamExt};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum TranscriptionResult {
    Interim(String),
    Final(String),
}

pub struct Transcriber {
    client: Deepgram,
    _debug: bool,
}

impl Transcriber {
    pub fn new(api_key: String, debug: bool) -> Self {
        let client = Deepgram::new(&api_key).expect("Failed to create Deepgram client");
        Self {
            client,
            _debug: debug,
        }
    }

    pub async fn transcribe_stream(
        self: std::sync::Arc<Self>,
        audio_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<mpsc::Receiver<TranscriptionResult>> {
        debug!("Creating transcription stream");
        let (text_tx, text_rx) = mpsc::channel(10);

        // Configure options for the base request
        let options = Options::builder()
            .punctuate(true)
            .language(Language::en)
            .model(Model::Nova3)
            .build();

        debug!("Starting WebSocket task with options: {:?}", options);
        tokio::spawn(async move {
            match self
                .start_websocket_stream(options, audio_rx, text_tx)
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
            .sample_rate(16000)
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
                    if let Err(e) = Self::handle_stream_response(response, &text_tx).await {
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
        response: deepgram::common::stream_response::StreamResponse,
        text_tx: &mpsc::Sender<TranscriptionResult>,
    ) -> Result<()> {
        use deepgram::common::stream_response::StreamResponse;

        match response {
            StreamResponse::TranscriptResponse {
                is_final, channel, ..
            } => {
                debug!("TranscriptResponse - is_final: {}", is_final);
                debug!(
                    "Processing transcript, alternatives count: {}",
                    channel.alternatives.len()
                );

                // Extract transcript text from the channel
                if let Some(alternative) = channel.alternatives.into_iter().next() {
                    let transcript = alternative.transcript.trim();
                    debug!(
                        "Transcript text: '{}', confidence: {:.2}, is_final: {}",
                        transcript, alternative.confidence, is_final
                    );

                    if !transcript.is_empty() {
                        let result = if is_final {
                            info!(
                                "Final transcript: {} (confidence: {:.2})",
                                transcript, alternative.confidence
                            );
                            TranscriptionResult::Final(transcript.to_string())
                        } else {
                            debug!("Interim transcript: {}", transcript);
                            TranscriptionResult::Interim(transcript.to_string())
                        };

                        if text_tx.send(result).await.is_err() {
                            error!("Failed to send transcript - receiver dropped");
                            return Err(eyre!("Text receiver dropped"));
                        }
                    } else {
                        debug!("Transcript was empty, ignoring");
                    }
                } else {
                    debug!("No alternatives in transcript response");
                }
            }
            StreamResponse::UtteranceEndResponse { last_word_end, .. } => {
                debug!("Utterance ended: last word end {:?}", last_word_end);
            }
            StreamResponse::SpeechStartedResponse { timestamp, .. } => {
                debug!("Speech started at timestamp: {:?}", timestamp);
            }
            StreamResponse::TerminalResponse {
                request_id,
                created,
                duration,
                ..
            } => {
                debug!(
                    "Terminal response: request_id={}, created={}, duration={:?}",
                    request_id, created, duration
                );
            }
            _ => {
                debug!("Received unknown response type: {:?}", response);
            }
        }

        Ok(())
    }
}

// Convert mpsc::Receiver to a Stream that produces Result<Bytes, Error>
fn create_audio_stream(
    mut audio_rx: mpsc::Receiver<Vec<u8>>,
) -> impl Stream<Item = Result<bytes::Bytes, std::io::Error>> {
    futures::stream::poll_fn(move |cx| match audio_rx.poll_recv(cx) {
        std::task::Poll::Ready(Some(data)) => {
            debug!("Audio stream produced {} bytes", data.len());
            std::task::Poll::Ready(Some(Ok(bytes::Bytes::from(data))))
        }
        std::task::Poll::Ready(None) => {
            debug!("Audio stream ended");
            std::task::Poll::Ready(None)
        }
        std::task::Poll::Pending => std::task::Poll::Pending,
    })
}
