use std::collections::HashMap;

use ratatui::style::Color;

#[derive(Debug, Default, Clone)]
pub struct StateModel {
  pub running: bool,
  pub total_execution_time: u128,
  pub elapsed_time: u128,
  pub command_stats: HashMap<String, (usize, u128)>,
  pub command_lines: Vec<(String, Color, u128)>,
  pub total_files: usize,
  pub statuses_count: usize,
  pub is_empty: bool,
}
