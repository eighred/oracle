<script setup lang="ts">
import { ref, watch } from "vue";
import { utc } from "./report";
const props = defineProps<{
  portfolio: string;
  basket?: { venue: string; pair: string; bps: number }[];
}>();
const positions = ref(
  props.basket?.map((p) => ({
    venue: p.venue,
    pair: p.pair,
    quantity: (p.bps / 100).toFixed(2),
    shock_bps: -1000,
  })) ?? [
    { venue: "binance", pair: "BTC/USDT", quantity: "1", shock_bps: -1000 },
  ],
);
const cash = ref("0"),
  closes = ref(60),
  confidence = ref(9500);
const commentary = ref(false);
const pending = ref(false),
  message = ref("");
type Result = {
  schema_version: number;
  source: string;
  quote: string;
  first_open_ms: number;
  as_of_ms: number;
  known_at_ms: number;
  risk: {
    model: string;
    nav: string;
    value_at_risk: string;
    expected_shortfall: string;
    stress_pnl: string;
    sample_size: number;
    confidence_bps: number;
  };
  advisory?: { state: string; model?: string; text?: string };
  positions: {
    venue: string;
    pair: string;
    exposure: string;
    mark: string;
    stress_pnl: string;
    tick: string;
    quantity_step: string;
    candles_sha256: string;
  }[];
};
const result = ref<Result>();
watch(
  [positions, cash, closes, confidence, commentary],
  () => {
    result.value = undefined;
    message.value = "";
  },
  { deep: true },
);
function amount(value: string) {
  const [n, d = "1"] = value.split("/");
  if (!n) return "Unavailable";
  // Round for display after dividing exact integers. Keep the exact value in the title.
  const num = BigInt(n),
    den = BigInt(d),
    abs = num < 0n ? -num : num;
  const scale = 100000000n,
    digits = ((abs * scale + den / 2n) / den).toString().padStart(9, "0");
  if (abs > 0n && abs * scale < den) return value;
  const decimal = digits.slice(-8).replace(/0+$/, "");
  return (
    (num < 0n ? "−" : "") + digits.slice(0, -8) + (decimal ? "." + decimal : "")
  );
}
async function run() {
  pending.value = true;
  result.value = undefined;
  message.value = "";
  try {
    const res = await fetch(
      "/api/portfolios/" + encodeURIComponent(props.portfolio) + "/spot-risk",
      {
        method: "POST",
        credentials: "same-origin",
        cache: "no-store",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          size_unit: props.basket ? "quote" : "base",
          commentary: commentary.value,
          cash: cash.value,
          confidence_bps: confidence.value,
          closes: closes.value,
          positions: positions.value,
        }),
      },
    );
    if (!res.ok) {
      if (res.status === 422) {
        const problem = await res.json();
        message.value = problem.reason ?? "Invalid scenario.";
      } else
        message.value =
          res.status === 403
            ? "Access to this portfolio is denied."
            : res.status === 401
              ? "Sign in with your operator credentials."
              : res.status === 429
                ? "The risk service is busy. Try again shortly."
                : "Risk service unavailable.";
      return;
    }
    const data: Result = await res.json();
    if (
      data.schema_version !== 1 ||
      data.source !== "public_spot_research" ||
      data.risk.model !== "joint_minute_spot_v1"
    )
      throw Error();
    result.value = data;
  } catch {
    message.value = "Risk service unavailable.";
  } finally {
    pending.value = false;
  }
}
</script>
<template>
  <p class="caption">
    Reprice hypothetical spot holdings in one quote currency using synchronized
    one-minute closes.
  </p>
  <form class="panel risk-form" @submit.prevent="run">
    <fieldset :disabled="pending">
      <legend>Scenario positions</legend>
      <div v-for="(p, i) in positions" :key="i" class="risk-fields">
        <label
          >Venue<select v-model="p.venue">
            <option value="binance">Binance</option>
            <option value="okx">OKX</option>
          </select></label
        >
        <label
          >Spot pair<input
            v-model.trim="p.pair"
            required
            placeholder="BASE/QUOTE"
        /></label>
        <label
          >{{ props.basket ? "Quote allocation" : "Quantity"
          }}<input v-model.trim="p.quantity" required inputmode="decimal"
        /></label>
        <label
          >Shock (bps)<input
            v-model.number="p.shock_bps"
            type="number"
            required
            min="-10000"
            max="100000"
        /></label>
        <button
          type="button"
          :disabled="positions.length === 1"
          :aria-label="'Remove position ' + (i + 1)"
          @click="positions.splice(i, 1)"
        >
          Remove
        </button>
      </div>
      <button
        type="button"
        :disabled="positions.length >= 8"
        @click="
          positions.push({
            venue: 'okx',
            pair: '',
            quantity: '1',
            shock_bps: -1000,
          })
        "
      >
        Add position
      </button>
      <div class="risk-fields">
        <label
          >Quote cash<input v-model.trim="cash" required inputmode="decimal"
        /></label>
        <label
          >Minute closes<input
            v-model.number="closes"
            type="number"
            required
            min="5"
            max="100"
        /></label>
        <label
          >Confidence (bps)<input
            v-model.number="confidence"
            type="number"
            required
            min="1"
            max="9999"
        /></label>
      </div>
      <label
        ><input type="checkbox" v-model="commentary" />Include AI risk
        commentary</label
      >
      <button type="submit">
        {{ pending ? "Reading market data…" : "Calculate risk" }}
      </button>
    </fieldset>
  </form>
  <p v-if="message" role="status">{{ message }}</p>
  <section v-if="result" class="panel risk-form">
    <h2>Portfolio risk · {{ result.quote }}</h2>
    <p class="caption">
      {{ utc(result.first_open_ms) }} – {{ utc(result.as_of_ms) }} · Retrieved
      {{ utc(result.known_at_ms) }}
    </p>
    <p>
      {{ result.risk.sample_size }} historical one-minute changes ·
      {{ result.risk.confidence_bps / 100 }}% confidence. A short sample limits
      tail estimates.
    </p>
    <dl>
      <template
        v-for="[name, value] in [
          ['Portfolio value', result.risk.nav],
          ['One-minute VaR', result.risk.value_at_risk],
          ['Expected shortfall', result.risk.expected_shortfall],
          ['Scenario P&L', result.risk.stress_pnl],
        ] as const"
        :key="name"
        ><dt>{{ name }}</dt>
        <dd :title="value">{{ amount(value) }} {{ result.quote }}</dd></template
      >
    </dl>
    <section v-if="result.advisory">
      <h3>AI risk commentary</h3>
      <p>
        {{
          result.advisory.state === "available"
            ? result.advisory.text
            : result.advisory.state === "unconfigured"
              ? "LiteLLM is not configured."
              : "Risk commentary is unavailable."
        }}
      </p>
      <small v-if="result.advisory.model">{{ result.advisory.model }}</small>
    </section>
    <details v-for="p in result.positions" :key="p.venue + p.pair">
      <summary>{{ p.venue }} · {{ p.pair }}</summary>
      <p>
        Mark {{ p.mark }} · Exposure
        <span :title="p.exposure">{{ amount(p.exposure) }}</span> · Scenario
        P&amp;L <span :title="p.stress_pnl">{{ amount(p.stress_pnl) }}</span>
        {{ result.quote }}
      </p>
      <p>Price tick {{ p.tick }} · Lot step {{ p.quantity_step }}</p>
      <p class="caption">Candle receipt {{ p.candles_sha256 }}</p>
    </details>
  </section>
</template>
<style scoped>
.risk-form {
  padding: 1.25rem;
  margin: 1rem 0;
  overflow-wrap: anywhere;
}
fieldset {
  border: 0;
  padding: 0;
  min-width: 0;
}
legend,
h2 {
  font-weight: 600;
}
.risk-fields {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
  align-items: end;
  margin: 1rem 0;
}
label {
  display: grid;
  gap: 0.25rem;
  flex: 1 1 8rem;
  min-width: 0;
}
input,
select,
button {
  font: inherit;
  background: var(--base);
  color: inherit;
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 0.45rem 0.65rem;
  min-width: 0;
}
button,
summary {
  cursor: pointer;
}
button:disabled {
  opacity: 0.5;
}
details {
  padding: 0.6rem 0;
  border-top: 1px solid var(--line);
}
dl {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 0.4rem 1rem;
}
dd {
  margin: 0;
}
</style>
