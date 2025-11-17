use anyhow::Result;
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
use std::collections::HashMap;
use std::io;
use std::time::Instant;

#[derive(Clone)]
struct TaskState {
    filename: String,
    command: String,
    status: Arc<Mutex<CommandStatus>>,
    started_at: Arc<Mutex<Option<Instant>>>,
    duration_ms: Arc<Mutex<Option<u128>>>,
}

type FilePattern = String;
type CommandList = Vec<String>;

#[derive(Debug, Deserialize)]
struct Config {
    // Ключ: шаблон файла
    // Значение: массив команд для выполнения
    // Используем HashMap для динамических ключей
    patterns: HashMap<FilePattern, CommandList>,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Загрузка конфигурации
    let config = load_config(".fast-staged.toml")?;

    // Получение измененных файлов
    let changed_files = get_changed_files().await?;

    // Сопоставление файлов с командами
    let file_commands = match_files_to_commands(&config, &changed_files);

    // Запуск команд и UI параллельно
    let states = execute_commands(file_commands).await;

    run_ui(states).await?;

    Ok(())
}

fn load_config(path: &str) -> Result<Config> {
    let config_content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&config_content)?;

    Ok(config)
}

async fn get_changed_files() -> Result<Vec<String>> {
    // Используем gix для получения списка измененных файлов
    let changed_files = tokio::task::spawn_blocking(|| -> Result<Vec<String>> {
        let repo = gix::open(".")?;
        let index = repo.index()?;

        let mut changed_files = Vec::new();

        // Получаем файлы из индекса (staged files)
        for entry in index.entries() {
            changed_files.push(entry.path(&index).to_string());
        }

        Ok(changed_files)
    })
    .await?;

    Ok(changed_files?)
}

fn match_files_to_commands(
    config: &Config,
    changed_files: &[String],
) -> HashMap<String, Vec<String>> {
    let mut file_commands = HashMap::new();

    for file in changed_files {
        for (pattern, commands) in &config.patterns {
            if glob_match(pattern, file) {
                println!("Found pattern {} for file {}", pattern, file);
                file_commands.insert(file.clone(), commands.clone());
                break; // Первое совпадение
            }
        }
    }

    if file_commands.is_empty() {
        println!("No commands found");
    }

    file_commands
}

async fn execute_commands(file_commands: HashMap<String, Vec<String>>) -> Vec<TaskState> {
    let mut states = Vec::new();

    for (filename, commands) in file_commands {
        for command in commands {
            let state = TaskState {
                filename: filename.clone(),
                command: command.clone(),
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
    }

    // Возвращаем состояния сразу, не ждем завершения задач
    states
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

async fn run_ui(states: Vec<TaskState>) -> Result<(), Box<dyn std::error::Error>> {
    // Инициализация терминала
    let mut terminal = setup_terminal()?;

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

        let total_time: u128 = statuses
            .iter()
            .zip(durations.iter())
            .map(|(status, duration)| match status {
                CommandStatus::Done | CommandStatus::Failed => *duration,
                _ => 0,
            })
            .sum();

        terminal.draw(|f| {
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(f.area());

            // Заголовок
            let title = Paragraph::new(format!("Running {} tasks...", statuses.len()))
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

                let list = List::new(items)
                    .block(Block::default().borders(Borders::empty()).title("Tasks"));

                f.render_widget(list, content_areas[0]);

                let total_line = Paragraph::new(format!("Total time: {}ms", total_time))
                    .style(Style::default().fg(Color::White));
                f.render_widget(total_line, content_areas[1]);
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
