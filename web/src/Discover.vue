<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
const emit = defineEmits<{ basket: [pair: string] }>();
type Market = {
  pair: string;
  price: string;
  change: number;
  volume: number;
  ts: number;
};
const rows = ref<Market[]>([]),
  group = ref<"Popular" | "Gainers" | "Losers">("Popular");
const pending = ref(false),
  message = ref(""),
  clock = ref(Date.now());
const controller = new AbortController();
type Headline = {
  title: string;
  url: string;
  publishedMs: number;
  source: string;
};
const articles = ref<Headline[]>([]),
  newsState = ref("Loading announcements…");
async function news() {
  newsState.value = "Loading announcements…";
  try {
    const r = await fetch("/api/discover/news", { signal: controller.signal });
    if (!r.ok) throw Error();
    articles.value = (await r.json()).articles;
    newsState.value = articles.value.length
      ? ""
      : "No announcements available.";
  } catch {
    articles.value = [];
    newsState.value = "News feed unavailable.";
  }
}
let timer: ReturnType<typeof setInterval>;
const ranked = computed(() => {
  const fresh = rows.value.filter((r) => clock.value - r.ts <= 300000);
  const mode = group.value;
  return fresh
    .filter(
      (r) =>
        mode === "Popular" ||
        (mode === "Gainers" ? r.change > 0 : r.change < 0),
    )
    .sort(
      (a, b) =>
        (mode === "Popular"
          ? b.volume - a.volume
          : mode === "Gainers"
            ? b.change - a.change
            : a.change - b.change) || a.pair.localeCompare(b.pair),
    )
    .slice(0, 25);
});
async function refresh() {
  if (pending.value) return;
  pending.value = true;
  message.value = "";
  rows.value = [];
  const timeout = AbortSignal.timeout(12000);
  try {
    const res = await fetch(
      "https://www.okx.com/api/v5/market/tickers?instType=SPOT",
      {
        credentials: "omit",
        referrerPolicy: "no-referrer",
        cache: "no-store",
        signal: AbortSignal.any([controller.signal, timeout]),
      },
    );
    if (!res.ok) throw Error();
    const feed = await res.json();
    if (feed.code !== "0" || !Array.isArray(feed.data)) throw Error();
    const now = Date.now();
    clock.value = now;
    for (const t of feed.data) {
      if (
        t.instType !== "SPOT" ||
        typeof t.instId !== "string" ||
        !t.instId.endsWith("-USDT")
      )
        continue;
      const last = Number(t.last),
        open = Number(t.open24h),
        volume = Number(t.volCcy24h),
        ts = Number(t.ts);
      if (
        typeof t.last !== "string" ||
        !t.volCcy24h ||
        ![last, open, volume, ts].every(Number.isFinite) ||
        Math.min(last, open, volume, ts) <= 0 ||
        ts > now + 30000 ||
        now - ts > 300000
      )
        continue;
      rows.value.push({
        pair: t.instId.slice(0, -5) + "/USDT",
        price: t.last,
        change: (last / open - 1) * 100,
        volume,
        ts,
      });
    }
  } catch {
    message.value = "OKX market data is unavailable. Try refreshing.";
  } finally {
    pending.value = false;
  }
}
onMounted(() => {
  void refresh();
  void news();
  timer = setInterval(() => (clock.value = Date.now()), 15000);
});
onBeforeUnmount(() => {
  controller.abort();
  clearInterval(timer);
});
</script>
<template>
  <p class="caption">OKX spot · USDT markets · 24 hours</p>
  <div class="filters">
    <button
      v-for="g in ['Popular', 'Gainers', 'Losers'] as const"
      :key="g"
      :aria-pressed="g === group"
      @click="group = g"
    >
      {{ g }}</button
    ><button :disabled="pending" @click="refresh">Refresh markets</button>
  </div>
  <p>
    {{
      group === "Popular"
        ? "Ranked by 24-hour quote volume."
        : group === "Gainers"
          ? "Largest positive price changes first."
          : "Largest negative price changes first."
    }}
  </p>
  <p v-if="pending || message || !ranked.length" role="status">
    {{
      pending
        ? "Loading markets…"
        : message || "No recent markets match this ranking."
    }}
  </p>
  <section v-else class="panel discovery">
    <table>
      <thead>
        <tr>
          <th>Pair</th>
          <th>Last (USDT)</th>
          <th>24h change</th>
          <th>Volume (USDT)</th>
          <th>Basket</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in ranked" :key="r.pair">
          <td>{{ r.pair }}</td>
          <td>{{ r.price }}</td>
          <td>{{ r.change.toFixed(2) }}%</td>
          <td>{{ Math.round(r.volume).toLocaleString() }}</td>
          <td>
            <button
              :aria-label="'Create basket with ' + r.pair"
              @click="emit('basket', r.pair)"
            >
              Add
            </button>
          </td>
        </tr>
      </tbody>
    </table>
  </section>
  <section class="panel headlines" aria-label="Market news">
    <div class="filters">
      <h2>Market news</h2>
      <button :disabled="newsState.startsWith('Loading')" @click="news">
        Refresh news
      </button>
    </div>
    <p class="caption">Exchange announcements · OKX</p>
    <p v-if="newsState" role="status">{{ newsState }}</p>
    <article v-for="a in articles" :key="a.url">
      <a :href="a.url" target="_blank" rel="noopener noreferrer">{{
        a.title
      }}</a>
      <p class="caption">
        {{ a.source }} ·
        <time :datetime="new Date(a.publishedMs).toISOString()">{{
          new Date(a.publishedMs).toLocaleString()
        }}</time>
      </p>
    </article>
  </section>
</template>
<style scoped>
.headlines {
  padding: 1rem;
  margin-top: 1rem;
}
.headlines article {
  padding: 0.65rem 0;
  border-bottom: 1px solid var(--line);
}
.headlines a:hover {
  text-decoration: underline;
}
.filters {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
  margin: 0.8rem 0;
}
button {
  cursor: pointer;
  color: inherit;
  background: var(--base);
  font: inherit;
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 0.4rem 0.65rem;
}
button[aria-pressed="true"] {
  border-color: var(--eighred-blue);
}
.discovery {
  margin-top: 1rem;
  overflow-x: auto;
}
</style>
