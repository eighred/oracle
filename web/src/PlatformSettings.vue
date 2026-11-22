<script setup lang="ts">
import { ref, onMounted } from "vue";
type Settings = {
  connections: {
    venue: string;
    secret_ref: string;
    enabled: boolean;
    revision: number;
  }[];
  fees: { bps: number; effective_at: string; proposed_at: string }[];
};
const data = ref<Settings>(),
  message = ref(""),
  busy = ref(false),
  venue = ref("binance"),
  bps = ref(1500),
  effective = ref("");
async function submit(op: string, fields: Record<string, unknown> = {}) {
  if (busy.value) return;
  busy.value = true;
  message.value = "";
  try {
    const res = await fetch("/api/platform-settings", {
      method: "POST",
      credentials: "same-origin",
      cache: "no-store",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ op, request: crypto.randomUUID(), ...fields }),
    });
    if (!res.ok) {
      if ([401, 403].includes(res.status)) data.value = undefined;
      message.value =
        res.status === 403
          ? "Platform administrator access required."
          : res.status === 401
            ? "Sign in to manage settings."
            : "Change unavailable. Check the rate and future effective time.";
      return;
    }
    data.value = await res.json();
  } catch {
    message.value = "Platform settings unavailable.";
  } finally {
    busy.value = false;
  }
}
onMounted(() => void submit("list"));
</script>
<template>
  <p v-if="message" role="status">{{ message }}</p>
  <template v-if="data">
    <section class="panel settings">
      <h2>Company exchange connections</h2>
      <p class="caption">
        Staged connections use server-side vault references. Activation for
        trading requires a separate execution mandate.
      </p>
      <form @submit.prevent="submit('connection', { venue, enabled: true })">
        <label
          >Exchange<select v-model="venue">
            <option value="binance">Binance</option>
            <option value="okx">OKX</option>
          </select></label
        ><button :disabled="busy">Stage connection</button>
      </form>
      <div v-for="c in data.connections" :key="c.venue">
        <strong>{{ c.venue }}</strong> ·
        {{ c.enabled ? "Staged" : "Disabled" }} ·
        <code>{{ c.secret_ref }}</code>
        <button
          v-if="c.enabled"
          :disabled="busy"
          @click="submit('connection', { venue: c.venue, enabled: false })"
        >
          Disable connection
        </button>
      </div>
    </section>
    <section class="panel settings">
      <h2>Profit-share schedule</h2>
      <p class="caption">
        Future-effective proposals for review. Collection is disabled.
      </p>
      <form
        @submit.prevent="
          submit('fee', { bps, effective: new Date(effective).toISOString() })
        "
      >
        <label
          >Rate (bps)<input
            type="number"
            v-model.number="bps"
            min="0"
            max="10000"
            required /></label
        ><label
          >Effective time (local)<input
            type="datetime-local"
            v-model="effective"
            required /></label
        ><button :disabled="busy">Propose rate</button>
      </form>
      <p v-for="f in data.fees" :key="f.proposed_at">
        {{ f.bps / 100 }}% · {{ new Date(f.effective_at).toLocaleString() }} ·
        Proposed
      </p>
    </section>
  </template>
</template>
<style scoped>
.settings {
  padding: 1rem;
  margin: 1rem 0;
}
form {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 0.7rem;
  margin: 0.75rem 0;
}
label {
  display: grid;
  gap: 0.2rem;
}
button,
input,
select {
  padding: 0.4rem 0.65rem;
  color: inherit;
  background: var(--base);
  border: 1px solid var(--line);
  border-radius: 6px;
  font: inherit;
}
</style>
