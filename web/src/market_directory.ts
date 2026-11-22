import { apiFetch } from "./api";
export type Venue = "s1" | "s2";
export type Listing = {
  venue: Venue;
  pair: string;
  quote: string;
  price: string;
  open: number;
  volume: number;
  ts: number;
};

export async function listings(
  venue: Venue,
  signal: AbortSignal,
): Promise<Listing[]> {
  const response = await apiFetch("/api/discover/markets/" + venue, { signal });
  if (!response.ok) throw Error("Market data unavailable.");
  const result: Listing[] = (await response.json()).map((row: Listing) => ({
    venue,
    pair: row.pair,
    quote: row.quote,
    price: row.price,
    open: Number(row.open),
    volume: Number(row.volume),
    ts: Number(row.ts),
  }));
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

export async function resolveRoutes<T extends { pair: string; venue: string }>(
  items: T[],
): Promise<T[]> {
  if (items.every((item) => item.venue)) return items;
  const signal = AbortSignal.timeout(25000);
  const snapshots = await Promise.allSettled(
    (["s1", "s2"] as const).map((route) => listings(route, signal)),
  );
  const markets = directory(
    snapshots.flatMap((s) => (s.status === "fulfilled" ? s.value : [])),
    Date.now(),
  );
  return items.map((item) => {
    if (item.venue) return item;
    const pair = item.pair.trim().toUpperCase();
    const found = markets.find((row) => row.pair === pair);
    if (!found)
      throw Error("A current market is unavailable for " + pair + ".");
    return { ...item, pair, venue: found.venue };
  });
}
