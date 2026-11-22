<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { apiFetch } from "./api";
import { utc } from "./report";
import { amount } from "./risk_amount";

const props = defineProps<{ currency: string }>();
type Report = {
  schema_version: number;
  state: "cash_only" | "valued";
  currency: string;
  cash: string;
  basket_cash: string;
  nav: string;
  captured_ms: number;
  first_open_ms?: number;
  market_close_ms?: number;
  oldest_mark_ms?: number;
  advisory?: { state: string; text?: string };
  partial_market_coverage?: boolean;
  risk?: {
    value_at_risk: string;
    expected_shortfall: string;
    stress_pnl: string;
    sample_size: number;
  };
  positions: {
    pair: string;
    quantity: string;
    exposure: string;
    tail_contribution: string;
  }[];
};
const report = ref<Report>(),
  status = ref(""),
  busy = ref(false);
const closes = ref(60),
  confidence = ref(9500),
  shock = ref(-1000);
const controller = new AbortController();
const commentary = ref(false);
onBeforeUnmount(() => controller.abort());
watch([closes, confidence, shock, commentary], () => {
  report.value = undefined;
  status.value = "";
});
async function calculate() {
  if (busy.value) return;
  busy.value = true;
  report.value = undefined;
  status.value = "";
  try {
    const res = await apiFetch("/api/v1/me/wallet-risk", {
      method: "POST",
      cache: "no-store",
      signal: controller.signal,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        currency: props.currency,
        closes: closes.value,
        confidence_bps: confidence.value,
        shock_bps: shock.value,
        commentary: commentary.value,
      }),
    });
    if (!res.ok) {
      const detail =
        res.status === 422 ? await res.json().catch(() => null) : null;
      throw Error(
        detail?.reason ??
          (res.status === 429
            ? "The risk service is busy. Try again shortly."
            : "Wallet risk is unavailable."),
      );
    }
    const data: Report = await res.json();
    if (data.schema_version !== 1 || data.currency !== props.currency)
      throw Error("Unexpected wallet risk response.");
    report.value = data;
  } catch (e) {
    if (!controller.signal.aborted)
      status.value =
        e instanceof Error ? e.message : "Wallet risk is unavailable.";
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <details class="wallet-risk">
    <summary>Wallet risk · {{ currency }}</summary>
    <form @submit.prevent="calculate">
      <fieldset :disabled="busy">
        <label
          >Minute closes<input
            v-model.number="closes"
            type="number"
            min="5"
            max="100"
            required
        /></label>
        <label
          >Confidence (bps)<input
            v-model.number="confidence"
            type="number"
            min="1"
            max="9999"
            required
        /></label>
        <label
          >Price shock (bps)<input
            v-model.number="shock"
            type="number"
            min="-10000"
            max="100000"
            required
        /></label>
        <label class="commentary"
          ><input type="checkbox" v-model="commentary" /> Include AI risk
          commentary</label
        >
        <button class="app-button">
          {{ busy ? "Calculating…" : "Calculate wallet risk" }}
        </button>
      </fieldset>
    </form>
    <p v-if="status" role="status">{{ status }}</p>
    <section v-if="report" aria-label="Wallet risk result">
      <p class="caption">Wallet snapshot · {{ utc(report.captured_ms) }}</p>
      <p>
        Paper value {{ report.nav }} {{ currency }} · Cash {{ report.cash }}
        {{ currency }} (including {{ report.basket_cash }} in baskets)
      </p>
      <p v-if="report.state === 'cash_only'">
        This balance is entirely cash in {{ currency }}. It has no spot price
        exposure in that currency.
      </p>
      <template v-if="report.risk">
        <p v-if="report.advisory">
          {{
            report.advisory.state === "available"
              ? report.advisory.text
              : "AI risk commentary is unavailable."
          }}
        </p>
        <p
          class="caption"
          v-if="
            report.first_open_ms &&
            report.market_close_ms &&
            report.oldest_mark_ms
          "
        >
          Oldest holding mark {{ utc(report.oldest_mark_ms) }} ·
          {{ report.risk.sample_size }} one-minute returns from
          {{ utc(report.first_open_ms) }} to {{ utc(report.market_close_ms) }}.
          Short samples give limited evidence about rare losses.
        </p>
        <p v-if="report.partial_market_coverage" role="status">
          Partial market coverage. The spot price history uses the available
          complete series.
        </p>
        <dl>
          <template
            v-for="[label, value] in [
              ['One-minute VaR', report.risk.value_at_risk],
              ['Expected shortfall', report.risk.expected_shortfall],
              ['Scenario P&L', report.risk.stress_pnl],
            ] as const"
            :key="label"
          >
            <dt>{{ label }}</dt>
            <dd :title="value">{{ amount(value) }} {{ currency }}</dd>
          </template>
        </dl>
        <details v-for="row in report.positions" :key="row.pair">
          <summary>{{ row.pair }} · {{ amount(row.quantity) }}</summary>
          <p>
            Exposure {{ amount(row.exposure) }} {{ currency }} · Expected
            shortfall contribution {{ amount(row.tail_contribution) }}
            {{ currency }}
          </p>
        </details>
      </template>
    </section>
  </details>
</template>
<style scoped>
.wallet-risk {
  margin: 12px 0;
  overflow-wrap: anywhere;
}
summary {
  cursor: pointer;
}
fieldset {
  border: 0;
  padding: 12px 0;
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 10px;
}
label {
  display: grid;
  gap: 4px;
}
input {
  width: 8em;
}
.commentary {
  display: flex;
  align-items: center;
}
.commentary input {
  width: auto;
}
dl {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 4px 16px;
}
dd {
  margin: 0;
}
</style>
