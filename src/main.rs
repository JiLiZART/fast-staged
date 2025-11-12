use asyncgit::Repository;
use glob::Pattern;
use ratatui::{
    prelude::*,
    symbols,
    widgets::{Block, Borders, Gauge, Paragraph},
};
use std::sync::Arc;
use tokio::sync::Mutex;

use std::fs;
use toml;

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone)]
struct TaskState {
    filename: String,
    command: String,
    status: Arc<Mutex<String>>, // "Running", "Done", "Failed"
    progress: Arc<Mutex<f64>>,  // от 0.0 до 1.0
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

#[derive(Debug, PartialEq)]
enum CommandStatus {
    Waiting,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Загрузка конфигурации
    let config = load_config("config.toml")?;

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

fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let config_content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&config_content)?;
    Ok(config)
}

async fn get_changed_files() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // Используйте asyncgit для получения списка файлов
    // через git status --porcelain или git diff --name-only
    let repo = asyncgit::Repository::open(".").await?;
    let statuses = repo.statuses().await?;

    let changed_files: Vec<String> = statuses
        .iter()
        .filter(|s| s.is_modified() || s.is_added() || s.is_renamed())
        .map(|s| s.path().to_string())
        .collect();

    Ok(changed_files)
}

fn match_files_to_commands(
    config: &Config,
    changed_files: &[String],
) -> HashMap<String, Vec<String>> {
    let mut file_commands = HashMap::new();

    for file in changed_files {
        for (pattern, commands) in &config.patterns {
            if Pattern::new(pattern).unwrap().matches(file) {
                file_commands.insert(file.clone(), commands.clone());
                break; // Первое совпадение
            }
        }
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

async fn run_ui(states: Vec<TaskState>) -> Result<(), Box<dyn std::error::Error>> {
    // Инициализация терминала
    let mut terminal = setup_terminal()?;

    loop {
        terminal.draw(|f| {
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
                .split(f.size());

            // Заголовок
            let title = Paragraph::new("Running tasks...")
                .block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(title, areas[0]);

            // Список задач с индикаторами
            let task_areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Length(3); states.len()].as_ref())
                .split(areas[1]);

            for (i, state) in states.iter().enumerate() {
                let status = state.status.block_on_lock();
                let progress = state.progress.block_on_lock();

                let gauge = Gauge::default()
                    .block(
                        Block::default().title(&format!("{}: {}", state.filename, state.command)),
                    )
                    .gauge_style(Style::default().fg(Color::Yellow))
                    .percent((progress * 100.0) as u16)
                    .label(format!("{} {:.0}%", status, progress * 100.0));

                f.render_widget(gauge, task_areas[i]);
            }
        })?;

        // Проверка завершения всех задач
        let all_done = states.iter().all(|s| {
            let status = s.status.block_on_lock();
            status == CommandStatus::Done || status == CommandStatus::Failed
        });

        if all_done {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    restore_terminal()?;
    Ok(())
}
