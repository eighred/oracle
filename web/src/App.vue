<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import EquityChart from "./EquityChart.vue";
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
async function refresh() {
  if (pending.value) return;
  pending.value = true;
  data.value = undefined;
  revision.value = "";
  status.value = "Loading portfolio…";
  try {
    const response = await fetch(endpoint + "latest", {
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
onMounted(refresh);
</script>
<template>
  <a class="skip" href="#overview">Skip to portfolio</a>
  <header>
    <div class="bar">
      <a class="brand" href="#overview"
        ><span class="brand-mark" aria-hidden="true"></span>Oracle</a
      >
      <nav aria-label="Workstation">
        <a href="#overview">Portfolio</a><a href="#comparison">Strategies</a
        ><a href="#ledger">Ledger</a>
      </nav>
      <span class="owner">Eighred Research</span>
    </div>
  </header>
  <main id="overview">
    <div class="heading">
      <div>
        <div class="context">{{ portfolio }}</div>
        <h1>Portfolio review</h1>
      </div>
      <button class="refresh" :disabled="pending" @click="refresh">
        Refresh report
      </button>
    </div>
    <p v-if="status" role="status" class="notice">{{ status }}</p>
    <template v-if="data && snap && run && row">
      <p class="asof">
        <span class="dot"></span> Archived synthetic snapshot <span>·</span>
        <time>{{ utc(snap.valid_at_ms) }}</time>
      </p>
      <section class="metrics" aria-label="Opening portfolio and risk">
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
      <section id="comparison" class="comparison">
        <div class="chart panel">
          <h2>Portfolio value</h2>
          <p class="caption">Minute-close NAV · {{ snap.quote }}</p>
          <EquityChart :runs="data.backtest.runs" :active="active" />
          <div class="times">
            <span>{{ clock(data.backtest.warmup_last_ms) }}</span
            ><span>{{ clock(data.backtest.holdout_last_ms) }} UTC</span>
          </div>
        </div>
        <div class="summary panel">
          <h2>Strategy comparison</h2>
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
        </div>
      </section>
      <section id="ledger" class="ledger panel">
        <h2>Settlement ledger</h2>
        <p class="caption">
          {{ names[active] }} · {{ run.ledger.length }} settlements · times in
          UTC
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
              <tr
                v-for="(r, i) in run.ledger"
                :key="r.valuation_close_ms"
                :class="{ selected: index === i }"
              >
                <td>
                  <button
                    :aria-pressed="index === i"
                    :aria-label="
                      'Inspect settlement ' + clock(r.valuation_close_ms)
                    "
                    @click="index = i"
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
        <div class="detail" aria-live="polite">
          <div>
            <h3>Posting detail</h3>
            <p>
              {{ clock(row.valuation_close_ms) }} UTC · {{ names[active] }}
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
                {{ row.posting.fill.side }} {{ row.posting.fill.quantity }}
                {{ snap.base }} @ {{ row.posting.fill.price }} {{ snap.quote }}
              </dd>
              <dt>Fee / cash posting</dt>
              <dd>
                {{ row.posting.fill.fee }} / {{ row.posting.fill.cash_delta }}
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
              {{ row.realized_pnl }} / {{ row.unrealized_pnl }} {{ snap.quote }}
            </dd>
          </dl>
        </div>
      </section>
      <details class="provenance panel">
        <summary>Report provenance</summary>
        <dl>
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
            <a :href="endpoint + revision" download="oracle-report.json">{{
              revision
            }}</a>
          </dd>
        </dl>
      </details>
    </template>
  </main>
</template>
