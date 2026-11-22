export type Rule = "hold" | "dca" | "momentum";
export const names: Record<Rule, string> = {
  hold: "Buy & hold",
  dca: "Periodic buys",
  momentum: "Momentum",
};
export const colors: Record<Rule, string> = {
  hold: "--eighred-blue",
  dca: "--eighred-teal",
  momentum: "--eighred-pink",
};
export interface Snapshot {
  portfolio_id: string;
  base: string;
  quote: string;
  nav: string;
  cash: string;
  quantity: string;
  mark: string;
  valid_at_ms: number;
  known_at_ms: number;
  observation_id: string;
  dataset_sha256: string;
}
export interface Fill {
  side: "buy" | "sell";
  price: string;
  quantity: string;
  cash_delta: string;
  quantity_delta: string;
  fee: string;
  half_spread_cost: string;
  slippage_cost: string;
  tick_rounding_cost: string;
}
export interface Row {
  decision_ms: number;
  signal_mark: string;
  valuation_close_ms: number;
  valuation_known_ms: number;
  valuation_mark: string;
  cash: string;
  quantity: string;
  nav: string;
  realized_pnl: string;
  unrealized_pnl: string;
  posting:
    | { state: "filled"; fill: Fill }
    | { state: "hold" }
    | { state: "rejected"; reason: string };
}
export interface Run {
  rule: Rule;
  opening_nav: string;
  closing_nav: string;
  net_pnl: string;
  total_fees: string;
  max_drawdown: string;
  ledger: Row[];
}
export interface Report {
  schema_version: 1;
  source_kind: "synthetic_historical";
  risk: {
    snapshot: Snapshot;
    risk_coverage: string;
    confidence_bps: number;
    sample_size?: number;
    value_at_risk?: string;
    expected_shortfall?: string;
    reason?: string;
    history_sha256: string;
  };
  backtest: {
    runs: Run[];
    benchmark: Rule;
    warmup_last_ms: number;
    holdout_last_ms: number;
    model: string;
    source_sha256: string;
    ledger_sha256: string;
  };
}
export function numeric(value: string) {
  const [n, d] = value.split("/");
  return Number(n) / Number(d ?? "1");
}
export function utc(time: number) {
  return new Date(time).toISOString().replace("T", " ").replace("Z", " UTC");
}
export function clock(time: number) {
  return utc(time).slice(11, 19);
}
