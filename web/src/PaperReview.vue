<script setup lang="ts">
import { apiFetch } from "./api";
import { computed, onMounted, ref } from "vue";
import { clock, utc, names, numeric, type Rule, type Run } from "./report";
import Icon from "./Icon.vue";
const props = defineProps<{ portfolio: string }>();
interface Paper {
  manifest: {
    version: number;
    portfolio: string;
    source_kind: string;
    first_ms: number;
    last_ms: number;
    quote: string;
    base: string;
  };
  manifest_sha256: string;
  frozen_ms: number;
  state: "complete" | "missed" | "canceled";
  samples: { mark: string; close_ms: number; known_ms: number }[];
  source_receipts?: {
    open_ms: number;
    received_ms: number;
    response_json_sha256: string;
  }[];
  opening_valuation?: { nav: string; quote: string; valid_ms: number };
  minute_risk?: {
    confidence_bps: number;
    sample_size: number;
    by_rule: Record<
      Rule,
      { value_at_risk: string; expected_shortfall: string }
    >;
  };
  unobserved_slots: number[];
  stop_reason?: string;
  projection: { runs: Run[] } | null;
}
const loading = ref(false),
  status = ref(""),
  paper = ref<Paper>(),
  revision = ref("");
const active = ref<Rule>("momentum");
const sourceNames: Record<string, string> = {
  prospective_synthetic: "Prospective synthetic experiment",
  ccxt_loopback_fixture: "CCXT Rust · synthetic venue",
  ccxt_binance_spot: "CCXT Rust · Binance spot",
};
const risk = computed(() => paper.value?.minute_risk?.by_rule[active.value]);
const run = computed(() =>
  paper.value?.projection?.runs.find((item) => item.rule === active.value),
);
const endpoint = "/api/portfolios/" + props.portfolio + "/paper/";
async function refresh() {
  if (loading.value) return;
  loading.value = true;
  paper.value = undefined;
  revision.value = "";
  status.value = "Loading paper experiment…";
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
            404: "No published paper experiment.",
          } as Record<number, string>
        )[response.status] ?? "Paper report service unavailable.";
      return;
    }
    const p: Paper = await response.json(),
      id = response.headers.get("x-oracle-report-id");
    if (
      !id ||
      !/^[a-f0-9]{64}$/.test(id) ||
      p.manifest.version !== 1 ||
      p.manifest.portfolio !== props.portfolio ||
      !Object.hasOwn(sourceNames, p.manifest.source_kind) ||
      !["complete", "missed", "canceled"].includes(p.state) ||
      (p.state === "complete" &&
        !p.projection?.runs.some(
          (r) => r.rule === "momentum" && r.ledger.length,
        ))
    ) {
      status.value = "Unsupported or incomplete paper report.";
      return;
    }
    paper.value = p;
    revision.value = id;
    status.value = "";
  } catch {
    status.value = "Paper report service unavailable.";
  } finally {
    loading.value = false;
  }
}
onMounted(refresh);
defineExpose({ refresh, loading });
</script>
<template>
  <p v-if="status" role="status" class="notice">{{ status }}</p>
  <template v-if="paper">
    <p class="asof">
      <Icon name="Clock" />{{ sourceNames[paper.manifest.source_kind] }} ·
      {{ paper.state }}
    </p>
    <section class="panel">
      <h2>Observation window</h2>
      <dl>
        <dt>Frozen at</dt>
        <dd>{{ utc(paper.frozen_ms) }}</dd>
        <dt>Scheduled window</dt>
        <dd>
          {{ utc(paper.manifest.first_ms) }} – {{ utc(paper.manifest.last_ms) }}
        </dd>
        <dt>Coverage</dt>
        <dd>
          {{ paper.samples.length }} observed ·
          {{ paper.unobserved_slots.length }} unobserved
        </dd>
        <template v-if="paper.stop_reason"
          ><dt>Stopped because</dt>
          <dd>{{ paper.stop_reason.replaceAll("_", " ") }}</dd></template
        >
        <template v-if="paper.opening_valuation"
          ><dt>Opening NAV</dt>
          <dd>
            {{ paper.opening_valuation.nav }} {{ paper.manifest.quote }} ·
            {{ utc(paper.opening_valuation.valid_ms) }}
          </dd></template
        >
      </dl>
      <a class="next" :href="endpoint + revision" download="oracle-paper.json"
        ><Icon name="Download" />Download experiment and ledger</a
      >
    </section>
    <section v-if="paper.projection?.runs.length && run" class="panel">
      <h2>Net paper outcomes</h2>
      <p class="caption">After modeled costs · {{ paper.manifest.quote }}</p>
      <button
        class="strategy"
        v-for="item in paper.projection.runs"
        :key="item.rule"
        :aria-pressed="active === item.rule"
        @click="active = item.rule"
      >
        <span
          >{{ names[item.rule]
          }}<small v-if="item.rule === 'hold'">Benchmark</small></span
        ><b>{{ item.net_pnl }}</b>
      </button>
      <p class="caption">
        {{ names[active] }} · total fees {{ run.total_fees }}
        {{ paper.manifest.quote }} · maximum drawdown
        {{ (numeric(run.max_drawdown) * 100).toFixed(2) }}%
      </p>
      <div class="table-scroll">
        <table>
          <thead>
            <tr>
              <th>Settlement UTC</th>
              <th>Decision UTC</th>
              <th>Action</th>
              <th>Reference</th>
              <th>Fill price</th>
              <th>Fee</th>
              <th>Cash</th>
              <th>{{ paper.manifest.base }}</th>
              <th>NAV</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in run.ledger" :key="row.valuation_close_ms">
              <td>{{ clock(row.valuation_close_ms) }}</td>
              <td>{{ clock(row.decision_ms) }}</td>
              <td>
                {{
                  row.posting.state === "filled"
                    ? row.posting.fill.side
                    : row.posting.state
                }}
              </td>
              <td>{{ row.valuation_mark }}</td>
              <td>
                {{
                  row.posting.state === "filled" ? row.posting.fill.price : "—"
                }}
              </td>
              <td>
                {{
                  row.posting.state === "filled" ? row.posting.fill.fee : "0"
                }}
              </td>
              <td>{{ row.cash }}</td>
              <td>{{ row.quantity }}</td>
              <td>{{ row.nav }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <p v-if="risk && paper.minute_risk" class="caption">
        One-minute historical risk ·
        {{ paper.minute_risk.confidence_bps / 100 }}% confidence ·
        {{ paper.minute_risk.sample_size }} returns
      </p>
      <dl v-if="risk">
        <dt>Value at risk</dt>
        <dd :title="risk.value_at_risk">
          {{ numeric(risk.value_at_risk).toFixed(6) }}
          {{ paper.manifest.quote }}
        </dd>
        <dt>Expected shortfall</dt>
        <dd :title="risk.expected_shortfall">
          {{ numeric(risk.expected_shortfall).toFixed(6) }}
          {{ paper.manifest.quote }}
        </dd>
      </dl>
    </section>
    <details class="panel" v-if="paper.source_receipts?.length">
      <summary>Market data receipts</summary>
      <dl v-for="(receipt, i) in paper.source_receipts" :key="receipt.open_ms">
        <dt>
          {{ utc(receipt.received_ms) }} · {{ paper.samples[i]?.mark }}
          {{ paper.manifest.quote }}
        </dt>
        <dd>{{ receipt.response_json_sha256 }}</dd>
      </dl>
    </details>
  </template>
</template>
