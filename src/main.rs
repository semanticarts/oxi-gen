use oxi_tarql::configure_transform;
use std::{env, time::Instant};

fn main() {
    // parse the supplied arguments
    let os_args: Vec<String> = 
        env::args_os().into_iter().map(|a| a.into_string().unwrap()).collect();
    let mut tarql = configure_transform(os_args);

    let start = Instant::now();

    tarql.transform().expect("Oops something went wrong");

    let duration = start.duration_since(start);
    eprintln!("Processing complete in {} seconds", duration.as_secs_f32());
}
