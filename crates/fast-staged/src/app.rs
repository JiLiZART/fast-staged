use crate::config::load_config;
use crate::file::{get_changed_files, match_files_to_commands};
use crate::render::render_frame;
use crate::task::TaskPool;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::{FutureExt, StreamExt};
use ratatui::DefaultTerminal;
use std::path::PathBuf;
use thiserror::Error;
use tokio::time::Instant;

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

  #[error("Timeout: {0}")]
  Timeout(#[from] tokio::time::error::Elapsed),

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

#[derive(Debug, Default)]
pub struct App {
  /// Is the application running?
  running: bool,
  // Event stream.
  event_stream: EventStream,
}

impl App {
  /// Construct a new instance of [`App`].
  pub fn new() -> Self {
    Self::default()
  }

  /// Run the application's main loop.
  pub async fn run(mut self) -> color_eyre::Result<()> {
    self.running = true;

    let mut terminal = ratatui::init();
    let mut task_pool = TaskPool::new();

    let start_time = Instant::now();

    // Загрузка конфигурации
    let config = load_config()?;

    // Получение измененных файлов
    let changed_files = get_changed_files().await?;

    // Сопоставление файлов с командами
    let file_commands = match_files_to_commands(&config, &changed_files)?;

    println!("pool_result start");

    // Запуск команд
    task_pool.execute_commands(file_commands).await?;

    println!("pool_result end");

    while self.running {
      let statuses = task_pool.statuses().await;

      // Общее время выполнения всех команд
      let total_execution_time = task_pool.get_total_execution_time().await;

      // Время с начала запуска
      let elapsed_time = start_time.elapsed().as_millis();

      // Группировка по командам для статистики
      let command_stats = task_pool.get_command_stats().await;
      let command_lines = task_pool.get_command_list().await;

      let is_empty = task_pool.is_empty();
      let total_files = changed_files.len();
      let statuses_count = statuses.len();

      task_pool.pull_task().await?;
      // self.handle_crossterm_events().await?;
      let done = task_pool.all_done().await?;

      terminal.draw(|frame| {
        render_frame(
          frame,
          &command_lines,
          &command_stats,
          statuses_count,
          total_files,
          is_empty,
          total_execution_time,
          elapsed_time,
        )
      })?;

      if done {
        println!("All done");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        self.quit();
      }
    }

    ratatui::restore();

    Ok(())
  }

  /// Reads the crossterm events and updates the state of [`App`].
  async fn handle_crossterm_events(&mut self) -> color_eyre::Result<()> {
    let event = self.event_stream.next().fuse().await;
    match event {
      Some(Ok(evt)) => match evt {
        Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
        Event::Mouse(_) => {}
        Event::Resize(_, _) => {}
        _ => {}
      },
      _ => {}
    }
    Ok(())
  }

  /// Handles the key events and updates the state of [`App`].
  fn on_key_event(&mut self, key: KeyEvent) {
    match (key.modifiers, key.code) {
      (_, KeyCode::Esc | KeyCode::Char('q'))
      | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),
      // Add other key handlers here.
      _ => {}
    }
  }

  /// Set running to false to quit the application.
  fn quit(&mut self) {
    self.running = false;
  }
}
