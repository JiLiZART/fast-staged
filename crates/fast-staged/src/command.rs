// use serde::Deserialize;
use ratatui::prelude::*;

#[derive(Debug, PartialEq, Clone)]
pub enum CommandStatus {
  None,
  Waiting,
  Running,
  Done,
  Failed(String),
  Timeout,
  // Cancelled,
}

impl std::fmt::Display for CommandStatus {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CommandStatus::None => write!(f, "None"),
      CommandStatus::Waiting => write!(f, "Waiting"),
      CommandStatus::Running => write!(f, "Running"),
      CommandStatus::Done => write!(f, "Done"),
      CommandStatus::Failed(_) => write!(f, "Failed"),
      CommandStatus::Timeout => write!(f, "Timeout"),
      // CommandStatus::Cancelled => write!(f, "Cancelled"),
    }
  }
}

pub trait StatusDisplay {
  fn colored(&self) -> (&str, Color);
}

impl StatusDisplay for CommandStatus {
  fn colored(&self) -> (&str, Color) {
    match self {
      CommandStatus::None => ("⏳", Color::Gray),
      CommandStatus::Done => ("✓", Color::Green),
      CommandStatus::Failed(_) => ("✗", Color::Red),
      CommandStatus::Running => ("⟳", Color::Yellow),
      CommandStatus::Waiting => ("⏳", Color::Gray),
      CommandStatus::Timeout => ("⏱", Color::Magenta),
    }
  }
}
