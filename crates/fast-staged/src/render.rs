use crossterm::{
  event::{DisableMouseCapture, EnableMouseCapture},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Frame;
use std::collections::HashMap;
use std::io;

use ratatui::{
  backend::CrosstermBackend,
  prelude::*,
  widgets::{Block, Borders, List, ListItem, Paragraph},
};

fn render_title<'a>(statuses_len: &'a usize, total_files: &'a usize) -> Paragraph<'a> {
  let title_text = format!(
    "Running {} tasks for {} file(s)...",
    statuses_len, total_files
  );

  Paragraph::new(title_text).block(Block::default().borders(Borders::empty()).title("Status"))
}

fn render_list<'a>(lines: &Vec<(String, Color, u128)>) -> List<'a> {
  let items: Vec<ListItem> = lines
    .iter()
    .enumerate()
    .map(|(_, (text, color, _))| {
      ListItem::new(text.clone()).style(Style::default().fg(color.clone()))
    })
    .collect();

  List::new(items).block(Block::default().borders(Borders::empty()).title("Tasks"))
}

fn render_total_time<'a>(total_execution_time: &'a u128, elapsed_time: &'a u128) -> Paragraph<'a> {
  Paragraph::new(format!(
    "Total execution time: {}ms | Elapsed: {}ms",
    total_execution_time, elapsed_time
  ))
  .style(Style::default().fg(Color::White))
}

fn render_command_stats<'a>(command_stats: &'a HashMap<String, (usize, u128)>) -> Paragraph<'a> {
  let mut stats_lines = Vec::new();
  for (command, (count, total)) in command_stats {
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

  Paragraph::new(stats_text)
    .block(
      Block::default()
        .borders(Borders::empty())
        .title("Command Statistics"),
    )
    .style(Style::default().fg(Color::Cyan))
}

pub fn render_frame<'a>(
  f: &mut Frame<'a>,
  command_lines: &Vec<(String, Color, u128)>,
  command_stats: &HashMap<String, (usize, u128)>,
  statuses_count: usize,
  total_files: usize,
  is_empty: bool,
  total_execution_time: u128,
  elapsed_time: u128,
) {
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
  f.render_widget(render_title(&statuses_count, &total_files), areas[0]);

  let content_areas = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
    .split(areas[1]);

  // Список задач
  if !is_empty {
    f.render_widget(render_list(&command_lines), content_areas[0]);
  }

  // Статистика по командам
  if !command_stats.is_empty() {
    f.render_widget(render_command_stats(&command_stats), areas[2]);
  }

  // Общее время выполнения команд
  f.render_widget(
    render_total_time(&total_execution_time, &elapsed_time),
    content_areas[1],
  )
}

pub fn setup_terminal() -> color_eyre::Result<ratatui::Terminal<CrosstermBackend<io::Stdout>>> {
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
  let backend = CrosstermBackend::new(stdout);
  let terminal = ratatui::Terminal::new(backend)?;
  Ok(terminal)
}

pub fn restore_terminal(
  mut terminal: ratatui::Terminal<CrosstermBackend<io::Stdout>>,
) -> color_eyre::Result<()> {
  disable_raw_mode()?;
  execute!(
    terminal.backend_mut(),
    LeaveAlternateScreen,
    DisableMouseCapture
  )?;
  terminal.show_cursor()?;
  Ok(())
}
