use deepgram::common::stream_response::StreamResponse;

use tracing::{debug, info};

#[derive(Debug, Clone)]
pub enum TranscriptionResult {
    Interim(String),
    Final(String),
}

/// Handle a simple transcription response (for examples)
pub fn handle_simple_response(response: StreamResponse) -> Option<TranscriptionResult> {
    if let StreamResponse::TranscriptResponse {
        is_final, channel, ..
    } = response
    {
        if let Some(alternative) = channel.alternatives.into_iter().next() {
            let transcript = alternative.transcript.trim();
            if !transcript.is_empty() {
                return Some(if is_final {
                    TranscriptionResult::Final(transcript.to_string())
                } else {
                    TranscriptionResult::Interim(transcript.to_string())
                });
            }
        }
    }

    None
}

/// Handle a full transcription response (for main application)
pub fn handle_full_response(
    response: StreamResponse,
    use_interim_results: bool,
) -> Option<TranscriptionResult> {
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
                    return Some(if is_final {
                        info!(
                            "Final transcript: {} (confidence: {:.2})",
                            transcript, alternative.confidence
                        );
                        TranscriptionResult::Final(transcript.to_string())
                    } else if use_interim_results {
                        debug!("Interim transcript: {}", transcript);
                        TranscriptionResult::Interim(transcript.to_string())
                    } else {
                        // Skip interim results if disabled
                        return None;
                    });
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

    None
}
