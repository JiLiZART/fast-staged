mod app;
mod command;
mod config;
mod event;
mod file;
mod model;
mod render;
mod task;

use crate::app::App;

pub async fn run() -> color_eyre::Result<()> {
  App::new().run().await
}
