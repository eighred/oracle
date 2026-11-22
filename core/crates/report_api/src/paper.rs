use oracle_backtest::paper::{CADENCE_MS, GRACE_MS, SAMPLES, Sample, plan, project};
use oracle_spot_amount::Amount;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};
use tokio_postgres::{Client, Transaction};

const DB_ERROR: &str = "paper journal database operation failed";

async fn now(tx: &Transaction<'_>) -> Result<u64, &'static str> {
    let row = tx
        .query_one(
            "SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
            &[],
        )
        .await
        .map_err(|_| DB_ERROR)?;
    u64::try_from(row.get::<_, i64>(0)).map_err(|_| "unsupported database clock")
}

// Draw at observation time. Rejection sampling gives every cent equal probability.
fn draw_mark() -> Result<Amount, &'static str> {
    let rng = SystemRandom::new();
    let mut bytes = [0; 2];
    loop {
        rng.fill(&mut bytes)
            .map_err(|_| "synthetic observation unavailable")?;
        let value = u32::from(u16::from_le_bytes(bytes));
        if value < 65_536 / 401 * 401 {
            let cents = 2300 + value % 401;
            return Amount::parse(&format!("{}.{:02}", cents / 100, cents % 100))
                .ok_or("invalid synthetic mark");
        }
    }
}

fn check_instruction(previous: &Value, projection: &Value) -> Result<(), &'static str> {
    for run in projection["runs"].as_array().ok_or("missing runs")? {
        let rule = run["rule"].as_str().ok_or("missing rule")?;
        let row = run["ledger"]
            .as_array()
            .and_then(|r| r.last())
            .ok_or("missing posting")?;
        let posting = &row["posting"];
        let side = match posting["state"].as_str() {
            Some("filled") => &posting["fill"]["side"],
            Some("rejected") => &posting["side"],
            Some("hold") => &Value::Null,
            _ => return Err("unknown posting state"),
        };
        if previous["next_instructions"].get(rule) != Some(side) {
            return Err("posting differs from committed instruction");
        }
    }
    Ok(())
}

struct Journal {
    samples: Vec<Sample>,
    projection: Value,
    terminal: Option<String>,
    receipts: Vec<i64>,
    instructions: Vec<Value>,
}

impl Journal {
    async fn read(tx: &Transaction<'_>, first: i64) -> Result<Self, &'static str> {
        let mut journal = Self {
            samples: Vec::new(),
            projection: Value::Null,
            terminal: None,
            receipts: Vec::new(),
            instructions: Vec::new(),
        };
        let rows = tx.query(
            "SELECT seq, kind, payload, recorded_ms FROM oracle_paper.events WHERE first_ms=$1 ORDER BY seq",
            &[&first],
        ).await.map_err(|_| DB_ERROR)?;
        for row in rows {
            let seq: i32 = row.get(0);
            let kind: String = row.get(1);
            if journal.terminal.is_some() || seq as usize != journal.samples.len() {
                return Err("paper journal sequence mismatch");
            }
            let event: Value = serde_json::from_slice(row.get::<_, &[u8]>(2))
                .map_err(|_| "invalid saved event")?;
            let recorded: i64 = row.get(3);
            let expected = first as u64 + seq as u64 * CADENCE_MS;
            if kind == "sample" {
                let sample: Sample = serde_json::from_value(event["sample"].clone())
                    .map_err(|_| "invalid saved observation")?;
                if sample.known_ms > recorded as u64
                    || journal
                        .receipts
                        .last()
                        .is_some_and(|last| *last >= sample.close_ms as i64)
                    || !(Amount::parse("23").unwrap()..=Amount::parse("27").unwrap())
                        .contains(&sample.mark)
                {
                    return Err("journal observation differs from the frozen source");
                }
                journal.samples.push(sample);
                let projection = project(first as u64, &journal.samples)?;
                check_instruction(&journal.projection, &projection)?;
                if projection != event["projection"] {
                    return Err("stored postings differ from replay");
                }
                journal
                    .instructions
                    .push(projection["next_instructions"].clone());
                journal.projection = projection;
            } else {
                let stopped = event["observed_ms"]
                    .as_u64()
                    .ok_or("missing stop receipt")?;
                if event["expected_ms"].as_u64() != Some(expected)
                    || stopped > recorded as u64
                    || (kind == "missed") != (stopped >= expected + GRACE_MS)
                {
                    return Err("invalid stop evidence");
                }
                journal.terminal = Some(kind);
            }
            journal.receipts.push(recorded);
        }
        Ok(journal)
    }
    fn state(&self) -> &str {
        self.terminal
            .as_deref()
            .unwrap_or(if self.samples.len() == SAMPLES {
                "complete"
            } else {
                "collecting"
            })
    }
}

async fn append(
    tx: &Transaction<'_>,
    first: i64,
    seq: i32,
    kind: &str,
    event: &Value,
) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(event).map_err(|_| "invalid event")?;
    tx.execute(
        "INSERT INTO oracle_paper.events(first_ms,seq,kind,payload) VALUES ($1,$2,$3,$4)",
        &[&first, &seq, &kind, &bytes],
    )
    .await
    .map_err(|_| "paper event refused or outside its recording window")?;
    Ok(())
}

pub async fn command(client: &mut Client, args: &[String]) -> Result<(), &'static str> {
    let (action, first_arg, slot) = match args {
        [action, first]
            if matches!(action.as_str(), "freeze" | "report" | "publish" | "cancel") =>
        {
            (action.as_str(), first, None)
        }
        [action, first, slot] if action == "sample" => (
            "sample",
            first,
            Some(slot.parse::<usize>().map_err(|_| "invalid sample index")?),
        ),
        _ => {
            return Err(
                "usage: paper freeze|report|publish|cancel FIRST_MS | paper sample FIRST_MS INDEX",
            );
        }
    };
    let first = first_arg
        .parse::<i64>()
        .ok()
        .filter(|f| *f > 0 && *f < i64::MAX - 10 * CADENCE_MS as i64)
        .ok_or("invalid UTC window")?;
    let manifest = plan(first as u64)?;
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|_| "invalid plan")?;
    let tx = client.transaction().await.map_err(|_| DB_ERROR)?;
    tx.query_one("SELECT pg_advisory_xact_lock($1)", &[&first])
        .await
        .map_err(|_| DB_ERROR)?;
    let saved = tx
        .query_opt(
            "SELECT manifest,frozen_ms FROM oracle_paper.experiments WHERE first_ms=$1",
            &[&first],
        )
        .await
        .map_err(|_| DB_ERROR)?;
    if let Some(row) = &saved {
        if row.get::<_, &[u8]>(0) != manifest_bytes {
            return Err("stored experiment requires its original engine and manifest");
        }
    } else {
        if action != "freeze" {
            return Err("paper window has not been frozen");
        }
        tx.execute(
            "INSERT INTO oracle_paper.experiments(first_ms,manifest) VALUES($1,$2)",
            &[&first, &manifest_bytes],
        )
        .await
        .map_err(|_| "window must be frozen before its first sample")?;
    }
    let journal = Journal::read(&tx, first).await?;
    let seq = journal.samples.len();
    let next = first as u64 + seq as u64 * CADENCE_MS;
    let output = match action {
        "freeze" => manifest,
        "report" | "publish" => {
            if action == "publish" && journal.state() == "collecting" {
                return Err("only a terminal experiment can be published");
            }
            let report = json!({"manifest":manifest,"manifest_sha256":super::digest(&manifest_bytes),
            "frozen_ms":saved.ok_or("missing experiment")?.get::<_,i64>(1),"state":journal.state(),
            "samples":journal.samples,"recorded_ms":journal.receipts,"projection":journal.projection,
            "committed_instructions":journal.instructions,"unobserved_slots":(seq..SAMPLES).collect::<Vec<_>>()});
            if action == "publish" {
                let bytes = serde_json::to_vec(&report).map_err(|_| "invalid paper report")?;
                let id = super::digest(&bytes);
                tx.execute("INSERT INTO oracle_read.reports(portfolio,report_id,payload,kind) VALUES('own-paper-01',$1,$2,'paper') ON CONFLICT DO NOTHING",&[&id,&bytes]).await.map_err(|_|"paper report publication failed")?;
                json!({"report_id":id})
            } else {
                report
            }
        }
        "sample" if slot.is_some_and(|i| i < journal.samples.len()) => {
            json!({"result":"already_recorded","index":slot})
        }
        "sample" if slot == Some(seq) && journal.terminal.is_some() => {
            json!({"result":journal.state(),"index":slot})
        }
        "cancel" if journal.terminal.as_deref() == Some("canceled") => {
            json!({"result":"canceled","index":seq})
        }
        _ => {
            if journal.state() != "collecting" {
                return Err("paper experiment is terminal");
            }
            if slot.is_some_and(|i| i != seq) {
                return Err("expected the next paper sample index");
            }
            let clock = now(&tx).await?;
            let kind = if clock >= next + GRACE_MS {
                "missed"
            } else if action == "cancel" {
                "canceled"
            } else {
                "sample"
            };
            let event = if kind == "sample" {
                if clock < next {
                    return Err("sample is not due");
                }
                let mark = draw_mark()?;
                let sample = Sample {
                    close_ms: next,
                    known_ms: now(&tx).await?,
                    mark,
                };
                let mut samples = journal.samples;
                samples.push(sample);
                let projection = project(first as u64, &samples)?;
                check_instruction(&journal.projection, &projection)?;
                json!({"sample":sample,"projection":projection})
            } else {
                json!({"expected_ms":next,"observed_ms":clock})
            };
            append(&tx, first, seq as i32, kind, &event).await?;
            json!({"result":kind,"index":seq})
        }
    };
    tx.commit().await.map_err(|_| DB_ERROR)?;
    println!("{output}");
    Ok(())
}
