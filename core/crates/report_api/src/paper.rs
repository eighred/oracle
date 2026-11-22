mod source;
use oracle_backtest::paper::{CADENCE_MS, GRACE_MS, SAMPLES, Sample, project};
use oracle_spot_amount::Amount;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};
use source::Source;
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
    source_refs: Vec<Value>,
    stop_reason: Option<String>,
}

impl Journal {
    async fn read(tx: &Transaction<'_>, first: i64, source: Source) -> Result<Self, &'static str> {
        let mut journal = Self {
            samples: Vec::new(),
            projection: Value::Null,
            terminal: None,
            receipts: Vec::new(),
            instructions: Vec::new(),
            source_refs: Vec::new(),
            stop_reason: None,
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
                {
                    return Err("journal observation differs from the frozen source");
                }
                if let Some(reference) = source.replay(expected, sample, &event)? {
                    journal.source_refs.push(reference);
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
                let source_gap = source != Source::Generated
                    && kind == "missed"
                    && stopped >= expected
                    && event["reason"].as_str().is_some_and(|r| {
                        matches!(
                            r,
                            "source_read_failed"
                                | "venue_throttled"
                                | "missing_minute"
                                | "late_receipt"
                        )
                    });
                if event["expected_ms"].as_u64() != Some(expected)
                    || stopped > recorded as u64
                    || (kind == "missed") != (stopped >= expected + GRACE_MS || source_gap)
                {
                    return Err("invalid stop evidence");
                }
                if source_gap {
                    journal.stop_reason = event["reason"].as_str().map(str::to_owned);
                }
                journal.terminal = Some(kind);
            }
            journal.receipts.push(recorded);
        }
        Ok(journal)
    }
    fn add_venue_evidence(&self, manifest: &Value, report: &mut Value) -> Result<(), &'static str> {
        if let Some(reason) = &self.stop_reason {
            report["stop_reason"] = json!(reason);
        }
        report["source_receipts"] = json!(self.source_refs);
        if self.state() == "complete" {
            let marks = self.samples.iter().map(|s| s.mark).collect::<Vec<_>>();
            let mut results = serde_json::Map::new();
            for run in self.projection["runs"]
                .as_array()
                .ok_or("missing paper runs")?
            {
                let closing = run["ledger"]
                    .as_array()
                    .and_then(|r| r.last())
                    .ok_or("missing closing ledger")?;
                let quantity = closing["quantity"]
                    .as_str()
                    .and_then(Amount::parse)
                    .ok_or("invalid closing inventory")?;
                let (var, es) = oracle_portfolio_replay::risk::minute_tail(&marks, quantity, 6250)
                    .ok_or("incomplete minute risk")?;
                results.insert(
                    run["rule"].as_str().ok_or("unknown paper rule")?.into(),
                    json!({"quantity":quantity,"value_at_risk":var,"expected_shortfall":es}),
                );
            }
            report["minute_risk"] = json!({"method":"equal_weight_spot_revaluation","horizon_ms":CADENCE_MS,
                "confidence_bps":6250,"sample_size":marks.len()-1,"quote":"USDT","by_rule":results});
        }
        if let Some(anchor) = self.samples.get(2) {
            let cash: Amount = serde_json::from_value(manifest["opening_cash"].clone())
                .map_err(|_| "invalid opening cash")?;
            let quantity: Amount = serde_json::from_value(manifest["opening_quantity"].clone())
                .map_err(|_| "invalid opening inventory")?;
            let (inventory, nav) = quantity
                .valuation(anchor.mark, cash)
                .map_err(|_| "invalid opening valuation")?;
            if let Some(runs) = self.projection["runs"].as_array()
                && runs.iter().any(|run| run["opening_nav"] != nav)
            {
                return Err("paper opening NAV differs from the source valuation");
            }
            report["opening_valuation"] = json!({"cash":cash,"quantity":quantity,"mark":anchor.mark,"inventory_value":inventory,
                "nav":nav,"quote":"USDT","valid_ms":anchor.close_ms,"known_ms":anchor.known_ms,
                "observation_sha256":self.source_refs[2]["response_json_sha256"]});
        }
        Ok(())
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

async fn capture_event(
    tx: &Transaction<'_>,
    source: Source,
    first: u64,
    next: u64,
    mut journal: Journal,
) -> Result<(&'static str, Value), &'static str> {
    let (sample, receipt) = if source == Source::Generated {
        let mark = draw_mark()?;
        (
            Sample {
                close_ms: next,
                known_ms: now(tx).await?,
                mark,
            },
            None,
        )
    } else {
        let result = source.observe(next).await;
        let clock = now(tx).await?;
        let observed = match result {
            Ok(receipt) if receipt.received_ms < next + GRACE_MS && clock < next + GRACE_MS => {
                receipt
            }
            other => {
                let reason = match other {
                    Err("venue throttled the read") => "venue_throttled",
                    Err("missing requested minute") => "missing_minute",
                    Ok(_) => "late_receipt",
                    _ => "source_read_failed",
                };
                return Ok((
                    "missed",
                    json!({"expected_ms":next,"observed_ms":clock,"reason":reason}),
                ));
            }
        };
        (
            Sample {
                close_ms: next,
                known_ms: observed.received_ms,
                mark: observed.close,
            },
            Some(json!(observed)),
        )
    };
    journal.samples.push(sample);
    let projection = project(first, &journal.samples)?;
    check_instruction(&journal.projection, &projection)?;
    let mut event = json!({"sample":sample,"projection":projection});
    if let Some(receipt) = receipt {
        event["venue_receipt"] = receipt;
    }
    source.replay(next, sample, &event)?;
    Ok(("sample", event))
}

pub async fn command(client: &mut Client, args: &[String]) -> Result<(), &'static str> {
    let (action, first_arg, slot) = match args {
        [action, first]
            if matches!(
                action.as_str(),
                "freeze"
                    | "freeze-ccxt-fixture"
                    | "freeze-binance"
                    | "report"
                    | "publish"
                    | "cancel"
            ) =>
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
                "usage: paper freeze|freeze-ccxt-fixture|freeze-binance|report|publish|cancel FIRST_MS | paper sample FIRST_MS INDEX",
            );
        }
    };
    let first = first_arg
        .parse::<i64>()
        .ok()
        .filter(|f| *f > 0 && *f < i64::MAX - 10 * CADENCE_MS as i64)
        .ok_or("invalid UTC window")?;
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
    let freeze_source = match action {
        "freeze" => Some(Source::Generated),
        "freeze-ccxt-fixture" => Some(Source::Fixture),
        "freeze-binance" => Some(Source::Binance),
        _ => None,
    };
    let stored = saved
        .as_ref()
        .map(|row| serde_json::from_slice::<Value>(row.get::<_, &[u8]>(0)))
        .transpose()
        .map_err(|_| "invalid stored manifest")?;
    let source = match freeze_source {
        Some(source) => source,
        None => Source::from_kind(
            stored.as_ref().ok_or("paper window has not been frozen")?["source_kind"]
                .as_str()
                .ok_or("missing source kind")?,
        )?,
    };
    let stored_scope = stored.as_ref().and_then(|m| m["source_scope_id"].as_i64());
    let manifest = source.manifest(&tx, first, stored_scope).await?;
    let manifest_bytes = crate::canonical_bytes(manifest.clone())?;
    if let Some(row) = &saved {
        if row.get::<_, &[u8]>(0) != manifest_bytes {
            return Err("stored experiment requires its original engine and manifest");
        }
    } else {
        tx.execute(
            "INSERT INTO oracle_paper.experiments(first_ms,manifest) VALUES($1,$2)",
            &[&first, &manifest_bytes],
        )
        .await
        .map_err(|_| "window must be frozen before its first sample")?;
    }
    let journal = Journal::read(&tx, first, source).await?;
    let seq = journal.samples.len();
    let next = first as u64 + seq as u64 * CADENCE_MS;
    let output = match action {
        "freeze" | "freeze-ccxt-fixture" | "freeze-binance" => manifest,
        "report" | "publish" => {
            if action == "publish" && journal.state() == "collecting" {
                return Err("only a terminal experiment can be published");
            }
            let mut report = json!({"manifest":manifest,"manifest_sha256":super::digest(&manifest_bytes),
            "frozen_ms":saved.ok_or("missing experiment")?.get::<_,i64>(1),"state":journal.state(),
            "samples":journal.samples,"recorded_ms":journal.receipts,"projection":journal.projection,
            "committed_instructions":journal.instructions,"unobserved_slots":(seq..SAMPLES).collect::<Vec<_>>()});
            if source != Source::Generated {
                journal.add_venue_evidence(&manifest, &mut report)?;
            }
            if action == "publish" {
                let bytes = crate::canonical_bytes(report)?;
                let id = super::digest(&bytes);
                let scope = manifest["source_scope_id"].as_i64();
                tx.execute("INSERT INTO oracle_read.reports(portfolio,report_id,payload,kind,source_scope) VALUES('own-paper-01',$1,$2,'paper',$3) ON CONFLICT DO NOTHING",&[&id,&bytes,&scope]).await.map_err(|_|"paper report publication failed")?;
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
            let (kind, event) = if kind == "sample" {
                if clock < next {
                    return Err("sample is not due");
                }
                capture_event(&tx, source, first as u64, next, journal).await?
            } else {
                (kind, json!({"expected_ms":next,"observed_ms":clock}))
            };
            append(&tx, first, seq as i32, kind, &event).await?;
            json!({"result":kind,"index":seq})
        }
    };
    tx.commit().await.map_err(|_| DB_ERROR)?;
    println!("{output}");
    Ok(())
}
