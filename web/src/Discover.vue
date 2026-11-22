<script setup lang="ts">
import { apiFetch, native, signedIn } from "./api";
import { computed, nextTick, onMounted, onBeforeUnmount, ref } from "vue";
import Icon from "./Icon.vue";
const emit = defineEmits<{ basket: [pair: string] }>();
type Market = {
  pair: string;
  price: string;
  change: number;
  volume: number;
  ts: number;
};
const categories = ["Popular", "Gainers", "Losers"] as const;
type Category = (typeof categories)[number];
const titles = {
  Popular: "Volume leaders",
  Gainers: "Top gainers",
  Losers: "Top losers",
};
const rows = ref<Market[]>([]);
const group = ref<Category>("Popular");
const marketHeading = ref<HTMLHeadingElement>();
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
function orderMarkets(markets: Market[], category: Category) {
  return markets
    .filter(
      (r) =>
        category === "Popular" ||
        (category === "Gainers" ? r.change > 0 : r.change < 0),
    )
    .sort(
      (a, b) =>
        (category === "Popular"
          ? b.volume - a.volume
          : category === "Gainers"
            ? b.change - a.change
            : a.change - b.change) || a.pair.localeCompare(b.pair),
    );
}
const rising = computed(() => fresh.value.filter((r) => r.change > 0).length);
const falling = computed(() => fresh.value.filter((r) => r.change < 0).length);
const leaders = computed(() =>
  categories.map((category) => ({
    category,
    market: orderMarkets(fresh.value, category)[0],
  })),
);
const ranked = computed(() => {
  const query = search.value.trim().toLowerCase();
  return orderMarkets(
    fresh.value.filter((r) => r.pair.toLowerCase().includes(query)),
    group.value,
  ).slice(0, 25);
});
async function openRanking(category: Category) {
  group.value = category;
  section.value = "Markets";
  search.value = "";
  await nextTick();
  marketHeading.value?.focus();
  marketHeading.value?.scrollIntoView({ block: "start" });
}
const volumeFormat = new Intl.NumberFormat("en", {
  notation: "compact",
  maximumFractionDigits: 2,
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
  <Teleport defer to="#discovery-search">
    <label class="market-search"
      ><span>Find a market</span
      ><input
        v-model="search"
        type="search"
        @input="section = 'Markets'"
        placeholder="Search by coin or pair"
    /></label>
  </Teleport>
  <div class="market-workspace">
    <div class="market-center">
      <section class="spotlight" aria-label="Featured market rankings">
        <button
          v-for="{ category, market } in leaders"
          :key="category"
          class="cover"
          :class="category.toLowerCase()"
          :aria-label="'Explore ' + titles[category]"
          @click="openRanking(category)"
        >
          <span class="cover-tag">OKX SPOT · 24H</span>
          <span class="cover-copy">
            <strong>{{ titles[category] }}</strong>
            <span v-if="market"
              >{{ market.pair }} ·
              {{
                category === "Popular"
                  ? volumeFormat.format(market.volume) + " USDT"
                  : (market.change > 0 ? "+" : "") +
                    market.change.toFixed(2) +
                    "%"
              }}</span
            >
            <span v-else>{{
              pending ? "Loading markets…" : "No recent leader"
            }}</span>
          </span>
          <Icon name="ChevronRightMd" />
        </button>
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
          <h2 ref="marketHeading" tabindex="-1">Spot markets</h2>
          <button
            class="tool"
            aria-label="Refresh markets"
            :disabled="pending"
            @click="refresh"
          >
            <Icon name="Regenerate" />
          </button>
        </div>
        <div class="filters" aria-label="Market rankings">
          <button
            v-for="g in categories"
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
        <p v-if="fresh.length" class="breadth-note">
          OKX spot · {{ fresh.length }} fresh pairs · {{ rising }} rising ·
          {{ falling }} falling · {{ fresh.length - rising - falling }} flat
        </p>
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
          class="feed-item basket-entry"
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
.spotlight {
  display: grid;
  grid-template-columns: 1.3fr 1fr 1fr;
  gap: 8px;
  margin-bottom: 20px;
}
.cover {
  position: relative;
  min-width: 0;
  min-height: clamp(235px, 22vw, 340px);
  padding: clamp(12px, 1.3vw, 22px);
  border-radius: var(--radius-lg);
  overflow: hidden;
  text-align: left;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: space-between;
  background-color: var(--color-obsidian);
  isolation: isolate;
  color: white;
}
.cover::before {
  content: "";
  position: absolute;
  inset: 0;
  z-index: -1;
  background:
    linear-gradient(0deg, #05060af2, #07101817 72%),
    var(--art) center 44% / cover;
  transition: transform 0.2s ease-out;
}
@media (hover: hover) and (prefers-reduced-motion: no-preference) {
  .cover:hover::before {
    transform: scale(1.04);
  }
}
.cover.popular {
  --art: url("./assets/volume_leaders.webp");
}
.cover.gainers {
  --art: url("./assets/top_gainers.webp");
}
.cover.losers {
  --art: url("./assets/top_losers.webp");
}
.cover-tag {
  font: 600 10px/1.5 var(--font-gtamericamono);
  letter-spacing: 0.05em;
  padding: 3px 8px;
  background: #080809af;
  border-radius: 12px;
}
.cover-copy {
  display: grid;
  gap: 5px;
}
.cover-copy strong {
  font-size: clamp(19px, 2.25vw, 32px);
  line-height: 1.08;
  letter-spacing: -0.045em;
}
.cover-copy > span {
  font: 12px/1.4 var(--font-gtamericamono);
  color: #e4e8ed;
}
.cover > .icon {
  position: absolute;
  right: 10px;
  top: 10px;
  border-radius: 50%;
  background: #080809aa;
  padding: 3px;
  width: 23px;
  height: 23px;
}
.market-list td:not(:first-child) {
  font-family: var(--font-gtamericamono);
}
.market-workspace .panel {
  background: var(--color-void);
  box-shadow: none;
  border-color: var(--color-graphite);
}
.market-list .section-line h2 {
  scroll-margin-top: 72px;
}
.market-list tbody tr:hover,
.basket-entry:hover {
  background: var(--color-obsidian);
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
}
.breadth-note {
  color: var(--muted);
  font-size: 12px;
  margin-top: 12px;
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
  background: var(--color-obsidian);
  border: 1px solid var(--line);
  border-radius: 18px;
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
  .spotlight {
    display: flex;
    overflow-x: auto;
    scroll-snap-type: x mandatory;
    padding: 3px;
    margin: 0 -3px 12px;
  }
  .cover {
    flex: 0 0 78%;
    min-height: 208px;
    scroll-snap-align: center;
  }
  .cover-copy strong {
    font-size: 26px;
  }
}
</style>
