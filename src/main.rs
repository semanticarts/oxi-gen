use oxi_tarql::configure_transform;
use std::{env, time::Instant};

#[tokio::main]
async fn main() {
    // parse the supplied arguments
    let os_args: Vec<String> = env::args_os().map(|a| a.into_string().unwrap()).collect();
    let mut tarql = configure_transform(os_args);

    let start = Instant::now();

    tarql.transform().await.expect("Transformation failed");

    let duration = Instant::now().duration_since(start);
    eprintln!("Processing complete in {} seconds", duration.as_secs_f32());
}
