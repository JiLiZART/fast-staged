

#[derive(Debug, PartialEq, Clone)]
enum CommandStatus {
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

pub fn command_exists(command: &str) -> bool {
  // Проверяем наличие команды в PATH
  // Для команд вида "sh -c 'command'" проверяем наличие 'sh'
  if command.starts_with("sh -c") {
    return which::which("sh").is_ok();
  }

  // Извлекаем первую часть команды (до пробела)
  let first_part = command.split_whitespace().next().unwrap_or(command);

  which::which(first_part).is_ok()
}

pub async fn run_single_command(state: TaskState, timeout_str: Option<String>) {
  // Обновляем статус на Running
  *state.status.lock().await = CommandStatus::Running;
  *state.started_at.lock().await = Some(Instant::now());

  let timeout = timeout_str.as_deref().and_then(|s| parse(s).ok());

  // Запускаем команду
  let command_future = tokio::process::Command::new("sh")
    .arg("-c")
    .arg(&state.command)
    .output();

  let output_result = if let Some(dur) = timeout {
    tokio::time::timeout(dur, command_future).await
  } else {
    Ok(command_future.await)
  };

  // Обновляем статус по результату
  let mut status = state.status.lock().await;
  let mut started_at = state.started_at.lock().await;
  let mut duration_ms = state.duration_ms.lock().await;

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

pub async fn execute_commands(file_commands: Vec<FileCommand>) -> Result<Vec<TaskState>> {
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
        // Параллельный запуск: как раньше, но только для этой группы
        for file_cmd in group_cmds {
          let state = TaskState {
            filename: file_cmd.filename.clone(),
            command: file_cmd.command.clone(),
            group_name: Some(file_cmd.group_name.clone()),
            status: Arc::new(Mutex::new(CommandStatus::Waiting)),
            started_at: Arc::new(Mutex::new(None)),
            duration_ms: Arc::new(Mutex::new(None)),
          };

          states.push(state.clone());
          let timeout_str = file_cmd.timeout.clone();
          let state_clone = state.clone();

          tokio::spawn(async move {
            run_single_command(state_clone, timeout_str).await;
          });
        }
      }
      ExecutionOrder::Sequential => {
        // Последовательный запуск: одна задача на группу
        let mut group_states = Vec::new();
        for file_cmd in group_cmds {
          let state = TaskState {
            filename: file_cmd.filename.clone(),
            command: file_cmd.command.clone(),
            group_name: Some(file_cmd.group_name.clone()),
            status: Arc::new(Mutex::new(CommandStatus::Waiting)),
            started_at: Arc::new(Mutex::new(None)),
            duration_ms: Arc::new(Mutex::new(None)),
          };
          states.push(state.clone());
          group_states.push((state, file_cmd.timeout.clone()));
        }

        tokio::spawn(async move {
          for (state, timeout_str) in group_states {
            run_single_command(state, timeout_str).await;
          }
        });
      }
    }
  }

  // Возвращаем состояния сразу, не ждем завершения задач
  Ok(states)
}
