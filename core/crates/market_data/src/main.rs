use oracle_market_data::public_minute;
use serde_json::json;
use std::{env, process::ExitCode};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    // CCXT catches exchange exceptions after Rust invokes the panic hook. The
    // exchange message can contain the response body. Keep it off the CLI log.
    std::panic::set_hook(Box::new(|_| eprintln!("market data runtime exception")));
    let args: Vec<_> = env::args().skip(1).collect();
    let [arg] = args.as_slice() else {
        eprintln!("usage: oracle-market-data OPEN_MS");
        return ExitCode::FAILURE;
    };
    let Ok(open_ms) = arg.parse() else {
        eprintln!("invalid UTC timestamp");
        return ExitCode::FAILURE;
    };
    match public_minute(open_ms).await {
        Ok(observation) => {
            println!("{}", json!(observation));
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!(
                "{}",
                json!({"market_id":"BTCUSDT","open_ms":open_ms,"completeness":"incomplete","reason":reason})
            );
            ExitCode::FAILURE
        }
    }
}
