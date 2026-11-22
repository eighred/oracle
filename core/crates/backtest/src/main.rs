use oracle_backtest::report;
use std::{
    env, fs,
    io::{self, Write},
    path::Path,
    process::ExitCode,
};
use tempfile::NamedTempFile;

fn save(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    match temp.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => {
            if fs::metadata(path)?.len() == bytes.len() as u64 && fs::read(path)? == bytes {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "existing report has different content",
                ))
            }
        }
        Err(e) => Err(e.error),
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let path = match args.as_slice() {
        [] => None,
        [option, path] if option == "--output" => Some(Path::new(path)),
        _ => return Err("usage: oracle-backtest [--output REPORT.json]".into()),
    };
    let mut bytes = serde_json::to_vec_pretty(&report()?).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    match path {
        Some(path) => save(path, &bytes),
        None => io::stdout().lock().write_all(&bytes),
    }
    .map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
