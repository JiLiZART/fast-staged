use crate::app::AppError;
use crate::app::Result;
use crate::config::Config;
use crate::config::ExecutionOrder;
use crate::config::parse_groups_from_config;
use fast_glob::glob_match;
use gix::prelude::*;

#[derive(Debug, Clone)]
pub struct FileCommand {
  pub filename: String,
  pub command: String,
  pub group_name: String,
  pub timeout: Option<String>,
  pub execution_order: ExecutionOrder,
}

impl FileCommand {
  pub fn command_exists(&self) -> bool {
    let command = &self.command;
    // Проверяем наличие команды в PATH
    // Для команд вида "sh -c 'command'" проверяем наличие 'sh'
    if command.starts_with("sh -c") {
      return which::which("sh").is_ok();
    }

    // Извлекаем первую часть команды (до пробела)
    let first_part = command.split_whitespace().next().unwrap_or(command);

    which::which(first_part).is_ok()
  }
}

pub fn match_files_to_commands(
  config: &Config,
  changed_files: &[String],
) -> Result<Vec<FileCommand>> {
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
          // println!(
          //   "Found pattern {} for file {} in group {}",
          //   pattern, file, group.name
          // );

          for command in commands {
            file_commands.push(FileCommand {
              filename: file.clone(),
              command: command.clone(),
              group_name: group.name.clone(),
              timeout: group.timeout.clone(),
              execution_order: group.execution_order,
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

pub async fn get_changed_files() -> Result<Vec<String>> {
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
