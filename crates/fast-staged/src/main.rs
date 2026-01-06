use fast_staged::run;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;

  run().await
}
