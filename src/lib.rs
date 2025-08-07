#[macro_use]
extern crate tracing;

#[macro_use]
extern crate eyre;

pub mod audio;
pub mod audio_utils;
pub mod config;
pub mod handlers;
pub mod keyboard;
pub mod state;
pub mod transcription;
pub mod transcription_utils;

// Re-export commonly used items
pub use config::Config;
pub use handlers::{
    process_transcription_with_handler, ConsoleTranscriptionHandler, KeyboardTranscriptionHandler,
    TranscriptionHandler,
};
pub use state::AppState;
pub use transcription::Transcriber;
pub use transcription_utils::TranscriptionResult;
