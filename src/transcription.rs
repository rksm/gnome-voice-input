use deepgram::{
    common::{
        audio_source::AudioSource,
        options::{Language, Model, Options},
    },
    Deepgram,
};
use eyre::Result;
use std::io::{Cursor, Write};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

pub struct Transcriber {
    client: Deepgram,
    debug: bool,
}

impl Transcriber {
    pub fn new(api_key: String, debug: bool) -> Self {
        let client = Deepgram::new(&api_key).expect("Failed to create Deepgram client");
        Self { client, debug }
    }

    pub async fn transcribe_stream(
        self: std::sync::Arc<Self>,
        mut audio_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<mpsc::Receiver<String>> {
        let (text_tx, text_rx) = mpsc::channel(10);

        let mut audio_buffer = Vec::new();
        let buffer_duration_ms = 1000; // Increase to 1 second for better transcription
                                       // For f32 at 16kHz: 16000 samples/sec * 4 bytes/sample = 64000 bytes/sec = 64 bytes/ms
        let bytes_per_ms = 64;
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
        debug!("Transcribing chunk of {} bytes", audio_data.len());

        // Convert raw PCM f32 to WAV format
        let wav_data = self.pcm_to_wav(&audio_data)?;
        debug!("WAV data size: {} bytes", wav_data.len());

        // Save WAV file if debug mode is enabled
        if self.debug {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S_%3f");
            let filename = format!("deepgram_audio_{timestamp}.wav");
            match std::fs::File::create(&filename) {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(&wav_data) {
                        error!("Failed to write debug WAV file: {}", e);
                    } else {
                        info!("Saved debug WAV file: {}", filename);
                    }
                }
                Err(e) => {
                    error!("Failed to create debug WAV file: {}", e);
                }
            }
        }

        let source = AudioSource::from_buffer_with_mime_type(wav_data, "audio/wav");

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
            .map_err(|e| {
                error!("Deepgram API error: {:?}", e);
                eyre!("Failed to transcribe audio: {}", e)
            })?;

        let transcript = response
            .results
            .channels
            .first()
            .and_then(|channel| channel.alternatives.first())
            .map(|alt| alt.transcript.clone())
            .unwrap_or_default();

        Ok(transcript)
    }

    fn pcm_to_wav(&self, pcm_data: &[u8]) -> Result<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;

        // Convert f32 bytes to i16 samples and write
        for chunk in pcm_data.chunks_exact(4) {
            if chunk.len() == 4 {
                let f32_bytes: [u8; 4] = chunk.try_into().unwrap();
                let f32_sample = f32::from_le_bytes(f32_bytes);
                // Convert f32 (-1.0 to 1.0) to i16 (-32768 to 32767)
                let i16_sample = (f32_sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                writer.write_sample(i16_sample)?;
            }
        }

        writer.finalize()?;
        Ok(cursor.into_inner())
    }
}
