use fast_staged::run;

#[tokio::main]
async fn main() {
  if let Err(e) = run().await {
    eprintln!("Error: {}", e);
    std::process::exit(1);
  }
}
