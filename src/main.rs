mod hook;
mod rewrite;

use std::io::{self, Read};

fn main() {
    if let Err(err) = run() {
        hook::log(&format!("fail-open error={err}"));
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "--version") {
        println!("rtk-codex-hook {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if args.first().is_some_and(|arg| arg == "--explain") {
        args.remove(0);
        let command = args.join(" ");
        if let Some(suggestion) = rewrite::suggest(&command) {
            println!("{suggestion}");
        }
        return Ok(());
    }

    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin)?;
    if let Some(output) = hook::handle_stdin(&stdin) {
        println!("{output}");
    }
    Ok(())
}
