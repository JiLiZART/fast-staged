use crate::app::Result;
use crate::command::CommandStatus;
use crate::command::TaskState;
use std::collections::HashMap;
use std::io;
use std::time::Instant;

use crossterm::{
  event::{DisableMouseCapture, EnableMouseCapture},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use gix::trace::debug;
use ratatui::{
  backend::CrosstermBackend,
  prelude::*,
  widgets::{Block, Borders, List, ListItem, Paragraph},
};

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
      CommandStatus::Timeout => ("⏱", Color::Magenta),
    }
  }
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

pub async fn run_ui(states: Vec<TaskState>, total_files: usize) -> Result<()> {
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
        stats_lines.sort_by_key(|name| name.to_lowercase());
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

    debug!("all_done: {}", all_done);

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
