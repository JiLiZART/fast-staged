use crate::config::load_config;
use crate::event::{AppEvent, Event, EventHandler};
use crate::file::{get_changed_files, match_files_to_commands};
use crate::model::StateModel;
use crate::render::render_frame;
use crate::task::TaskPool;
use crossterm::event::Event::Key;
use crossterm::event::KeyEventKind;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

#[derive(Debug)]
pub struct App {
  /// Is the application running?
  running: bool,
  // Event stream.
  pub events: EventHandler,
  pub model: StateModel,
  pub task_pool: TaskPool,
  pub start_time: Option<Instant>,
  pub changed_files: Vec<String>,
}

impl Default for App {
  fn default() -> Self {
    Self {
      running: false,
      start_time: None,
      changed_files: Vec::new(),
      events: EventHandler::new(),
      model: StateModel::default(),
      task_pool: TaskPool::new(),
    }
  }
}

impl App {
  /// Construct a new instance of [`App`].
  pub fn new() -> Self {
    Self::default()
  }

  /// Run the application's main loop.
  pub async fn run(mut self) -> color_eyre::Result<()> {
    self.running = true;
    self.model.running = true;
    self.start_time = Some(Instant::now());
    self.changed_files = get_changed_files().await?;

    let mut terminal = ratatui::init();

    let config = load_config()?;

    let file_commands = match_files_to_commands(&config, &self.changed_files)?;

    // Запуск команд
    self.task_pool.execute_commands(file_commands).await?;

    while self.model.running {
      terminal.draw(|f| render_frame(f, &self.model))?;

      match self.events.next().await? {
        Event::Tick => {
          self.task_pool.pull_task().await?;

          let done = self.task_pool.all_done().await?;
          let start_time = self.start_time.unwrap_or_else(Instant::now);

          self.model.command_stats = self.task_pool.get_command_stats().await;
          self.model.command_lines = self.task_pool.get_command_list().await;
          self.model.is_empty = self.task_pool.is_empty();
          self.model.total_files = self.changed_files.len();
          self.model.total_execution_time = self.task_pool.get_total_execution_time().await;
          self.model.statuses_count = self.task_pool.statuses().await.len();
          self.model.elapsed_time = start_time.elapsed().as_millis();

          if done {
            println!("All done");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            // self.quit();
          }
        }
        Event::Crossterm(event) => match event {
          Key(key_event) if key_event.kind == KeyEventKind::Press => {
            self.handle_key_events(key_event).await?
          }
          _ => {}
        },
        Event::App(app_event) => match app_event {
          AppEvent::Quit => self.quit(),
        },
      }
    }

    ratatui::restore();

    Ok(())
  }

  /// Handles the key events and updates the state of [`App`].
  pub async fn handle_key_events(&mut self, key_event: KeyEvent) -> color_eyre::Result<()> {
    match key_event.code {
      KeyCode::Esc | KeyCode::Char('q') => self.events.send(AppEvent::Quit),
      KeyCode::Char('c' | 'C') if key_event.modifiers == KeyModifiers::CONTROL => {
        self.events.send(AppEvent::Quit)
      }
      // Other handlers you could add here.
      _ => {}
    }
    Ok(())
  }

  /// Set running to false to quit the application.
  pub fn quit(&mut self) {
    self.running = false;
    self.model.running = false;
  }
}
