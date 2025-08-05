use crate::{config::Config, transcription};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub recording: Arc<AtomicBool>,
    pub transcriber: Arc<RwLock<Arc<transcription::Transcriber>>>,
    pub shutdown_token: CancellationToken,
    pub debug: bool,
    pub custom_config_path: Option<std::path::PathBuf>,
}

impl AppState {
    pub(crate) fn new(
        config: Config,
        debug: bool,
        custom_config_path: Option<std::path::PathBuf>,
        shutdown_token: CancellationToken,
    ) -> Self {
        let transcriber = Arc::new(transcription::Transcriber::new(
            config.deepgram_api_key.clone(),
            config.transcription.clone(),
            debug,
        ));

        Self {
            config: Arc::new(RwLock::new(config)),
            recording: Arc::new(AtomicBool::new(false)),
            transcriber: Arc::new(RwLock::new(transcriber)),
            shutdown_token,
            debug,
            custom_config_path,
        }
    }
}
