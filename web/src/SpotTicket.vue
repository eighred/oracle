<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, computed } from "vue";
import { apiFetch } from "./api";

const props = defineProps<{ pair: string }>();
const emit = defineEmits<{ close: [] }>();
type Quote = {
  id: string;
  pair: string;
  side: string;
  quantity: string;
  price: string;
  fee: string;
  spread: string;
  total: string;
  currency: string;
  expires: string;
  asOf: string;
  settled: string | null;
};
const side = ref("buy"),
  quantity = ref(""),
  ticket = ref<Quote>(),
  busy = ref(false),
  message = ref(""),
  now = ref(Date.now());
const remaining = computed(() =>
  ticket.value
    ? Math.max(
        0,
        Math.ceil((Date.parse(ticket.value.expires) - now.value) / 1000),
      )
    : 0,
);
const dialog = ref<HTMLDialogElement>();
const timer = window.setInterval(() => (now.value = Date.now()), 500);
onMounted(() => dialog.value?.showModal());
onBeforeUnmount(() => clearInterval(timer));
async function send(confirm = false) {
  if (busy.value) return;
  busy.value = true;
  message.value = "";
  if (!confirm) ticket.value = undefined;
  try {
    const body = confirm
      ? { action: "confirm", id: ticket.value?.id }
      : {
          action: "quote",
          pair: props.pair,
          side: side.value,
          quantity: quantity.value,
        };
    const res = await apiFetch("/api/paper/spot", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok)
      throw Error(
        (
          {
            401: "Sign in to trade with paper funds.",
            409: "This quote is no longer available. Request a new quote.",
            422: confirm
              ? "Insufficient paper balance or holdings."
              : "Check the quantity and the permitted order size.",
            503: "No fresh, eligible book level covers this quantity.",
          } as Record<number, string>
        )[res.status] ?? "Paper trade unavailable.",
      );
    ticket.value = await res.json();
    now.value = Date.now();
  } catch (e) {
    message.value = e instanceof Error ? e.message : "Paper trade unavailable.";
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <dialog
    ref="dialog"
    class="spot-ticket"
    aria-labelledby="ticket-title"
    @close="emit('close')"
    @click.self="dialog?.close()"
  >
    <div class="ticket-heading">
      <h2 id="ticket-title">{{ pair }}</h2>
      <button aria-label="Close trade ticket" @click="dialog?.close()">
        ×
      </button>
    </div>
    <p class="caption">Paper spot order</p>
    <form v-if="!ticket" @submit.prevent="send()">
      <label
        >Side<select v-model="side">
          <option value="buy">Buy</option>
          <option value="sell">Sell</option>
        </select></label
      >
      <label
        >Quantity · {{ pair.split("/")[0]
        }}<input
          v-model.trim="quantity"
          required
          inputmode="decimal"
          maxlength="32"
          pattern="[0-9]+(\.[0-9]+)?"
      /></label>
      <button class="app-button" :disabled="busy">
        {{ busy ? "Reading books…" : "Request paper quote" }}
      </button>
    </form>
    <template v-else
      ><p>
        {{ ticket.side === "buy" ? "Buy" : "Sell" }} {{ ticket.quantity }}
        {{ pair.split("/")[0] }}
      </p>
      <dl>
        <dt>Customer price</dt>
        <dd>{{ ticket.price }} {{ ticket.currency }}</dd>
        <dt>Trading fee · 10 bps</dt>
        <dd>{{ ticket.fee }} {{ ticket.currency }}</dd>
        <dt>Eighred spread included in price</dt>
        <dd>{{ ticket.spread }} {{ ticket.currency }}</dd>
        <dt>{{ side === "buy" ? "You pay" : "You receive" }}</dt>
        <dd>{{ ticket.total }} {{ ticket.currency }}</dd>
      </dl>
      <p v-if="ticket.settled" role="status">
        Paper order settled ·
        <a href="#profile" @click="dialog?.close()">View wallet</a>
      </p>
      <template v-else
        ><p class="caption">Quote valid for {{ remaining }}s</p>
        <button
          class="app-button"
          :disabled="!remaining || busy"
          @click="send(true)"
        >
          Confirm paper order
        </button>
        <button
          :disabled="busy"
          @click="
            ticket = undefined;
            message = '';
          "
        >
          Edit order
        </button></template
      ></template
    >
    <p v-if="message" role="status">{{ message }}</p>
  </dialog>
</template>
<style scoped>
.spot-ticket {
  margin: auto;
  width: min(94vw, 420px);
  padding: 24px;
  color: var(--color-paper-white);
  background: var(--color-charcoal);
  border: 1px solid var(--line);
  border-radius: 12px;
}
.spot-ticket::backdrop {
  background: rgb(0 0 0 / 0.65);
}
.ticket-heading,
dl {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.ticket-heading button {
  font-size: 24px;
}
.spot-ticket p,
.spot-ticket form {
  margin: 12px 0;
}
.spot-ticket label {
  display: grid;
  gap: 5px;
}
.spot-ticket form {
  display: grid;
  gap: 14px;
}
input,
select {
  color: inherit;
  background: var(--base);
  border: 1px solid var(--line);
  padding: 8px;
}
.spot-ticket dl {
  align-items: baseline;
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 10px;
  margin: 18px 0;
}
.spot-ticket dt {
  color: var(--muted);
}
.spot-ticket dd {
  margin: 0;
  text-align: right;
  overflow-wrap: anywhere;
}
.spot-ticket a {
  color: var(--signal);
}
</style>
