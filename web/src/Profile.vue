<script setup lang="ts">
import { apiFetch } from "./api";
import { onMounted, onBeforeUnmount, computed, ref } from "vue";
import Icon from "./Icon.vue";

type Balance = {
  currency: string;
  available: string;
  deposited: string;
  basketValue: string;
  total: string;
  net: string;
  asOf: number | null;
};
type Coin = {
  asset: string;
  venue: string;
  currency: string;
  quantity: string;
  value: string;
  asOf: number;
};
type Entry = {
  id: string;
  kind: string;
  at: string;
  currency: string;
  amount: string;
  name: string | null;
  receipt: null | {
    fees: string;
    fills: {
      venue: string;
      pair: string;
      side: string;
      quantity: string;
      price: string;
      fee: string;
    }[];
  };
};
type Position = {
  id: string;
  name: string;
  status: string;
  basket: string;
  currency: string;
  contribution: string;
  enteredAt: string;
  asOfMs: number | null;
  netReturnPct: string;
  revision: number;
};
type Profile = {
  principal: string;
  balances: Balance[];
  coins: Coin[];
  positions: Position[];
  activity: Entry[];
};

const data = ref<Profile>(),
  activity = ref<Entry[]>([]),
  next = ref<string>(),
  busy = ref(false),
  error = ref("");
const currency = ref(""),
  section = ref<"Coins" | "Baskets" | "History">("Coins");
const depositing = ref(false),
  unit = ref("TRY"),
  amount = ref("");
let depositKey: { intent: string; key: string } | undefined;
const balance = computed(() =>
  data.value?.balances.find((b) => b.currency === currency.value),
);
const coins = computed(
  () => data.value?.coins.filter((c) => c.currency === currency.value) ?? [],
);
const positions = computed(
  () =>
    data.value?.positions.filter((a) => a.currency === currency.value) ?? [],
);
const abort = new AbortController();

async function read(before?: string) {
  const res = await apiFetch(
    "/api/profile" + (before ? "?before=" + encodeURIComponent(before) : ""),
    { credentials: "same-origin", cache: "no-store", signal: abort.signal },
  );
  if (!res.ok)
    throw Error(
      res.status === 401
        ? "Sign in to view your wallet."
        : "Wallet unavailable.",
    );
  const p: Profile = await res.json();
  data.value = p;
  if (!p.balances.some((b) => b.currency === currency.value))
    currency.value = p.balances[0]?.currency ?? "";
  const page = p.activity.slice(0, 25);
  activity.value = before ? [...activity.value, ...page] : page;
  next.value = p.activity.length > 25 ? page[24]?.id : undefined;
}
async function perform(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = e instanceof Error ? e.message : "Wallet unavailable.";
    data.value = undefined;
    activity.value = [];
    next.value = undefined;
  } finally {
    busy.value = false;
  }
}
async function deposit() {
  const selected = unit.value.trim().toUpperCase();
  const intent = JSON.stringify([selected, amount.value]);
  if (depositKey?.intent !== intent)
    depositKey = { intent, key: crypto.randomUUID() };
  const request = depositKey.key;
  await perform(async () => {
    const res = await apiFetch("/api/baskets", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        op: "deposit",
        request,
        currency: selected,
        amount: amount.value,
      }),
      signal: abort.signal,
    });
    if (!res.ok)
      throw Error(
        "Paper deposit could not be recorded. Check the currency and amount.",
      );
    currency.value = selected;
    await read();
    depositing.value = false;
    amount.value = "";
    depositKey = undefined;
  });
}
// Use integer arithmetic so presentation never rounds a large ledger amount through a float.
function displayAmount(value: string, currency: string) {
  const places = ["TRY", "USD", "EUR", "USDT", "USDC"].includes(currency)
    ? 2
    : 8;
  const [whole = "0", fraction = ""] = value.replace(/^-/, "").split(".");
  const scale = 10n ** BigInt(places);
  const rounding = (fraction[places] ?? "0") >= "5" ? 1n : 0n;
  const atoms =
    BigInt(whole) * scale +
    BigInt(fraction.slice(0, places).padEnd(places, "0")) +
    rounding;
  const decimals = (atoms % scale).toString().padStart(places, "0");
  const sign = value.startsWith("-") && atoms !== 0n ? "-" : "";
  return sign + (atoms / scale).toLocaleString("en-US") + "." + decimals;
}
const date = (v: string | number) => new Date(v).toLocaleString();
onMounted(() => void perform(() => read()));
const timer = window.setInterval(() => {
  if (
    document.visibilityState === "visible" &&
    !depositing.value &&
    section.value !== "History" &&
    data.value
  )
    void perform(() => read());
}, 60000);
onBeforeUnmount(() => {
  clearInterval(timer);
  abort.abort();
});
</script>
<template>
  <p v-if="error" role="status">{{ error }}</p>
  <button v-if="!data" :disabled="busy" @click="perform(() => read())">
    {{ busy ? "Loading wallet…" : "Reload wallet" }}
  </button>
  <template v-if="data">
    <header class="profile-head">
      <div class="avatar" aria-hidden="true">
        {{ data.principal.slice(0, 1).toUpperCase() }}
      </div>
      <div>
        <strong>{{ data.principal }}</strong>
        <p class="caption">Personal paper account</p>
      </div>
      <button
        class="tool"
        aria-label="Refresh wallet"
        :disabled="busy"
        @click="perform(() => read())"
      >
        <Icon name="Regenerate" />
      </button>
    </header>
    <section class="panel wallet">
      <div class="wallet-top">
        <h2>Balance overview</h2>
        <label v-if="data.balances.length"
          >Currency
          <select v-model="currency" aria-label="Currency">
            <option v-for="b in data.balances" :key="b.currency">
              {{ b.currency }}
            </option>
          </select></label
        >
      </div>
      <div v-if="balance">
        <p class="caption">Total paper balance</p>
        <p class="total" :title="balance.total + ' ' + currency">
          {{ displayAmount(balance.total, currency) }}
          <small>{{ currency }}</small>
        </p>
        <p class="caption" v-if="balance.asOf">
          {{
            Date.now() - balance.asOf > 600000
              ? "Stale basket marks"
              : "Oldest basket mark"
          }}
          · {{ date(balance.asOf) }}
        </p>
        <dl class="values">
          <div>
            <dt>Deposited</dt>
            <dd :title="balance.deposited">
              {{ displayAmount(balance.deposited, currency) }}
            </dd>
          </div>
          <div>
            <dt>Available</dt>
            <dd :title="balance.available">
              {{ displayAmount(balance.available, currency) }}
            </dd>
          </div>
          <div>
            <dt>In baskets</dt>
            <dd :title="balance.basketValue">
              {{ displayAmount(balance.basketValue, currency) }}
            </dd>
          </div>
          <div>
            <dt>Net P&amp;L</dt>
            <dd :title="balance.net">
              {{ displayAmount(balance.net, currency) }}
            </dd>
          </div>
        </dl>
      </div>
      <p v-else>
        Your wallet is empty. Add paper funds in the currency you want to use.
      </p>
      <div class="actions-row">
        <button
          class="primary"
          :disabled="busy"
          @click="depositing = !depositing"
        >
          Add paper funds</button
        ><a href="#baskets">Browse baskets <Icon name="ChevronRightMd" /></a>
      </div>
      <form v-if="depositing" @submit.prevent="deposit">
        <label
          >Deposit currency<input
            list="paper-currencies"
            v-model.trim="unit"
            maxlength="16"
            pattern="[A-Za-z0-9]{1,16}"
            required
        /></label>
        <datalist id="paper-currencies">
          <option
            v-for="c in ['TRY', 'USD', 'USDT', 'USDC', 'EUR']"
            :key="c"
            :value="c"
          />
        </datalist>
        <label
          >Deposit amount<input
            v-model.trim="amount"
            inputmode="decimal"
            pattern="[0-9]{1,12}(\.[0-9]{1,8})?"
            required
            placeholder="0.00"
        /></label>
        <button :disabled="busy">Confirm paper deposit</button>
      </form>
    </section>
    <div class="tabs">
      <button
        v-for="s in ['Coins', 'Baskets', 'History'] as const"
        :key="s"
        :aria-pressed="section === s"
        @click="section = s"
      >
        {{ s }}
      </button>
    </div>
    <section v-if="section === 'Coins'" aria-label="Coin holdings">
      <p v-if="!coins.length" class="caption">
        No coin holdings in this currency.
      </p>
      <div v-for="c in coins" :key="c.asset + c.venue" class="row">
        <div>
          <strong>{{ c.asset }}</strong>
          <p class="caption">{{ c.venue }} · {{ date(c.asOf) }}</p>
        </div>
        <div class="right">
          <strong>{{ c.quantity }} {{ c.asset }}</strong>
          <p class="caption">
            {{ displayAmount(c.value, c.currency) }} {{ c.currency }}
          </p>
        </div>
      </div>
    </section>
    <section v-else-if="section === 'Baskets'" aria-label="Basket returns">
      <p v-if="!positions.length" class="caption">
        No basket entries in this currency.
      </p>
      <div v-for="p in positions" :key="p.id" class="row">
        <div>
          <a
            v-if="p.status !== 'Access paused' && p.status !== 'Closed'"
            :href="'#baskets?id=' + p.basket"
            ><strong>{{ p.name }}</strong> <Icon name="ChevronRightMd" /></a
          ><strong v-else>{{ p.name }}</strong>
          <p class="caption">
            {{ p.status }} · Entered {{ date(p.enteredAt) }} · Revision
            {{ p.revision }}
          </p>
        </div>
        <div class="right">
          <strong>{{ p.netReturnPct }}%</strong>
          <p v-if="p.asOfMs" class="caption">As of {{ date(p.asOfMs) }}</p>
          <p class="caption">Entry {{ p.contribution }} {{ p.currency }}</p>
        </div>
      </div>
    </section>
    <section v-else aria-label="Transaction history">
      <h2>Transaction history</h2>
      <p class="caption">All deposit currencies · newest first</p>
      <p v-if="!activity.length">No transactions yet.</p>
      <article v-for="e in activity" :key="e.id">
        <div class="row">
          <div>
            <strong>{{ e.kind }}</strong>
            <p class="caption">{{ e.name }} {{ date(e.at) }}</p>
          </div>
          <div class="right">{{ e.amount }} {{ e.currency }}</div>
        </div>
        <details v-if="e.receipt">
          <summary>
            Fill details · Fees {{ e.receipt.fees }} {{ e.currency }}
          </summary>
          <p
            v-for="f in e.receipt.fills.filter((f) => f.quantity !== '0')"
            :key="f.venue + f.pair"
          >
            {{ f.side }} {{ f.quantity }} {{ f.pair }} at {{ f.price }} ·
            {{ f.venue }} · Fee {{ f.fee }} {{ e.currency }}
          </p>
        </details>
      </article>
      <button v-if="next" :disabled="busy" @click="perform(() => read(next))">
        Older transactions
      </button>
    </section>
  </template>
</template>
<style scoped>
.profile-head,
.wallet-top,
.row,
.actions-row,
form,
.tabs {
  display: flex;
  align-items: center;
  gap: 0.9rem;
  flex-wrap: wrap;
}
.profile-head {
  margin: 1rem 0;
}
.profile-head .tool {
  margin-left: auto;
}
.avatar {
  width: 2.6rem;
  height: 2.6rem;
  border-radius: 50%;
  display: grid;
  place-items: center;
  background: var(--hover);
  font-weight: 650;
}
.wallet {
  padding: clamp(1rem, 3vw, 2rem);
  margin-bottom: 1.3rem;
}
.wallet-top,
.row {
  justify-content: space-between;
}
.wallet-top {
  margin-bottom: 1.2rem;
}
.total {
  font-size: clamp(1.8rem, 4vw, 3rem);
  letter-spacing: -0.04em;
  font-weight: 620;
  overflow-wrap: anywhere;
}
.total small {
  font-size: 0.45em;
  color: var(--muted);
  letter-spacing: normal;
}
.values {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
  gap: 0.8rem;
  padding: 1rem 0;
}
.values dt,
.caption {
  color: var(--muted);
}
.values dd {
  overflow-wrap: anywhere;
}
.actions-row {
  margin-top: 1rem;
}
.actions-row a {
  display: flex;
  align-items: center;
}
.primary {
  background: var(--eighred-white);
  color: var(--eighred-black);
  border-radius: 8px;
  padding: 0.55rem 1rem;
  font-weight: 600;
}
form {
  align-items: end;
  margin-top: 1rem;
}
form label {
  display: grid;
  gap: 0.25rem;
}
input,
select,
form button {
  color: inherit;
  background: var(--base);
  border: 1px solid var(--line);
  padding: 0.4rem 0.6rem;
  border-radius: 6px;
  font: inherit;
  max-width: 100%;
}
.tabs {
  border-bottom: 1px solid var(--line);
  margin-bottom: 1rem;
}
.tabs button {
  padding: 0.65rem 0.25rem;
  border-bottom: 2px solid transparent;
}
.tabs [aria-pressed="true"] {
  border-color: var(--eighred-blue);
}
.row {
  border-bottom: 1px solid var(--line);
  padding: 0.85rem 0;
  overflow-wrap: anywhere;
}
.right {
  text-align: right;
}
.wallet-top label {
  display: flex;
  align-items: center;
  gap: 0.4rem;
}
details {
  padding: 0.4rem 0 0.8rem;
}
summary {
  cursor: pointer;
}
</style>
