mod app;
mod command;
mod config;
mod file;
mod render;
mod task;

use crate::app::App;

pub async fn run() -> color_eyre::Result<()> {
  App::new().run().await
}
