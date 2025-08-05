use deepgram::{
    common::options::{Encoding, Language, Model, Options},
    Deepgram,
};
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_env_filter("debug").init();

    info!("Starting Deepgram WebSocket test");

    // Load API key
    let api_key = std::env::var("DEEPGRAM_API_KEY")?;
    let client = Deepgram::new(&api_key)?;

    // Create options
    let options = Options::builder()
        .punctuate(true)
        .language(Language::en)
        .model(Model::Nova3)
        .build();

    info!("Creating WebSocket connection...");

    // Create WebSocket handle
    let mut websocket = client
        .transcription()
        .stream_request_with_options(options)
        .encoding(Encoding::Linear16)
        .sample_rate(16000)
        .channels(1)
        .interim_results(true)
        .handle()
        .await?;

    info!("WebSocket connected!");

    // Send a test audio chunk (silence)
    let test_chunk = vec![0u8; 3200]; // 100ms of silence at 16kHz
    info!("Sending test audio chunk...");
    websocket.send_data(test_chunk).await?;

    // Try to receive a response
    info!("Waiting for response...");
    match websocket.receive().await {
        Some(Ok(response)) => {
            info!("Received response: {:?}", response);
        }
        Some(Err(e)) => {
            error!("Error receiving response: {:?}", e);
        }
        None => {
            info!("No response received");
        }
    }

    // Close the connection
    websocket.close_stream().await?;
    info!("Test completed");

    Ok(())
}
