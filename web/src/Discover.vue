<script setup lang="ts">
import { apiFetch } from "./api";
import { computed, watch, onMounted, onBeforeUnmount, ref } from "vue";
import TraderFeed from "./TraderFeed.vue";
import SpotTicket from "./SpotTicket.vue";
import Icon from "./Icon.vue";
import {
  directory,
  listings,
  type Listing,
  type Venue,
} from "./market_directory";
const emit = defineEmits<{ basket: [pair: string, venue: Venue] }>();
const categories = ["Popular", "Gainers", "Losers"] as const;
type Category = (typeof categories)[number];
const group = ref<Category>("Popular");
const quote = ref("USDT");
const limit = ref(25);
const rows = ref<Listing[]>([]);
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
    newsState.value = articles.value.length ? "" : "No headlines available.";
  } catch {
    articles.value = [];
    newsState.value = "News feed unavailable.";
  }
}
let timer: ReturnType<typeof setInterval>;
const trading = ref("");
const search = ref(""),
  section = ref<"Markets" | "Community" | "News">("Markets");
const fresh = computed(() => directory(rows.value, clock.value));
const currencies = computed(() =>
  Array.from(
    new Set(["USDT", quote.value, ...fresh.value.map((row) => row.quote)]),
  ).sort(),
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
const matches = computed(() =>
  fresh.value
    .filter(
      (r) =>
        r.quote === quote.value &&
        r.pair.toLowerCase().includes(search.value.trim().toLowerCase()) &&
        (group.value === "Popular" ||
          (group.value === "Gainers" ? r.change > 0 : r.change < 0)),
    )
    .sort(
      (a, b) =>
        (group.value === "Popular"
          ? b.volume - a.volume
          : group.value === "Gainers"
            ? b.change - a.change
            : a.change - b.change) || a.pair.localeCompare(b.pair),
    ),
);
const ranked = computed(() => matches.value.slice(0, limit.value));
async function refresh() {
  if (pending.value) return;
  pending.value = true;
  const venues = ["s1", "s2"] as const;
  const failed: string[] = [];
  await Promise.all(
    venues.map(async (venue) => {
      try {
        const snapshot = await listings(
          venue,
          AbortSignal.any([controller.signal, AbortSignal.timeout(25000)]),
        );
        clock.value = Date.now();
        rows.value = [
          ...rows.value.filter((r) => r.venue !== venue),
          ...snapshot,
        ];
      } catch {
        failed.push(venue);
      }
    }),
  );
  message.value =
    failed.length === 2
      ? "Market sources unavailable."
      : failed.length
        ? "Partial market coverage."
        : "";
  pending.value = false;
}
watch(section, (tab) => {
  if (tab === "News" && !articles.value.length) void news();
  if (tab === "Community" && !community.value.length) void loadCommunity();
});
onMounted(() => {
  void refresh();

  timer = setInterval(() => {
    clock.value = Date.now();
    if (document.visibilityState === "visible") void refresh();
  }, 60000);
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
  <SpotTicket v-if="trading" :pair="trading" @close="trading = ''" />
  <div class="market-workspace">
    <div class="market-center">
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
          <label class="quote-filter"
            >Quote currency
            <select v-model="quote" @change="limit = 25">
              <option v-for="c in currencies" :key="c">{{ c }}</option>
            </select></label
          >
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
        <p v-if="message" role="status">{{ message }}</p>
        <p v-if="!ranked.length" role="status">
          {{
            pending
              ? "Loading markets…"
              : "No recent markets match this ranking."
          }}
        </p>
        <table v-else>
          <thead>
            <tr>
              <th>Market</th>
              <th>Oracle price · {{ quote }}</th>
              <th>24h</th>
              <th class="volume">24h volume · {{ quote }}</th>
              <th><span class="screen-reader">Basket</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="r in ranked" :key="r.pair">
              <td>
                <button
                  class="ticker"
                  :aria-label="'Trade ' + r.pair"
                  @click="trading = r.pair"
                >
                  <i aria-hidden="true">{{ r.pair.slice(0, 2) }}</i
                  ><span>{{ r.pair }}</span>
                </button>
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
                  @click="emit('basket', r.pair, r.venue)"
                >
                  +
                </button>
              </td>
            </tr>
          </tbody>
        </table>
        <button
          class="more"
          v-if="matches.length > ranked.length"
          @click="limit += 25"
        >
          Show more markets
        </button>
        <p class="breadth-note">
          {{ matches.length }} pairs · Indicative prices · 24h window
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
        <TraderFeed v-if="section === 'Community'" />
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
  margin-top: 12px;
}
.market-workspace .panel {
  background: transparent;
  border: 0;
  box-shadow: none;
  padding: 4px 0;
}
.market-workspace .market-list:not(.chosen),
.market-workspace .feed:not(.chosen) {
  display: none;
}
.market-list td:not(:first-child) {
  font-family: var(--font-gtamericamono);
}
.quote-filter {
  margin-left: auto;
  color: var(--muted);
}
.market-sections {
  border-bottom: 1px solid var(--line);
}
.market-list tbody tr:hover,
.feed-item:hover {
  background: var(--color-obsidian);
}
.more {
  color: var(--signal);
  padding: 12px;
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
@media (max-width: 600px) {
  .volume {
    display: none;
  }
  .ticker i {
    display: none;
  }
  .market-list td,
  .market-list th {
    padding: 10px 3px;
  }
  .quote-filter {
    font-size: 12px;
  }
}
</style>
