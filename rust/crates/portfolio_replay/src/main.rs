use oracle_portfolio_replay::{FIXTURE, replay};
use std::{env, fs::File, io::Read, process::ExitCode};

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let usage = "usage: oracle-portfolio-replay VALID_AT_MS KNOWN_AT_MS [DATASET.json]";
    let valid = args
        .next()
        .ok_or(usage)?
        .parse::<u64>()
        .map_err(|_| usage)?;
    let known = args
        .next()
        .ok_or(usage)?
        .parse::<u64>()
        .map_err(|_| usage)?;
    let path = args.next();
    if args.next().is_some() {
        return Err(usage.into());
    }
    let mut input = Vec::new();
    let bytes = if let Some(path) = path {
        File::open(path)
            .map_err(|e| e.to_string())?
            .take(FIXTURE.len() as u64 + 1)
            .read_to_end(&mut input)
            .map_err(|e| e.to_string())?;
        &input[..]
    } else {
        FIXTURE
    };
    let snapshot = replay(bytes, valid, known)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
