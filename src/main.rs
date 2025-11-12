use anyhow::Result;
use fast_glob::glob_match;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, Paragraph},
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

#[derive(Clone)]
struct TaskState {
    filename: String,
    command: String,
    status: Arc<Mutex<CommandStatus>>,
    progress: Arc<Mutex<f64>>, // от 0.0 до 1.0
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Загрузка конфигурации
    let config = load_config("config.toml")?;

    print!("Loading config... {:?}", config);

    // Получение измененных файлов
    let changed_files = get_changed_files().await?;

    // Сопоставление файлов с командами
    let file_commands = match_files_to_commands(&config, &changed_files);

    // Запуск команд с UI
    let states = execute_commands(file_commands).await;

    // Отображение прогресса
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
    let mut tasks = Vec::new();
    let mut states = Vec::new();

    for (filename, commands) in file_commands {
        for command in commands {
            let state = TaskState {
                filename: filename.clone(),
                command: command.clone(),
                status: Arc::new(Mutex::new(CommandStatus::Waiting)),
                progress: Arc::new(Mutex::new(0.0)),
            };

            states.push(state.clone());

            // Запускаем каждую команду в отдельной задаче
            let task = tokio::spawn(async move {
                // Обновляем статус
                *state.status.lock().await = CommandStatus::Running;

                // Запускаем команду
                let output = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&state.command)
                    .output()
                    .await;

                // Обновляем статус по результату
                let mut status = state.status.lock().await;
                let mut progress = state.progress.lock().await;

                match output {
                    Ok(output) if output.status.success() => {
                        *status = CommandStatus::Done;
                        *progress = 1.0;
                    }
                    _ => {
                        *status = CommandStatus::Failed;
                        *progress = 1.0;
                    }
                }
            });

            tasks.push(task);
        }
    }

    // Ждем завершения всех задач
    for task in tasks {
        let _ = task.await;
    }

    states
}

fn setup_terminal()
-> Result<ratatui::Terminal<CrosstermBackend<io::Stdout>>, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = ratatui::Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(
    mut terminal: ratatui::Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
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
        terminal.draw(|f| {
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(f.area());

            // Заголовок
            let title = Paragraph::new("Running tasks...")
                .block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(title, areas[0]);

            // Список задач с индикаторами
            let task_areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(3); states.len()])
                .split(areas[1]);

            for (i, state) in states.iter().enumerate() {
                let status = state.status.blocking_lock();
                let progress = state.progress.blocking_lock();

                let title_text = format!("{}: {}", state.filename, state.command);
                let gauge = Gauge::default()
                    .block(Block::default().title(title_text.as_str()))
                    .gauge_style(Style::default().fg(Color::Yellow))
                    .percent((*progress * 100.0) as u16)
                    .label(format!("{} {:.0}%", status, *progress * 100.0));

                f.render_widget(gauge, task_areas[i]);
            }
        })?;

        // Проверка завершения всех задач
        let all_done = states.iter().all(|s| {
            let status = s.status.blocking_lock();
            *status == CommandStatus::Done || *status == CommandStatus::Failed
        });

        if all_done {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    restore_terminal(terminal)?;
    Ok(())
}
