#![deny(clippy::all)]

use napi_derive::napi;
use fast_staged::run;

#[napi]
async fn main() {
  if let Err(e) = run().await {
    eprintln!("Error: {}", e);
    std::process::exit(1);
  }
}
