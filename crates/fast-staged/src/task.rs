use crate::app::AppError;
use crate::app::Result;
use crate::command::{CommandStatus, StatusDisplay};
use crate::config::ExecutionOrder;
use crate::file::FileCommand;
use ratatui::style::Color;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::timeout;

#[derive(Debug)]
pub struct TaskPool {
  pub states: Vec<Task>,
  pub join_set: JoinSet<()>,
}

impl TaskPool {
  pub fn new() -> Self {
    Self {
      states: Vec::new(),
      join_set: JoinSet::new(),
    }
  }

  pub fn add(&mut self, state: Task) {
    self.states.push(state);
  }

  pub fn is_empty(&self) -> bool {
    self.states.is_empty()
  }

  pub fn get_states(&self) -> &Vec<Task> {
    &self.states
  }

  pub async fn get_command_stats(&self) -> HashMap<String, (usize, u128)> {
    let states = &self.states;
    let durations = &self.durations().await;
    let mut command_stats: HashMap<String, (usize, u128)> = HashMap::new();

    for (state, duration) in states.iter().zip(durations.iter()) {
      let entry = command_stats.entry(state.command.clone()).or_insert((0, 0));
      entry.0 += 1;
      if *duration > 0 {
        entry.1 += *duration;
      }
    }

    command_stats
  }

  pub async fn get_command_list(&self) -> Vec<(String, Color, u128)> {
    let statuses = &self.statuses().await;
    let durations = &self.durations().await;

    self
      .get_states()
      .iter()
      .enumerate()
      .map(|(idx, state)| {
        let status = &statuses[idx];
        let duration = durations[idx];
        let (symbol, color) = status.colored();
        let text = match status {
          CommandStatus::Failed(msg) => {
            format!(
              "{} {}: {} - {}ms ({})",
              symbol, state.filename, state.command, duration, msg
            )
          }
          _ => format!(
            "{} {}: {} - {}ms",
            symbol, state.filename, state.command, duration
          ),
        };

        (text, color, duration)
      })
      .collect()
  }

  pub async fn get_total_execution_time(&self) -> u128 {
    let durations = &self.durations().await;
    let statuses = &self.statuses().await;

    statuses
      .iter()
      .zip(durations.iter())
      .map(|(status, duration)| match status {
        CommandStatus::Done | CommandStatus::Failed(_) => *duration,
        _ => 0,
      })
      .sum()
  }

  pub async fn durations(&self) -> Vec<u128> {
    let mut durations = Vec::new();

    for state in &self.states {
      let duration = state.get_duration_ms().await;

      durations.push(duration);
    }

    durations
  }

  pub async fn statuses(&self) -> Vec<CommandStatus> {
    let mut statuses = Vec::new();

    for state in &self.states {
      let status = state.get_status().await;

      statuses.push(status);
    }

    statuses
  }

  pub async fn pull_task(&mut self) -> Result<()> {
    if let Some(res) = self.join_set.join_next().await {
      return res.map_err(|err| AppError::TaskJoinError(err));
    }

    Ok(())
  }

  pub async fn all_done(&self) -> Result<bool> {
    let task_len = self.join_set.len();
    let mut all_done = Vec::new();

    for state in &self.states {
      let done = state.get_done().await;

      all_done.push(done);
    }

    println!("task_len {:?}", task_len);

    Ok(task_len == 0)

    // Ok(all_done.iter().all(|done| *done == true))
  }

  pub async fn execute_commands(&mut self, file_commands: Vec<FileCommand>) -> Result<()> {
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

          for file_cmd in group_cmds {
            let state = Task::from_file_command(file_cmd.clone());

            self.add(state.clone());

            self.join_set.spawn(async move {
              state
                .clone()
                .run_single_command(file_cmd.clone().timeout)
                .await;
            });
          }
        }
        ExecutionOrder::Sequential => {
          // Последовательный запуск: одна задача на группу
          let group_states: Vec<_> = group_cmds
            .iter()
            .map(|file_cmd| {
              let state = Task::from_file_command(file_cmd.clone());

              self.add(state.clone());

              (state.clone(), file_cmd.timeout.clone())
            })
            .collect();

          self.join_set.spawn(async move {
            for (state, timeout_str) in group_states {
              state.run_single_command(timeout_str).await;
            }
          });
        }
      }
    }

    Ok(())
  }
}

#[derive(Clone, Debug)]
pub struct Task {
  pub filename: String,
  pub command: String,
  // pub group_name: Option<String>,
  pub status: Arc<Mutex<CommandStatus>>,
  pub started_at: Arc<Mutex<Option<Instant>>>,
  pub duration_ms: Arc<Mutex<Option<u128>>>,
  pub done: Arc<Mutex<bool>>,
}

impl Task {
  pub fn from_file_command(file_cmd: FileCommand) -> Self {
    Task {
      filename: file_cmd.filename.clone(),
      command: file_cmd.command.clone(),
      // group_name: Some(file_cmd.group_name.clone()),
      status: Arc::new(Mutex::new(CommandStatus::Waiting)),
      started_at: Arc::new(Mutex::new(None)),
      duration_ms: Arc::new(Mutex::new(None)),
      done: Arc::new(Mutex::new(false)),
    }
  }

  pub async fn get_done(&self) -> bool {
    self.done.lock().await.clone()
  }

  pub async fn get_status(&self) -> CommandStatus {
    self.status.lock().await.clone()
  }

  pub async fn get_duration_ms(&self) -> u128 {
    self.duration_ms.lock().await.clone().unwrap_or(0)
  }

  pub async fn set_status(&self, status: CommandStatus) {
    *self.status.lock().await = status;
  }

  pub async fn set_duration_ms(&self, duration_ms: u128) {
    *self.duration_ms.lock().await = Some(duration_ms);
  }

  pub async fn set_started_at(&self, started_at: Option<Instant>) {
    *self.started_at.lock().await = started_at;
  }

  pub async fn set_done(&self) {
    *self.done.lock().await = true;
  }

  pub async fn parse_timeout(&self, timeout_str: Option<String>) -> Option<Duration> {
    timeout_str
      .as_deref()
      .and_then(|s| parse_duration::parse(s).ok())
  }

  pub async fn run_single_command(&self, timeout_str: Option<String>) {
    // Обновляем статус на Running
    let started = Instant::now();

    self.set_status(CommandStatus::Running).await;
    self.set_started_at(Some(started)).await;

    // let timeout_dur = self.parse_timeout(timeout_str).await;

    // Запускаем команду
    // let command_future = Command::new("sh").arg("-c").arg(&self.command).output();

    let command_future = Command::new(&self.command).output();

    // if let Some(dur) = timeout_dur {
    //   println!("timeout {:?}", dur);

    //   let status = match timeout(dur, command_future).await {
    //     Ok(output) if output.is_ok() => CommandStatus::Done,
    //     Ok(output) if output.is_err() => {
    //       let message = output.unwrap_err().to_string();
    //       println!("timeout output error {:?}", message);
    //       CommandStatus::Failed(message)
    //     }
    //     Ok(_) => CommandStatus::Failed("timeout".to_string()),
    //     Err(err) => {
    //       println!("timeout error {:?}", err);

    //       CommandStatus::Timeout
    //     }
    //   };

    //   println!("Running command status: {:?}", status);

    //   self.set_status(status).await;
    // } else {
    //   let status = match command_future.await {
    //     Ok(_) => CommandStatus::Done,
    //     Err(err) => {
    //       let message = err.to_string();

    //       CommandStatus::Failed(message)
    //     }
    //   };

    //   println!("Running command status: {:?}", status);

    //   self.set_status(status).await;
    // };

    let status = match command_future.await {
      Ok(_) => CommandStatus::Done,
      Err(err) => {
        let message = err.to_string();

        CommandStatus::Failed(message)
      }
    };

    self.set_status(status).await;
    self.set_duration_ms(started.elapsed().as_millis()).await;
    self.set_done().await;
  }
}
