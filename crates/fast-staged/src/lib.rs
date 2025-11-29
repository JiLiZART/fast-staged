use fast_glob::glob_match;
use parse_duration::parse;

use ratatui::{
  prelude::*,
  widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::sync::Arc;
use tokio::sync::Mutex;

use std::fs;
use toml;

use crossterm::{
  event::{DisableMouseCapture, EnableMouseCapture},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::backend::CrosstermBackend;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;
use thiserror::Error;

pub async fn run() -> Result<()> {
  // Загрузка конфигурации
  let config = load_config()?;

  // Получение измененных файлов
  let changed_files = get_changed_files().await?;
  let total_files = changed_files.len();

  // Сопоставление файлов с командами
  let file_commands = match_files_to_commands(&config, &changed_files)?;

  // Запуск команд и UI параллельно
  let states = execute_commands(file_commands).await?;

  run_ui(states, total_files).await?;

  Ok(())
}
