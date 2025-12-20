mod app;
mod command;
mod config;
mod file;
mod render;

use app::Result;
use command::execute_commands;
use config::load_config;
use file::{get_changed_files, match_files_to_commands};
use render::render_ui;

pub async fn run() -> Result<()> {
  // Загрузка конфигурации
  let config = load_config()?;

  // Получение измененных файлов
  let changed_files = get_changed_files().await?;
  let total_files = changed_files.len();

  // Сопоставление файлов с командами
  let file_commands = match_files_to_commands(&config, &changed_files)?;

  // Запуск команд и UI параллельно
  let (states, _) = execute_commands(file_commands).await?;

  render_ui(states, total_files).await?;

  // match signal::ctrl_c().await {
  //   Ok(()) => {
  //     std::process::exit(1);
  //   }
  //   Err(err) => {
  //     eprintln!("Unable to listen for shutdown signal: {}", err);
  //     // we also shut down in case of error
  //   }
  // }

  Ok(())
}
