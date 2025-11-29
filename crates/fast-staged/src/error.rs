
#[derive(Debug, Error)]
pub enum AppError {
  #[error("Configuration file not found. Checked paths: {checked_paths:?}")]
  ConfigNotFound { checked_paths: Vec<PathBuf> },

  #[error("Invalid configuration in {path:?}: {details}")]
  ConfigInvalid { path: PathBuf, details: String },

  #[error("Not a git repository. Current directory: {dir:?}")]
  NotGitRepository { dir: PathBuf },

  #[error("No staged files found. Run 'git add' to stage files.")]
  NoStagedFiles,

  #[error("No files matched any patterns. Patterns checked: {patterns:?}")]
  NoFilesMatched { patterns: Vec<String> },

  #[error("Failed to execute command '{command}': {reason}")]
  CommandNotFound { command: String, reason: String },

  #[error("IO error: {0}")]
  IoError(#[from] std::io::Error),

  #[error("Git error: {0}")]
  GitError(String),

  #[error("TOML parse error: {0}")]
  TomlError(#[from] toml::de::Error),

  #[error("Task join error: {0}")]
  TaskJoinError(#[from] tokio::task::JoinError),

  #[error("JSON parse error: {0}")]
  JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;
