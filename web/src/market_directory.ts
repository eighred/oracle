export type Venue = "binance" | "okx";
export type Listing = {
  venue: Venue;
  pair: string;
  quote: string;
  price: string;
  open: number;
  volume: number;
  ts: number;
};

const binanceOrigin = "https://data-api.binance.vision";
async function read(url: string, signal: AbortSignal) {
  const response = await fetch(url, {
    credentials: "omit",
    referrerPolicy: "no-referrer",
    cache: "no-store",
    signal,
  });
  if (!response.ok) throw Error("Market source unavailable");
  return response.json();
}

type BinanceInstrument = {
  symbol: string;
  baseAsset: string;
  quoteAsset: string;
  status: string;
  isSpotTradingAllowed: boolean;
};
let catalog:
  | { at: number; symbols: Map<string, BinanceInstrument> }
  | undefined;
async function symbols(signal: AbortSignal) {
  if (catalog && Date.now() - catalog.at < 3600000) return catalog.symbols;
  const rules = await read(binanceOrigin + "/api/v3/exchangeInfo", signal);
  const active = new Map<string, BinanceInstrument>();
  for (const row of rules.symbols as BinanceInstrument[]) {
    if (
      row.status === "TRADING" &&
      row.isSpotTradingAllowed &&
      row.baseAsset &&
      row.quoteAsset
    )
      active.set(row.symbol, row);
  }
  catalog = { at: Date.now(), symbols: active };
  return active;
}
export async function listings(
  venue: Venue,
  signal: AbortSignal,
): Promise<Listing[]> {
  const result: Listing[] = [];
  if (venue === "binance") {
    const [active, ticks] = await Promise.all([
      symbols(signal),
      read(binanceOrigin + "/api/v3/ticker/24hr", signal),
    ]);
    for (const tick of ticks) {
      const rule = active.get(tick.symbol);
      if (rule)
        result.push({
          venue,
          pair: rule.baseAsset + "/" + rule.quoteAsset,
          quote: rule.quoteAsset,
          price: tick.lastPrice,
          open: Number(tick.openPrice),
          volume: Number(tick.quoteVolume),
          ts: Number(tick.closeTime),
        });
    }
  } else {
    const data = await read(
      "https://www.okx.com/api/v5/market/tickers?instType=SPOT",
      signal,
    );
    if (data.code !== "0") throw Error("Market source unavailable");
    for (const tick of data.data) {
      if (tick.instType !== "SPOT" || typeof tick.instId !== "string") continue;
      const [base, quote, extra] = tick.instId.split("-");
      if (base && quote && extra === undefined)
        result.push({
          venue,
          pair: base + "/" + quote,
          quote,
          price: tick.last,
          open: Number(tick.open24h),
          volume: Number(tick.volCcy24h),
          ts: Number(tick.ts),
        });
    }
  }
  return result.filter(
    (row) =>
      typeof row.price === "string" &&
      row.price.length <= 64 &&
      /^\d+(?:\.\d+)?$/.test(row.price) &&
      [Number(row.price), row.open, row.volume, row.ts].every(
        (n) => Number.isFinite(n) && n > 0,
      ),
  );
}
function comparePrices(left: string, right: string) {
  const [a = "0", af = ""] = left.split(".");
  const [b = "0", bf = ""] = right.split(".");
  const places = Math.max(af.length, bf.length);
  const x = BigInt(a + af.padEnd(places, "0"));
  const y = BigInt(b + bf.padEnd(places, "0"));
  return x < y ? -1 : x > y ? 1 : 0;
}
export function directory(rows: Listing[], now: number) {
  const pairs = new Map<string, Listing[]>();
  for (const row of rows) {
    if (now - row.ts > 300000 || row.ts - now > 30000) continue;
    const group = pairs.get(row.pair);
    if (group) group.push(row);
    else pairs.set(row.pair, [row]);
  }
  return Array.from(pairs.values(), (legs) => {
    const leader = legs.reduce((a, b) =>
      comparePrices(a.price, b.price) >= 0 ? a : b,
    );
    const open = Math.max(...legs.map((l) => l.open));
    return {
      ...leader,
      change: (Number(leader.price) / open - 1) * 100,
      volume: legs.reduce((v, l) => v + l.volume, 0),
      sources: legs.length,
    };
  });
}
