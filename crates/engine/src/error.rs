use std::path::PathBuf;

/// All fallible engine operations return this error.
/// Messages are user-facing (shown in the UI), so keep them short and specific.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot decode {path}: {reason}")]
    Decode { path: PathBuf, reason: String },
    #[error("unsupported audio format: {0}")]
    Unsupported(String),
    #[error("resampling failed: {0}")]
    Resample(String),
    #[error("invalid project file: {0}")]
    ProjectFormat(String),
    #[error("invalid edit: {0}")]
    InvalidEdit(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("audio device error: {0}")]
    Device(String),
    #[error("engine command queue full")]
    QueueFull,
    #[error("WAV write error: {0}")]
    Wav(String),
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, EngineError>;

impl EngineError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
