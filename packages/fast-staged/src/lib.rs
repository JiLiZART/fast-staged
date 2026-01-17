#![deny(clippy::all)]

use fast_staged::run;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio;

#[napi_derive::module_init]
fn init() {
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .thread_name("my-native-module")
    .build()
    .unwrap();
  create_custom_tokio_runtime(rt);
}

#[napi]
async fn main() {
  if let Err(e) = run().await {
    eprintln!("Error: {}", e);
    std::process::exit(1);
  }
}
