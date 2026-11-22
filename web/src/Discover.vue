<script setup lang="ts">
import { apiFetch, native, signedIn } from "./api";
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import Icon from "./Icon.vue";
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
    const r = await apiFetch("/api/discover/news", {
      signal: controller.signal,
    });
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
const search = ref(""),
  section = ref<"Markets" | "Community" | "News">("Markets");
const fresh = computed(() =>
  rows.value.filter((r) => clock.value - r.ts <= 300000),
);
const breadth = computed(() => {
  const up = fresh.value.filter((r) => r.change > 0).length,
    down = fresh.value.filter((r) => r.change < 0).length,
    count = fresh.value.length;
  return {
    up,
    down,
    flat: count - up - down,
    count,
    upPct: count ? (up * 100) / count : 0,
    downPct: count ? (down * 100) / count : 0,
  };
});
type PublicBasket = {
  id: string;
  name: string;
  creator: string;
  users: number;
  pinned: boolean;
  revision: number;
  items: { pair: string; bps: number }[];
};
const community = ref<PublicBasket[]>([]),
  shelfMessage = ref("Loading baskets…");
async function loadCommunity() {
  if (native && !signedIn.value) {
    shelfMessage.value = "Sign in to explore public baskets.";
    return;
  }
  try {
    const r = await apiFetch("/api/discover/baskets", {
      cache: "no-store",
      signal: controller.signal,
    });
    if (!r.ok) throw Error();
    community.value = (await r.json()).baskets;
    shelfMessage.value = community.value.length ? "" : "No public baskets yet.";
  } catch {
    community.value = [];
    shelfMessage.value = "Community baskets unavailable.";
  }
}
const ranked = computed(() => {
  const mode = group.value;
  return fresh.value
    .filter((r) =>
      r.pair.toLowerCase().includes(search.value.trim().toLowerCase()),
    )
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
  void loadCommunity();
  timer = setInterval(() => (clock.value = Date.now()), 15000);
});
onBeforeUnmount(() => {
  controller.abort();
  clearInterval(timer);
});
</script>
<template>
  <div class="market-workspace">
    <div class="market-center">
      <section class="pulse" aria-label="24 hour market breadth">
        <div>
          <p class="source"><i />OKX SPOT <span> / </span> 24 HOURS</p>
          <h2>Market pulse</h2>
          <p class="pulse-value">
            {{ breadth.count ? Math.round(breadth.upPct) + "%" : "—" }}
            <small>advancing</small>
          </p>
          <div class="pulse-key">
            <span
              >Rising <b>{{ breadth.count ? breadth.up : "—" }}</b></span
            ><span
              >Falling <b>{{ breadth.count ? breadth.down : "—" }}</b></span
            ><span
              >Flat <b>{{ breadth.count ? breadth.flat : "—" }}</b></span
            >
          </div>
        </div>
        <div
          class="pulse-ring"
          aria-hidden="true"
          :style="{
            '--rise': breadth.upPct + '%',
            '--moving': breadth.upPct + breadth.downPct + '%',
          }"
        >
          <div>
            <strong>{{ breadth.count || "—" }}</strong
            ><small>pairs</small>
          </div>
        </div>
      </section>
      <nav class="market-sections" aria-label="Discovery sections">
        <button
          v-for="tab in ['Markets', 'Community', 'News'] as const"
          :key="tab"
          :aria-pressed="section === tab"
          @click="section = tab"
        >
          {{ tab }}
        </button>
      </nav>
      <section
        class="panel market-list"
        :class="{ chosen: section === 'Markets' }"
      >
        <div class="section-line">
          <h2>Spot markets</h2>
          <button
            class="tool"
            aria-label="Refresh markets"
            :disabled="pending"
            @click="refresh"
          >
            <Icon name="Regenerate" />
          </button>
        </div>
        <label class="market-search"
          ><span>Find a market</span
          ><input
            v-model="search"
            type="search"
            placeholder="Search by coin or pair"
        /></label>
        <div class="filters" aria-label="Market rankings">
          <button
            v-for="g in ['Popular', 'Gainers', 'Losers'] as const"
            :key="g"
            :aria-pressed="g === group"
            @click="group = g"
          >
            {{ g }}
          </button>
        </div>
        <p v-if="pending || message || !ranked.length" role="status">
          {{
            pending
              ? "Loading markets…"
              : message || "No recent markets match this ranking."
          }}
        </p>
        <table v-else>
          <thead>
            <tr>
              <th>Market</th>
              <th>Price · USDT</th>
              <th>24h</th>
              <th class="volume">Volume · USDT</th>
              <th><span class="screen-reader">Basket</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="r in ranked" :key="r.pair">
              <td>
                <div class="ticker">
                  <i aria-hidden="true">{{ r.pair.slice(0, 2) }}</i
                  ><span>{{ r.pair }}</span>
                </div>
              </td>
              <td>{{ r.price }}</td>
              <td :class="{ increase: r.change > 0, decrease: r.change < 0 }">
                {{ r.change > 0 ? "+" : "" }}{{ r.change.toFixed(2) }}%
              </td>
              <td class="volume">
                {{ Math.round(r.volume).toLocaleString() }}
              </td>
              <td>
                <button
                  class="add"
                  :aria-label="'Create basket with ' + r.pair"
                  @click="emit('basket', r.pair)"
                >
                  +
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </section>
    </div>
    <aside class="market-rail" aria-label="Discovery feed">
      <section class="panel feed" :class="{ chosen: section === 'Community' }">
        <div class="section-line">
          <h2>Community baskets</h2>
          <a href="#baskets" aria-label="Browse baskets"
            ><Icon name="ChevronRightMd"
          /></a>
        </div>
        <p v-if="shelfMessage" class="caption">{{ shelfMessage }}</p>
        <a
          v-for="b in community"
          :key="b.id"
          class="feed-item"
          :href="'#baskets?id=' + b.id"
        >
          <span class="caption"
            >{{ b.pinned ? "Eighred pick · " : "" }}{{ b.creator }} · v{{
              b.revision
            }}</span
          >
          <strong>{{ b.name }}</strong
          ><span class="weights" aria-hidden="true"
            ><i
              v-for="(i, ix) in b.items"
              :key="ix"
              :style="{ flex: i.bps, opacity: 1 - (ix % 5) * 0.16 }"
            ></i
          ></span>
          <small
            >{{ b.items.length }} assets · {{ b.users }}
            {{ b.users === 1 ? "participant" : "participants" }}</small
          >
        </a>
      </section>
      <section
        class="panel feed"
        :class="{ chosen: section === 'News' }"
        aria-label="Market news"
      >
        <div class="section-line">
          <h2>Market news</h2>
          <button
            class="tool"
            :disabled="newsState.startsWith('Loading')"
            aria-label="Refresh news"
            @click="news"
          >
            <Icon name="Regenerate" />
          </button>
        </div>
        <p v-if="newsState" role="status">{{ newsState }}</p>
        <article v-for="a in articles" :key="a.url" class="feed-item">
          <div class="caption">
            {{ a.source }} ·
            <time :datetime="new Date(a.publishedMs).toISOString()">{{
              new Date(a.publishedMs).toLocaleDateString(undefined, {
                month: "short",
                day: "numeric",
              })
            }}</time>
          </div>
          <a :href="a.url" target="_blank" rel="noopener noreferrer">{{
            a.title
          }}</a>
        </article>
      </section>
    </aside>
  </div>
</template>
<style scoped>
.market-workspace {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(240px, 0.41fr);
  gap: var(--spacing-12);
  margin-top: 20px;
}
.market-center,
.market-rail {
  min-width: 0;
}
.market-rail {
  display: flex;
  flex-direction: column;
  gap: var(--spacing-12);
}
.pulse {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: clamp(18px, 3vw, 30px);
  border-radius: var(--radius-lg);
  border: 1px solid #314458;
  background: linear-gradient(118deg, #101b29, #10151c 72%);
  margin-bottom: 14px;
}
.source,
.pulse-key,
.pulse-ring small,
.market-list td:not(:first-child) {
  font-family: var(--font-gtamericamono);
}
.source {
  color: var(--signal);
  font-size: 10px;
  letter-spacing: 0.13em;
  display: flex;
  align-items: center;
  gap: 8px;
}
.source i {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--signal);
}
.source span {
  color: var(--muted);
}
.pulse h2 {
  font-size: 19px;
  margin: 16px 0 1px;
}
.pulse-value {
  font-size: clamp(32px, 3.5vw, 53px);
  font-weight: 550;
  letter-spacing: -0.06em;
  line-height: 1.15;
}
.pulse-value small {
  font-size: 14px;
  color: var(--muted);
  font-weight: 400;
  letter-spacing: 0;
}
.pulse-key {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  font-size: 11px;
  color: var(--muted);
  margin-top: 16px;
}
.pulse-key b {
  color: var(--eighred-white);
}
.pulse-ring {
  flex: none;
  width: clamp(80px, 9vw, 125px);
  aspect-ratio: 1;
  border-radius: 50%;
  display: grid;
  place-items: center;
  background: conic-gradient(
    var(--signal) var(--rise),
    #537893 var(--rise) var(--moving),
    #35404b var(--moving)
  );
}
.pulse-ring div {
  width: 84%;
  height: 84%;
  background: #101820;
  display: grid;
  align-content: center;
  justify-items: center;
  border-radius: 50%;
}
.pulse-ring strong {
  font:
    550 clamp(20px, 2.4vw, 34px) / 1.2 ui-monospace,
    monospace;
}
.pulse-ring small {
  color: var(--muted);
}
.market-list,
.feed {
  padding: var(--spacing-16);
}
.section-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-height: 34px;
}
.section-line a {
  color: var(--signal);
}
.market-search {
  display: block;
  margin: 10px 0;
}
.market-search span,
.screen-reader {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
}
.market-search input {
  width: 100%;
  padding: 10px 12px;
  color: inherit;
  background: var(--base);
  border: 1px solid var(--line);
  border-radius: 6px;
  font: inherit;
}
.filters,
.market-sections {
  display: flex;
  gap: 5px;
  margin: 10px 0;
}
.filters button,
.market-sections button {
  padding: 6px 12px;
  color: var(--muted);
  border-radius: var(--radius-md);
}
.filters [aria-pressed="true"],
.market-sections [aria-pressed="true"] {
  background: #192937;
  color: var(--signal);
}
.market-list th,
.market-list td {
  text-align: right;
  padding: 10px 5px;
  font-size: 12px;
}
.market-list th:first-child,
.market-list td:first-child {
  text-align: left;
}
.market-list table {
  width: 100%;
  min-width: 0;
}
.ticker {
  display: flex;
  align-items: center;
  gap: 7px;
  font-weight: 600;
  overflow-wrap: anywhere;
}
.ticker i {
  font:
    600 9px ui-monospace,
    monospace;
  border-radius: 50%;
  background: #26323e;
  color: var(--signal);
  width: 25px;
  height: 25px;
  display: grid;
  place-items: center;
  flex: none;
}
.increase {
  color: var(--eighred-green);
}
.decrease {
  color: #ff8ba9;
}
.add {
  width: 28px;
  height: 28px;
  background: var(--hover);
  color: var(--signal);
  font-size: 18px;
  text-decoration: none;
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
}
.feed-item {
  display: grid;
  gap: 5px;
  padding: 12px 0;
  border-top: 1px solid var(--line);
  line-height: 1.4;
}
.feed-item:first-of-type {
  margin-top: 7px;
}
.feed-item small {
  color: var(--muted);
}
.feed-item:hover strong,
.feed-item a:hover {
  color: var(--signal);
}
.weights {
  height: 3px;
  display: flex;
  gap: 2px;
}
.weights i {
  background: var(--signal);
}
.market-sections {
  display: none;
}
@media (max-width: 950px) {
  .market-workspace {
    grid-template-columns: minmax(0, 1fr);
  }
  .market-sections {
    display: flex;
  }
  .market-list:not(.chosen),
  .market-rail > .feed:not(.chosen) {
    display: none;
  }
  .market-rail:has(> .chosen) {
    display: block;
  }
  .market-rail:not(:has(> .chosen)) {
    display: none;
  }
}
@media (max-width: 550px) {
  .market-workspace {
    margin-top: 12px;
    gap: 0;
  }
  .market-list,
  .feed {
    padding: 12px;
  }
  .market-list td,
  .market-list th {
    padding: 9px 3px;
    font-size: 11px;
  }
  .volume {
    display: none;
  }
  .ticker i {
    display: none;
  }
  .pulse-key {
    gap: 8px;
  }
}
</style>
