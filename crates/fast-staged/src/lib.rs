use fast_glob::glob_match;

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

#[derive(Clone)]
struct TaskState {
  filename: String,
  command: String,
  group_name: Option<String>,
  status: Arc<Mutex<CommandStatus>>,
  started_at: Arc<Mutex<Option<Instant>>>,
  duration_ms: Arc<Mutex<Option<u128>>>,
}

type FilePattern = String;
type CommandList = Vec<String>;

#[derive(Debug, Deserialize)]
struct Config {
  // Глобальный timeout (опционально)
  #[serde(default)]
  timeout: Option<String>,

  // Группы с паттернами и командами
  // Используем HashMap для динамических ключей групп
  #[serde(flatten)]
  groups: HashMap<String, GroupConfig>,
}

#[derive(Debug, Deserialize)]
struct GroupConfig {
  // Timeout для группы (опционально)
  #[serde(default)]
  timeout: Option<String>,

  // Паттерны и команды для группы
  patterns: HashMap<FilePattern, CommandList>,
}

#[derive(Debug, Clone)]
struct Group {
  name: String,
  patterns: HashMap<FilePattern, CommandList>,
  timeout: Option<String>,
}

#[derive(Debug, PartialEq, Clone)]
enum CommandStatus {
  Waiting,
  Running,
  Done,
  Failed,
  // Cancelled,
}

impl std::fmt::Display for CommandStatus {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CommandStatus::Waiting => write!(f, "Waiting"),
      CommandStatus::Running => write!(f, "Running"),
      CommandStatus::Done => write!(f, "Done"),
      CommandStatus::Failed => write!(f, "Failed"),
      // CommandStatus::Cancelled => write!(f, "Cancelled"),
    }
  }
}

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

type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone)]
enum ConfigSource {
  TomlFile(PathBuf),
  JsonFile(PathBuf),
  PackageJson(PathBuf),
}

trait StatusDisplay {
  fn colored(&self) -> (&str, Color);
}

impl StatusDisplay for CommandStatus {
  fn colored(&self) -> (&str, Color) {
    match self {
      CommandStatus::Done => ("✓", Color::Green),
      CommandStatus::Failed => ("✗", Color::Red),
      CommandStatus::Running => ("⟳", Color::Yellow),
      CommandStatus::Waiting => ("⏳", Color::Gray),
    }
  }
}

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

fn find_config_file() -> Result<ConfigSource> {
  let current_dir = std::env::current_dir()?;
  let mut checked_paths = Vec::new();

  // Порядок проверки файлов
  let candidates: Vec<(&str, fn(PathBuf) -> ConfigSource)> = vec![
    (".fast-staged.toml", ConfigSource::TomlFile),
    ("fast-staged.toml", ConfigSource::TomlFile),
    (".fast-staged.json", ConfigSource::JsonFile),
    ("fast-staged.json", ConfigSource::JsonFile),
    ("package.json", ConfigSource::PackageJson),
  ];

  for (filename, source_fn) in candidates {
    let path = current_dir.join(filename);
    checked_paths.push(path.clone());

    if path.exists() {
      return Ok(source_fn(path));
    }
  }

  Err(AppError::ConfigNotFound { checked_paths })
}

fn load_config_from_package_json(path: &Path) -> Result<Config> {
  let content = fs::read_to_string(path).map_err(|e| AppError::ConfigInvalid {
    path: path.to_path_buf(),
    details: format!("Failed to read package.json: {}", e),
  })?;

  let json: Value = serde_json::from_str(&content).map_err(|e| AppError::ConfigInvalid {
    path: path.to_path_buf(),
    details: format!("Invalid JSON in package.json: {}", e),
  })?;

  let fast_staged = json
    .get("fast-staged")
    .ok_or_else(|| AppError::ConfigInvalid {
      path: path.to_path_buf(),
      details: "No 'fast-staged' section found in package.json".to_string(),
    })?;

  // Конвертируем JSON в Config
  // Для простоты используем serde_json для десериализации
  let config: Config =
    serde_json::from_value(fast_staged.clone()).map_err(|e| AppError::ConfigInvalid {
      path: path.to_path_buf(),
      details: format!("Invalid 'fast-staged' section: {}", e),
    })?;

  Ok(config)
}

fn load_config() -> Result<Config> {
  let source = find_config_file()?;

  match source {
    ConfigSource::TomlFile(path) => {
      let config_content = fs::read_to_string(&path).map_err(|e| AppError::ConfigInvalid {
        path: path.clone(),
        details: format!("Failed to read file: {}", e),
      })?;

      let config: Config =
        toml::from_str(&config_content).map_err(|e| AppError::ConfigInvalid {
          path: path.clone(),
          details: format!("Invalid TOML: {}", e),
        })?;

      Ok(config)
    }
    ConfigSource::JsonFile(path) => {
      let config_content = fs::read_to_string(&path).map_err(|e| AppError::ConfigInvalid {
        path: path.clone(),
        details: format!("Failed to read file: {}", e),
      })?;

      let config: Config =
        serde_json::from_str(&config_content).map_err(|e| AppError::ConfigInvalid {
          path: path.clone(),
          details: format!("Invalid JSON: {}", e),
        })?;

      Ok(config)
    }
    ConfigSource::PackageJson(path) => load_config_from_package_json(&path),
  }
}

async fn get_changed_files() -> Result<Vec<String>> {
  // Используем gix для получения списка измененных файлов
  let changed_files = tokio::task::spawn_blocking(|| -> Result<Vec<String>> {
    let current_dir = std::env::current_dir().map_err(|e| AppError::IoError(e))?;

    let repo = gix::open(".").map_err(|_| AppError::NotGitRepository {
      dir: current_dir.clone(),
    })?;

    let index = repo
      .index()
      .map_err(|e| AppError::GitError(format!("{}", e)))?;

    let mut changed_files = Vec::new();

    // Получаем файлы из индекса (staged files)
    for entry in index.entries() {
      changed_files.push(entry.path(&index).to_string());
    }

    if changed_files.is_empty() {
      return Err(AppError::NoStagedFiles);
    }

    Ok(changed_files)
  })
  .await??;

  Ok(changed_files)
}

fn parse_groups_from_config(config: &Config) -> Vec<Group> {
  let mut groups = Vec::new();

  for (group_name, group_config) in &config.groups {
    groups.push(Group {
      name: group_name.clone(),
      patterns: group_config.patterns.clone(),
      timeout: group_config.timeout.clone().or(config.timeout.clone()),
    });
  }

  groups
}

#[derive(Debug, Clone)]
struct FileCommand {
  filename: String,
  command: String,
  group_name: String,
}

fn match_files_to_commands(config: &Config, changed_files: &[String]) -> Result<Vec<FileCommand>> {
  let groups = parse_groups_from_config(config);
  let mut file_commands = Vec::new();
  let mut all_patterns: Vec<String> = Vec::new();

  // Собираем все паттерны для сообщения об ошибке
  for group in &groups {
    all_patterns.extend(group.patterns.keys().cloned());
  }

  for file in changed_files {
    let mut matched = false;
    for group in &groups {
      for (pattern, commands) in &group.patterns {
        if glob_match(pattern, file) {
          println!(
            "Found pattern {} for file {} in group {}",
            pattern, file, group.name
          );
          for command in commands {
            file_commands.push(FileCommand {
              filename: file.clone(),
              command: command.clone(),
              group_name: group.name.clone(),
            });
          }
          matched = true;
          break; // Первое совпадение в группе
        }
      }
      if matched {
        break; // Первое совпадение среди всех групп
      }
    }
  }

  if file_commands.is_empty() && !changed_files.is_empty() {
    return Err(AppError::NoFilesMatched {
      patterns: all_patterns,
    });
  }

  Ok(file_commands)
}

async fn execute_commands(file_commands: Vec<FileCommand>) -> Result<Vec<TaskState>> {
  let mut states = Vec::new();

  // Проверяем наличие всех команд перед запуском
  for file_cmd in &file_commands {
    if !command_exists(&file_cmd.command) {
      return Err(AppError::CommandNotFound {
        command: file_cmd.command.clone(),
        reason: "Command not found in PATH".to_string(),
      });
    }
  }

  for file_cmd in file_commands {
    let state = TaskState {
      filename: file_cmd.filename.clone(),
      command: file_cmd.command.clone(),
      group_name: Some(file_cmd.group_name.clone()),
      status: Arc::new(Mutex::new(CommandStatus::Waiting)),
      started_at: Arc::new(Mutex::new(None)),
      duration_ms: Arc::new(Mutex::new(None)),
    };

    states.push(state.clone());

    // Запускаем каждую команду в отдельной задаче (не ждем завершения)
    let state_clone = state.clone();

    tokio::spawn(async move {
      // Обновляем статус на Running
      *state_clone.status.lock().await = CommandStatus::Running;
      *state_clone.started_at.lock().await = Some(Instant::now());

      // Запускаем команду
      let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&state_clone.command)
        .output()
        .await;

      // Обновляем статус по результату
      let mut status = state_clone.status.lock().await;
      let mut started_at = state_clone.started_at.lock().await;
      let mut duration_ms = state_clone.duration_ms.lock().await;

      match output {
        Ok(output) if output.status.success() => {
          *status = CommandStatus::Done;
        }
        _ => {
          *status = CommandStatus::Failed;
        }
      }
      if let Some(start) = *started_at {
        *duration_ms = Some(start.elapsed().as_millis());
        *started_at = None;
      }
    });
  }

  // Возвращаем состояния сразу, не ждем завершения задач
  Ok(states)
}

fn setup_terminal() -> Result<ratatui::Terminal<CrosstermBackend<io::Stdout>>> {
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
  let backend = CrosstermBackend::new(stdout);
  let terminal = ratatui::Terminal::new(backend)?;
  Ok(terminal)
}

fn restore_terminal(mut terminal: ratatui::Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
  disable_raw_mode()?;
  execute!(
    terminal.backend_mut(),
    LeaveAlternateScreen,
    DisableMouseCapture
  )?;
  terminal.show_cursor()?;
  Ok(())
}

fn command_exists(command: &str) -> bool {
  // Проверяем наличие команды в PATH
  // Для команд вида "sh -c 'command'" проверяем наличие 'sh'
  if command.starts_with("sh -c") {
    return which::which("sh").is_ok();
  }

  // Извлекаем первую часть команды (до пробела)
  let first_part = command.split_whitespace().next().unwrap_or(command);
  which::which(first_part).is_ok()
}

async fn run_ui(states: Vec<TaskState>, total_files: usize) -> Result<()> {
  // Инициализация терминала
  let mut terminal = setup_terminal()?;
  let start_time = Instant::now();

  loop {
    // Собираем данные о статусах задач
    let mut statuses = Vec::new();
    let mut durations = Vec::new();

    for state in &states {
      let status = state.status.lock().await;
      let duration = state.duration_ms.lock().await;
      let duration = duration.unwrap_or(0);

      statuses.push(status.clone());
      durations.push(duration);
    }

    // Общее время выполнения всех команд
    let total_execution_time: u128 = statuses
      .iter()
      .zip(durations.iter())
      .map(|(status, duration)| match status {
        CommandStatus::Done | CommandStatus::Failed => *duration,
        _ => 0,
      })
      .sum();

    // Время с начала запуска
    let elapsed_time = start_time.elapsed().as_millis();

    // Группировка по командам для статистики
    let mut command_stats: HashMap<String, (usize, u128)> = HashMap::new();
    for (state, duration) in states.iter().zip(durations.iter()) {
      let entry = command_stats.entry(state.command.clone()).or_insert((0, 0));
      entry.0 += 1;
      if *duration > 0 {
        entry.1 += *duration;
      }
    }

    terminal.draw(|f| {
      let areas = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
          [
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
          ]
          .as_ref(),
        )
        .split(f.area());

      // Заголовок с информацией о файлах
      let title_text = format!(
        "Running {} tasks for {} file(s)...",
        statuses.len(),
        total_files
      );
      let title = Paragraph::new(title_text)
        .block(Block::default().borders(Borders::empty()).title("Status"));

      f.render_widget(title, areas[0]);

      // Список задач
      if !states.is_empty() {
        let content_areas = Layout::default()
          .direction(Direction::Vertical)
          .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
          .split(areas[1]);

        let items: Vec<ListItem> = states
          .iter()
          .enumerate()
          .map(|(idx, state)| {
            let status = &statuses[idx];
            let duration = durations[idx];
            let (symbol, color) = status.colored();
            let text = match status {
              CommandStatus::Done | CommandStatus::Failed => {
                format!(
                  "{} {}: {} - {}ms",
                  symbol, state.filename, state.command, duration
                )
              }
              _ => format!("{} {}: {}", symbol, state.filename, state.command),
            };
            ListItem::new(text).style(Style::default().fg(color))
          })
          .collect();

        let list =
          List::new(items).block(Block::default().borders(Borders::empty()).title("Tasks"));

        f.render_widget(list, content_areas[0]);

        // Общее время выполнения команд
        let total_line = Paragraph::new(format!(
          "Total execution time: {}ms | Elapsed: {}ms",
          total_execution_time, elapsed_time
        ))
        .style(Style::default().fg(Color::White));
        f.render_widget(total_line, content_areas[1]);
      }

      // Статистика по командам
      if !command_stats.is_empty() {
        let mut stats_lines = Vec::new();
        for (command, (count, total)) in &command_stats {
          let avg = if *count > 0 {
            *total / (*count as u128)
          } else {
            0
          };
          stats_lines.push(format!(
            "{}: {} execution(s), total {}ms, avg {}ms",
            command, count, total, avg
          ));
        }
        let stats_text = stats_lines.join("\n");
        let stats_block = Paragraph::new(stats_text)
          .block(
            Block::default()
              .borders(Borders::empty())
              .title("Command Statistics"),
          )
          .style(Style::default().fg(Color::Cyan));
        f.render_widget(stats_block, areas[2]);
      }
    })?;

    // Проверка завершения всех задач
    let all_done = statuses
      .iter()
      .all(|status| *status == CommandStatus::Done || *status == CommandStatus::Failed);

    if all_done {
      // Ждем немного перед закрытием, чтобы пользователь увидел финальный статус
      tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
      break;
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
  }

  restore_terminal(terminal)?;
  Ok(())
}
