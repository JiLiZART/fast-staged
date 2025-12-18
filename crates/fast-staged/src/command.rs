use crate::app::AppError;
use crate::app::Result;
use crate::config::ExecutionOrder;
use crate::file::FileCommand;
// use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

#[derive(Debug, PartialEq, Clone)]
pub enum CommandStatus {
  Waiting,
  Running,
  Done,
  Failed,
  Timeout,
  // Cancelled,
}

impl std::fmt::Display for CommandStatus {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CommandStatus::Waiting => write!(f, "Waiting"),
      CommandStatus::Running => write!(f, "Running"),
      CommandStatus::Done => write!(f, "Done"),
      CommandStatus::Failed => write!(f, "Failed"),
      CommandStatus::Timeout => write!(f, "Timeout"),
      // CommandStatus::Cancelled => write!(f, "Cancelled"),
    }
  }
}

#[derive(Clone)]
pub struct TaskState {
  pub filename: String,
  pub command: String,
  // pub group_name: Option<String>,
  pub status: Arc<Mutex<CommandStatus>>,
  pub started_at: Arc<Mutex<Option<Instant>>>,
  pub duration_ms: Arc<Mutex<Option<u128>>>,
}

impl TaskState {
  pub fn from_file_command(file_cmd: FileCommand) -> Self {
    TaskState {
      filename: file_cmd.filename.clone(),
      command: file_cmd.command.clone(),
      // group_name: Some(file_cmd.group_name.clone()),
      status: Arc::new(Mutex::new(CommandStatus::Waiting)),
      started_at: Arc::new(Mutex::new(None)),
      duration_ms: Arc::new(Mutex::new(None)),
    }
  }

  pub async fn run_single_command(&self, timeout_str: Option<String>) {
    // Обновляем статус на Running
    *self.status.lock().await = CommandStatus::Running;
    *self.started_at.lock().await = Some(Instant::now());

    let timeout = timeout_str
      .as_deref()
      .and_then(|s| parse_duration::parse(s).ok());

    // Запускаем команду
    let command_future = tokio::process::Command::new("sh")
      .arg("-c")
      .arg(&self.command)
      .output();

    let output_result = if let Some(dur) = timeout {
      tokio::time::timeout(dur, command_future).await
    } else {
      Ok(command_future.await)
    };

    // Обновляем статус по результату
    let mut status = self.status.lock().await;
    let mut started_at = self.started_at.lock().await;
    let mut duration_ms = self.duration_ms.lock().await;

    match output_result {
      Ok(Ok(output)) if output.status.success() => {
        *status = CommandStatus::Done;
      }
      Ok(Ok(_)) | Ok(Err(_)) => {
        *status = CommandStatus::Failed;
      }
      Err(_) => {
        *status = CommandStatus::Timeout;
      }
    }

    if let Some(start) = *started_at {
      *duration_ms = Some(start.elapsed().as_millis());
      *started_at = None;
    }
  }
}

pub async fn execute_commands(file_commands: Vec<FileCommand>) -> Result<Vec<TaskState>> {
  let mut states = Vec::new();
  let mut handles = Vec::new();

  // Проверяем наличие всех команд перед запуском
  for file_cmd in &file_commands {
    if !&file_cmd.command_exists() {
      return Err(AppError::CommandNotFound {
        command: file_cmd.command.clone(),
        reason: "Command not found in PATH".to_string(),
      });
    }
  }

  // Группируем команды по имени группы
  let mut by_group: HashMap<String, Vec<FileCommand>> = HashMap::new();
  for cmd in file_commands {
    by_group
      .entry(cmd.group_name.clone())
      .or_default()
      .push(cmd);
  }

  for (_, group_cmds) in by_group {
    if group_cmds.is_empty() {
      continue;
    }

    let order = group_cmds[0].execution_order;

    match order {
      ExecutionOrder::Parallel => {
        // Параллельный запуск с использованием JoinSet для управления задачами
        let mut join_set = tokio::task::JoinSet::new();

        for file_cmd in group_cmds {
          let state = TaskState::from_file_command(file_cmd.clone());
          let timeout_str = file_cmd.timeout.clone();
          let state_clone = state.clone();

          states.push(state.clone());

          let abort = join_set.spawn(async move {
            state_clone.run_single_command(timeout_str).await;
          });
        }

        // Сохраняем JoinSet для ожидания завершения
        handles.push(join_set);
      }
      ExecutionOrder::Sequential => {
        let mut join_set = tokio::task::JoinSet::new();

        // Последовательный запуск: одна задача на группу
        let group_states: Vec<_> = group_cmds
          .iter()
          .map(|file_cmd| {
            let state = TaskState::from_file_command(file_cmd.clone());
            states.push(state.clone());
            (state, file_cmd.timeout.clone())
          })
          .collect();

        let abort = join_set.spawn(async move {
          for (state, timeout_str) in group_states {
            state.run_single_command(timeout_str).await;
          }
        });

        handles.push(join_set);
      }
    }
  }

  // Ожидаем завершения всех задач
  for mut join_set in handles {
    while let Some(result) = join_set.join_next().await {
      if let Err(e) = result {
        // Логируем ошибку, но не прерываем выполнение других задач
        eprintln!("Task failed with error: {:?}", e);
      }
    }
  }

  Ok(states)
}
