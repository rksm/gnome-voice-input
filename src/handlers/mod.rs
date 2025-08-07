pub mod console_handler;
pub mod keyboard_handler;
mod transcription_handler;

#[allow(unused_imports)]
pub use console_handler::ConsoleTranscriptionHandler;
pub use keyboard_handler::KeyboardTranscriptionHandler;

pub use transcription_handler::{process_transcription_with_handler, TranscriptionHandler};
