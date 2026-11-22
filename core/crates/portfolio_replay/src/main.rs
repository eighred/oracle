use oracle_portfolio_replay::{FIXTURE, replay, risk};
use std::{env, fs::File, io::Read, process::ExitCode};

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1).peekable();
    let usage = "usage: oracle-portfolio-replay [--risk-bps CONFIDENCE] VALID_AT_MS KNOWN_AT_MS [DATASET.json]";
    let confidence = if args.next_if(|arg| arg == "--risk-bps").is_some() {
        Some(
            args.next()
                .ok_or(usage)?
                .parse::<u16>()
                .map_err(|_| usage)?,
        )
    } else {
        None
    };
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
    let json = match confidence {
        Some(bps) => serde_json::to_string_pretty(&risk::report(&snapshot, bps)?),
        None => serde_json::to_string_pretty(&snapshot),
    }
    .map_err(|e| e.to_string())?;
    println!("{json}");
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
