<script setup lang="ts">
import {
  apiFetch,
  native,
  signedIn,
  endSession,
  accountReady,
  accountState,
  memberHandle,
  memberLogin,
} from "./api";
import { defineAsyncComponent, computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import EquityChart from "./EquityChart.vue";
const PaperReview = defineAsyncComponent(() => import("./PaperReview.vue"));
import SpotRisk from "./SpotRisk.vue";
import PublicBasket from "./PublicBasket.vue";
import NativeLogin from "./NativeLogin.vue";
import Profile from "./Profile.vue";
import Baskets from "./Baskets.vue";
import Discover from "./Discover.vue";
import PlatformSettings from "./PlatformSettings.vue";
const selectedItems = ref<{ venue: string; pair: string; bps: number }[]>();
const newPair = ref<{ pair: string; venue: "s1" | "s2" }>();
const basketPage = ref(0);
function basketRisk(items: { venue: string; pair: string; bps: number }[]) {
  selectedItems.value = items;
  location.hash = "spot-risk";
}
function newBasket(pair: string, venue: "s1" | "s2") {
  newPair.value = { pair, venue };
  location.hash = "baskets";
}
import {
  clock,
  colors,
  names,
  numeric,
  utc,
  type Report,
  type Rule,
} from "./report";

const portfolio = "own-paper-01";
const data = ref<Report>(),
  revision = ref(""),
  status = ref("Loading portfolio…");
const active = ref<Rule>("momentum"),
  index = ref(0),
  pending = ref(false);
const run = computed(() =>
  data.value?.backtest.runs.find((s) => s.rule === active.value),
);
const row = computed(() => run.value?.ledger[index.value]);
const snap = computed(() => data.value?.risk.snapshot);
const endpoint = "/api/portfolios/" + portfolio + "/reports/";
function select(rule: Rule) {
  active.value = rule;
  index.value = 0;
}
const views = [
  { id: "overview", title: "Portfolio review", icon: "Chart" },
  { id: "profile", title: "My profile", icon: "Notebook" },
  { id: "strategies", title: "Strategy comparison", icon: "Compare" },
  { id: "ledger", title: "Settlement ledger", icon: "Notebook" },
  { id: "settings", title: "Platform settings", icon: "Folder" },
  { id: "discover", title: "Discover markets", icon: "Chart" },
  { id: "baskets", title: "Baskets", icon: "Folder" },
  { id: "spot-risk", title: "Spot portfolio risk", icon: "Chart" },
  { id: "paper", title: "Paper outcomes", icon: "Clock" },
] as const;
const consumerRoutes = ["discover", "baskets", "profile", "spot-risk"];
const navigation = native
  ? views.filter((item) => consumerRoutes.includes(item.id))
  : views;
const defaultView = views.find((item) => item.id === "discover")!;
function currentView() {
  return (
    navigation.find(
      (item) => "#" + item.id === window.location.hash.split("?")[0],
    ) ?? defaultView
  );
}
const view = ref(currentView());
const collapsed = ref(currentView().id === "discover");
const breakpoint = window.matchMedia("(max-width: 760px)");
const narrow = ref(breakpoint.matches);
const mobileMenu = ref<HTMLDialogElement>();
const inspector = ref<HTMLDialogElement>();
const heading = ref<HTMLHeadingElement>();
const paperReview = ref<InstanceType<typeof PaperReview>>();
const inspection = ref<"posting" | "provenance">("provenance");
function navigate() {
  if (view.value.id === "spot-risk") selectedItems.value = undefined;
  if (view.value.id === "baskets") newPair.value = undefined;
  basketPage.value++;
  view.value = currentView();
  document.title = view.value.title + " · Oracle";
  mobileMenu.value?.close();
  inspector.value?.close();
  if (
    ![
      "paper",
      "spot-risk",
      "discover",
      "baskets",
      "settings",
      "profile",
    ].includes(view.value.id) &&
    !data.value &&
    !pending.value
  )
    refresh();
  nextTick(() => heading.value?.focus());
}
function resize() {
  narrow.value = breakpoint.matches;
  mobileMenu.value?.close();
}
function toggleMenu() {
  if (narrow.value) mobileMenu.value?.showModal();
  else collapsed.value = !collapsed.value;
}
function inspect(kind: "posting" | "provenance", position = 0) {
  inspection.value = kind;
  index.value = position;
  inspector.value?.showModal();
}
async function refresh() {
  if (pending.value) return;
  inspector.value?.close();
  pending.value = true;
  data.value = undefined;
  revision.value = "";
  status.value = "Loading portfolio…";
  try {
    const response = await apiFetch(endpoint + "latest", {
      cache: "no-store",
      credentials: "same-origin",
    });
    if (!response.ok) {
      status.value =
        (
          {
            401: "Sign in with your operator credentials.",
            403: "Access to this portfolio is denied.",
            404: "No published report for this portfolio.",
          } as Record<number, string>
        )[response.status] ?? "Report service unavailable.";
      return;
    }
    const parsed: Report = await response.json();
    const id = response.headers.get("x-oracle-report-id");
    if (
      !id ||
      !/^[a-f0-9]{64}$/.test(id) ||
      parsed.schema_version !== 1 ||
      parsed.source_kind !== "synthetic_historical" ||
      parsed.risk.snapshot.portfolio_id !== portfolio ||
      !parsed.backtest.runs.some(
        (s) => s.rule === "momentum" && s.ledger.length,
      )
    ) {
      status.value = "Unsupported or incomplete report.";
      return;
    }
    revision.value = id;
    data.value = parsed;
    select("momentum");
    status.value = "";
  } catch {
    status.value = "Report service unavailable.";
  } finally {
    pending.value = false;
  }
}
onMounted(() => {
  document.title = view.value.title + " · Oracle";
  window.addEventListener("hashchange", navigate);
  breakpoint.addEventListener("change", resize);
  if (
    ![
      "paper",
      "spot-risk",
      "discover",
      "baskets",
      "settings",
      "profile",
    ].includes(view.value.id)
  )
    refresh();
});
onBeforeUnmount(() => {
  window.removeEventListener("hashchange", navigate);
  breakpoint.removeEventListener("change", resize);
});
</script>
<template>
  <a class="skip" href="#workstation" @click.prevent="heading?.focus()"
    >Skip to workspace</a
  >
  <div
    class="shell"
    :class="{
      collapsed: collapsed && !narrow,
      consumer: consumerRoutes.includes(view.id),
    }"
  >
    <dialog
      ref="mobileMenu"
      class="menu-dialog"
      aria-label="Workstation navigation"
      @click.self="mobileMenu?.close()"
    >
      <div id="mobile-navigation"></div>
    </dialog>
    <Teleport to="#mobile-navigation" defer :disabled="!narrow">
      <aside class="sidebar">
        <div class="brand">
          <span class="brand-mark" aria-hidden="true"></span><b>Oracle</b>
          <button
            v-if="narrow"
            class="tool menu-close"
            aria-label="Close navigation"
            @click="mobileMenu?.close()"
          >
            <Icon name="XCrossed" />
          </button>
        </div>
        <nav aria-label="Workspace">
          <p class="nav-caption">Workspace</p>
          <a
            v-for="item in navigation"
            :key="item.id"
            :href="'#' + item.id"
            :title="item.title"
            :aria-label="item.title"
            :aria-current="view.id === item.id ? 'page' : undefined"
            @click="mobileMenu?.close()"
          >
            <Icon :name="item.icon" /><span>{{ item.title }}</span>
          </a>
        </nav>
        <div class="identity">
          <b aria-hidden="true">E</b>
          <div>Eighred Research<small>Research workspace</small></div>
        </div>
      </aside>
    </Teleport>
    <div class="workspace">
      <header class="toolbar">
        <button
          class="tool"
          aria-label="Toggle navigation"
          :aria-expanded="narrow ? undefined : !collapsed"
          @click="toggleMenu"
        >
          <Icon name="Sidebar" />
        </button>
        <nav
          v-if="!narrow && consumerRoutes.includes(view.id)"
          class="market-nav"
          aria-label="Market navigation"
        >
          <a class="market-home" href="#discover">Oracle</a
          ><a
            href="#discover"
            :aria-current="view.id === 'discover' ? 'page' : undefined"
            >Markets</a
          ><a
            href="#baskets"
            :aria-current="view.id === 'baskets' ? 'page' : undefined"
            >Baskets</a
          ><a
            href="#profile"
            :aria-current="view.id === 'profile' ? 'page' : undefined"
            >Wallet</a
          >
        </nav>
        <span class="mobile-brand"
          ><span class="brand-mark" aria-hidden="true"></span>Oracle</span
        ><span class="crumb"
          ><Icon name="Folder" />{{
            consumerRoutes.includes(view.id) ? "Paper workspace" : portfolio
          }}</span
        >
        <Icon name="ChevronRightMd" /><span class="current">{{
          view.title
        }}</span>
        <div v-if="view.id === 'discover'" id="discovery-search"></div>
        <template v-if="accountReady && !signedIn"
          ><button @click="memberLogin()">Sign in</button
          ><button class="app-button" @click="memberLogin(true)">
            Register
          </button></template
        >
        <span v-if="memberHandle" class="caption">{{ memberHandle }}</span>
        <button v-if="signedIn" class="session-exit" @click="endSession">
          Sign out
        </button>
        <div
          v-if="
            ![
              'spot-risk',
              'baskets',
              'discover',
              'settings',
              'profile',
            ].includes(view.id)
          "
          class="actions"
        >
          <button
            class="tool"
            title="Refresh report"
            aria-label="Refresh report"
            :disabled="view.id === 'paper' ? paperReview?.loading : pending"
            @click="view.id === 'paper' ? paperReview?.refresh() : refresh()"
          >
            <Icon name="Regenerate" />
          </button>
          <button
            class="tool"
            title="Report provenance"
            v-if="view.id !== 'paper'"
            aria-label="Report provenance"
            :disabled="!data"
            aria-haspopup="dialog"
            @click="inspect('provenance')"
          >
            <Icon name="Info" />
          </button>
        </div>
      </header>
      <main id="workstation">
        <p v-if="accountState" role="status">{{ accountState }}</p>
        <h1 ref="heading" tabindex="-1">{{ view.title }}</h1>
        <PublicBasket
          v-if="view.id === 'baskets' && !signedIn && (accountReady || native)"
          :key="basketPage"
        />
        <section
          v-else-if="
            accountReady &&
            !signedIn &&
            ['baskets', 'profile', 'spot-risk'].includes(view.id)
          "
          class="panel member-entry"
        >
          <h2>Access your Oracle account</h2>
          <p>
            Use your email or username and authenticator to open your paper
            workspace.
          </p>
          <button class="app-button" @click="memberLogin()">
            Sign in to Oracle
          </button>
          <button @click="memberLogin(true)">Register with email</button>
        </section>
        <NativeLogin
          v-else-if="native && !signedIn && view.id !== 'discover'"
        />
        <PlatformSettings v-else-if="view.id === 'settings'" />
        <Profile v-else-if="view.id === 'profile'" />
        <Discover v-else-if="view.id === 'discover'" @basket="newBasket" />
        <Baskets
          v-else-if="view.id === 'baskets'"
          :key="basketPage"
          :initial-pair="newPair"
          @risk="basketRisk"
        />
        <SpotRisk
          v-else-if="view.id === 'spot-risk'"
          :portfolio="memberHandle ? 'member-scenario' : portfolio"
          :basket="selectedItems"
        />
        <PaperReview
          v-else-if="view.id === 'paper'"
          ref="paperReview"
          :portfolio="portfolio"
        />
        <p v-else-if="status" role="status" class="notice">{{ status }}</p>
        <template v-else-if="data && snap && run && row">
          <p class="asof">
            <Icon name="Clock" />Archived synthetic snapshot <span>·</span
            ><time>{{ utc(snap.valid_at_ms) }}</time>
          </p>
          <section
            v-if="view.id === 'overview'"
            class="metrics"
            aria-label="Opening portfolio and risk"
          >
            <div>
              <span>Opening NAV</span
              ><strong
                >{{ snap.nav }} <small>{{ snap.quote }}</small></strong
              >
              <p>
                {{ snap.quantity }} {{ snap.base }} + {{ snap.cash }}
                {{ snap.quote }} cash
              </p>
            </div>
            <div>
              <span>Spot mark</span
              ><strong
                >{{ snap.mark }} <small>{{ snap.quote }}</small></strong
              >
              <p>{{ snap.base }} / {{ snap.quote }}</p>
            </div>
            <div>
              <span>One-day VaR</span
              ><strong
                >{{ data.risk.value_at_risk ?? "Unavailable" }}
                <small v-if="data.risk.value_at_risk">{{
                  snap.quote
                }}</small></strong
              >
              <p>{{ data.risk.confidence_bps / 100 }}% confidence</p>
            </div>
            <div>
              <span>Expected shortfall</span
              ><strong
                >{{ data.risk.expected_shortfall ?? "Unavailable" }}
                <small v-if="data.risk.expected_shortfall">{{
                  snap.quote
                }}</small></strong
              >
              <p>{{ data.risk.sample_size ?? 0 }} synthetic scenarios</p>
            </div>
          </section>
          <section class="chart panel" v-if="view.id !== 'ledger'">
            <h2>Portfolio value</h2>
            <p class="caption">Minute-close NAV · {{ snap.quote }}</p>
            <EquityChart :runs="data.backtest.runs" :active="active" />
            <div class="times">
              <span>{{ clock(data.backtest.warmup_last_ms) }}</span
              ><span>{{ clock(data.backtest.holdout_last_ms) }} UTC</span>
            </div>

            <div class="legend" aria-label="Chart strategies">
              <button
                v-for="s in data.backtest.runs"
                :key="s.rule"
                :aria-pressed="active === s.rule"
                :style="{ '--series': 'var(' + colors[s.rule] + ')' }"
                @click="select(s.rule)"
              >
                <i aria-hidden="true"></i>{{ names[s.rule] }}
              </button>
            </div>
          </section>
          <section v-if="view.id === 'strategies'" class="summary panel">
            <h2>Strategy results</h2>
            <p class="caption">Net P&amp;L after costs · {{ snap.quote }}</p>
            <button
              v-for="s in data.backtest.runs"
              :key="s.rule"
              class="strategy"
              :style="{ '--series': `var(${colors[s.rule]})` }"
              :aria-pressed="active === s.rule"
              @click="select(s.rule)"
            >
              <span
                ><i aria-hidden="true"></i>{{ names[s.rule]
                }}<small v-if="s.rule === data.backtest.benchmark"
                  >Benchmark</small
                ></span
              >
              <b>{{ s.net_pnl }}</b>
            </button>
            <div class="drawdown">
              <span>Max drawdown · {{ names[active] }}</span
              ><strong :title="run.max_drawdown"
                >{{ (numeric(run.max_drawdown) * 100).toFixed(2) }}%</strong
              >
            </div>
            <a class="next" href="#ledger"
              >View settlements <Icon name="ChevronRightMd"
            /></a>
          </section>
          <section v-if="view.id === 'ledger'" class="ledger panel">
            <div class="picker">
              <label for="ledger-strategy">Strategy</label
              ><select
                id="ledger-strategy"
                :value="active"
                @change="
                  select(($event.target as HTMLSelectElement).value as Rule)
                "
              >
                <option
                  v-for="s in data.backtest.runs"
                  :key="s.rule"
                  :value="s.rule"
                >
                  {{ names[s.rule] }}
                </option>
              </select>
            </div>
            <p class="caption">
              {{ names[active] }} · {{ run.ledger.length }} settlements · times
              in UTC
            </p>
            <div class="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Close</th>
                    <th>Action</th>
                    <th>Mark</th>
                    <th>Cash</th>
                    <th>{{ snap.base }}</th>
                    <th>NAV</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="(r, i) in run.ledger" :key="r.valuation_close_ms">
                    <td>
                      <button
                        aria-haspopup="dialog"
                        :aria-label="
                          'Inspect settlement ' + clock(r.valuation_close_ms)
                        "
                        @click="inspect('posting', i)"
                      >
                        {{ clock(r.valuation_close_ms) }}
                      </button>
                    </td>
                    <td>
                      {{
                        r.posting.state === "filled"
                          ? r.posting.fill.side
                          : r.posting.state
                      }}
                    </td>
                    <td>{{ r.valuation_mark }}</td>
                    <td>{{ r.cash }}</td>
                    <td>{{ r.quantity }}</td>
                    <td>{{ r.nav }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>
          <section
            v-if="view.id === 'overview'"
            class="review-links"
            aria-label="Explore this report"
          >
            <a href="#strategies"
              ><Icon name="Compare" />Strategy comparison
              <Icon name="ChevronRightMd"
            /></a>
            <a href="#ledger"
              ><Icon name="Notebook" />Settlement ledger
              <Icon name="ChevronRightMd"
            /></a>
          </section>
          <dialog
            ref="inspector"
            class="inspector"
            aria-labelledby="inspector-title"
            @click.self="inspector?.close()"
          >
            <div class="sheet">
              <header>
                <h2 id="inspector-title">
                  {{
                    inspection === "posting"
                      ? "Posting detail"
                      : "Report provenance"
                  }}
                </h2>
                <button
                  class="tool"
                  aria-label="Close details"
                  autofocus
                  @click="inspector?.close()"
                >
                  <Icon name="XCrossed" />
                </button>
              </header>
              <div class="detail" v-if="inspection === 'posting'">
                <div>
                  <p>
                    {{ clock(row.valuation_close_ms) }} UTC ·
                    {{ names[active] }}
                  </p>
                </div>
                <dl>
                  <dt>Decision / signal mark</dt>
                  <dd>
                    {{ clock(row.decision_ms) }} · {{ row.signal_mark }}
                    {{ snap.quote }}
                  </dd>
                  <dt>Settlement known at</dt>
                  <dd>{{ utc(row.valuation_known_ms) }}</dd>
                  <template v-if="row.posting.state === 'filled'"
                    ><dt>Fill</dt>
                    <dd>
                      {{ row.posting.fill.side }}
                      {{ row.posting.fill.quantity }} {{ snap.base }} @
                      {{ row.posting.fill.price }} {{ snap.quote }}
                    </dd>
                    <dt>Fee / cash posting</dt>
                    <dd>
                      {{ row.posting.fill.fee }} /
                      {{ row.posting.fill.cash_delta }}
                      {{ snap.quote }}
                    </dd>
                    <dt>Spread / slippage / tick rounding</dt>
                    <dd>
                      {{ row.posting.fill.half_spread_cost }} /
                      {{ row.posting.fill.slippage_cost }} /
                      {{ row.posting.fill.tick_rounding_cost }} {{ snap.quote }}
                    </dd></template
                  >
                  <template v-else
                    ><dt>Decision</dt>
                    <dd>
                      {{
                        row.posting.state === "rejected"
                          ? row.posting.reason
                          : "Hold position"
                      }}
                    </dd></template
                  >
                  <dt>Realized / unrealized P&amp;L</dt>
                  <dd>
                    {{ row.realized_pnl }} / {{ row.unrealized_pnl }}
                    {{ snap.quote }}
                  </dd>
                </dl>
              </div>

              <dl class="provenance" v-else>
                <dt>Known at</dt>
                <dd>{{ utc(snap.known_at_ms) }}</dd>
                <dt>Opening observation</dt>
                <dd>{{ snap.observation_id }}</dd>
                <dt>Opening dataset</dt>
                <dd>{{ snap.dataset_sha256 }}</dd>
                <dt>Risk history</dt>
                <dd>{{ data.risk.history_sha256 }}</dd>
                <dt>Backtest source</dt>
                <dd>{{ data.backtest.source_sha256 }}</dd>
                <dt>Ledger</dt>
                <dd>{{ data.backtest.ledger_sha256 }}</dd>
                <dt>Report</dt>
                <dd>
                  <a :href="endpoint + revision" download="oracle-report.json"
                    ><Icon name="Download" /> {{ revision }}</a
                  >
                </dd>
              </dl>
            </div>
          </dialog></template
        >
      </main>
      <nav v-if="narrow" class="app-dock" aria-label="App navigation">
        <a
          v-for="tab in [
            { id: 'discover', label: 'Discover', icon: 'Chart' },
            { id: 'baskets', label: 'Baskets', icon: 'Folder' },
            { id: 'profile', label: 'Wallet', icon: 'Notebook' },
          ] as const"
          :key="tab.id"
          :href="'#' + tab.id"
          :aria-current="view.id === tab.id ? 'page' : undefined"
          ><Icon :name="tab.icon" />{{ tab.label }}</a
        >
      </nav>
    </div>
  </div>
</template>
