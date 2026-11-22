<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
type Item = { venue: string; pair: string; bps: number };
type Basket = {
  id: string;
  name: string;
  items: Item[];
  revision: number;
  creator: string;
  mine: boolean;
  public: boolean;
  pinned: boolean;
  users: number;
  joined: boolean;
  invites: string[] | null;
  linkActive: boolean;
  sourceRevision: number | null;
};
const props = defineProps<{ initialPair?: string }>();
const emit = defineEmits<{ risk: [items: Item[]] }>();
const rows = ref<Basket[]>([]),
  curator = ref(false),
  busy = ref(false),
  message = ref(""),
  ready = ref(false);
const editing = ref(false),
  editId = ref(""),
  revision = ref(0),
  title = ref(""),
  items = ref<Item[]>([]);
const detail = ref(""),
  member = ref(""),
  pass = ref(""),
  share = ref(""),
  unlockPass = ref("");
const ticket = ref(
  new URLSearchParams(location.hash.split("?")[1]).get("share") ?? "",
);
const selected = computed(() => rows.value.find((b) => b.id === detail.value));
const group = ref("Library");
const filtered = computed(() =>
  rows.value.filter((b) =>
    group.value === "Popular"
      ? b.public
      : group.value === "Joined"
        ? b.joined
        : b.mine || b.joined || !b.public,
  ),
);
const total = computed(() =>
  items.value.reduce((sum, i) => sum + Number(i.bps), 0),
);
async function api(payload?: Record<string, unknown>) {
  const res = await fetch("/api/baskets", {
    method: payload ? "POST" : "GET",
    cache: "no-store",
    credentials: "same-origin",
    ...(payload
      ? {
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        }
      : {}),
  });
  if (!res.ok)
    throw Error(
      (
        {
          401: "Sign in to use baskets.",
          403: "This action requires a curator.",
          404: "Basket, recipient or share credentials unavailable.",
          409: "The basket has changed. Reopen it before saving.",
          422: "Check the name, pairs, weights and password.",
        } as Record<number, string>
      )[res.status] ?? "Basket service unavailable.",
    );
  return res.json();
}
async function load() {
  const data = await api().catch((error) => {
    rows.value = [];
    curator.value = false;
    ready.value = false;
    share.value = "";
    throw error;
  });
  rows.value = data.baskets;
  curator.value = data.curator;
  ready.value = true;
}
async function perform(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  message.value = "";
  try {
    await fn();
  } catch (e) {
    message.value =
      e instanceof Error ? e.message : "Basket service unavailable.";
  } finally {
    busy.value = false;
  }
}
function select(id: string) {
  detail.value = id;
  share.value = "";
  pass.value = "";
  member.value = "";
  editing.value = false;
}
function editor(b?: Basket) {
  editId.value = b?.id ?? crypto.randomUUID();
  revision.value = b?.revision ?? 0;
  title.value = b?.name ?? "";
  items.value = b
    ? b.items.map((i) => ({ ...i }))
    : [
        {
          venue: props.initialPair ? "okx" : "binance",
          pair: props.initialPair ?? "BTC/USDT",
          bps: 10000,
        },
      ];
  editing.value = true;
  message.value = "";
}
async function save() {
  await perform(async () => {
    const r = await api({
      op: revision.value ? "edit" : "create",
      id: editId.value,
      revision: revision.value,
      name: title.value,
      items: items.value,
    });
    await load();
    select(r.id);
    group.value = "Library";
  });
}
async function action(op: string, extra: Record<string, unknown> = {}) {
  await perform(async () => {
    const r = await api({ op, id: detail.value, ...extra });
    await load();
    if (op === "join") group.value = "Joined";
    if (op === "clone" || op === "unlock") {
      select(r.id);
      group.value = "Library";
    }
    if (op === "link") {
      share.value =
        location.origin +
        location.pathname +
        "#baskets?share=" +
        encodeURIComponent(r.token);
    }
    if (op === "unlink") share.value = "";
    if (op === "unlock") {
      ticket.value = "";
      unlockPass.value = "";
    }
    member.value = "";
    pass.value = "";
  });
}
onMounted(() => {
  if (ticket.value)
    history.replaceState(
      null,
      "",
      location.pathname + location.search + "#baskets",
    );
  void perform(async () => {
    await load();
    if (props.initialPair) editor();
  });
});
const refreshTimer = window.setInterval(() => {
  if (
    document.visibilityState === "visible" &&
    !busy.value &&
    !editing.value &&
    (group.value === "Joined" || selected.value?.joined)
  )
    void perform(load);
}, 15000);
onBeforeUnmount(() => clearInterval(refreshTimer));
</script>
<template>
  <p v-if="message" role="status">{{ message }}</p>
  <form
    v-if="ticket"
    class="panel basket-panel"
    @submit.prevent="action('unlock', { token: ticket, password: unlockPass })"
  >
    <h2>Open private basket</h2>
    <label
      >Share password<input
        v-model="unlockPass"
        type="password"
        required
        autocomplete="off" /></label
    ><button :disabled="busy">Open basket</button>
  </form>
  <div class="basket-tools">
    <button
      v-for="g in ['Library', 'Joined', 'Popular']"
      :key="g"
      :aria-pressed="group === g"
      @click="
        group = g;
        select('');
      "
    >
      {{ g }}
    </button>
    <button :disabled="busy" @click="editor()">Create basket</button>
  </div>
  <form v-if="editing" class="panel basket-panel" @submit.prevent="save">
    <fieldset :disabled="busy">
      <legend>{{ revision ? "Edit basket" : "New private basket" }}</legend>
      <label
        >Basket name<input v-model.trim="title" maxlength="80" required
      /></label>
      <div v-for="(item, i) in items" :key="i" class="basket-tools">
        <label
          >Venue<select v-model="item.venue">
            <option value="binance">Binance</option>
            <option value="okx">OKX</option>
          </select></label
        >
        <label
          >Spot pair<input
            v-model.trim="item.pair"
            required
            placeholder="BASE/QUOTE"
        /></label>
        <label
          >Weight (bps)<input
            v-model.number="item.bps"
            type="number"
            min="1"
            max="10000"
            required
        /></label>
        <button
          type="button"
          :disabled="items.length === 1"
          @click="items.splice(i, 1)"
        >
          Remove
        </button>
      </div>
      <p>{{ total }} / 10,000 bps allocated</p>
      <div class="basket-tools">
        <button
          type="button"
          :disabled="items.length >= 8"
          @click="items.push({ venue: 'okx', pair: '', bps: 0 })"
        >
          Add holding</button
        ><button :disabled="total !== 10000">Save basket</button
        ><button type="button" @click="editing = false">Cancel</button>
      </div>
    </fieldset>
  </form>
  <section v-else-if="selected" class="panel basket-panel">
    <h2>{{ selected.name }}</h2>
    <p class="caption">Created by {{ selected.creator }}</p>
    <p>
      {{ selected.public ? "Public" : "Private" }} · Revision
      {{ selected.revision
      }}<span v-if="selected.sourceRevision">
        · Copied from revision {{ selected.sourceRevision }}</span
      >
    </p>
    <table>
      <thead>
        <tr>
          <th>Venue</th>
          <th>Pair</th>
          <th>Weight</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="i in selected.items" :key="i.venue + i.pair">
          <td>{{ i.venue }}</td>
          <td>{{ i.pair }}</td>
          <td>{{ i.bps / 100 }}%</td>
        </tr>
      </tbody>
    </table>
    <div class="basket-tools">
      <button @click="emit('risk', selected.items)">Review basket risk</button
      ><button
        v-if="!selected.mine"
        :disabled="busy"
        @click="action('clone', { revision: selected.revision })"
      >
        Copy to my library
      </button>
      <button
        v-if="!selected.mine"
        :disabled="busy"
        @click="action(selected.joined ? 'leave' : 'join')"
      >
        {{ selected.joined ? "Leave basket" : "Join basket" }}
      </button>
      <template v-else
        ><button :disabled="busy" @click="editor(selected)">
          Edit composition</button
        ><button
          :disabled="busy"
          @click="action('publish', { enabled: !selected.public })"
        >
          {{ selected.public ? "Make private" : "Publish basket" }}
        </button>
        <button
          v-if="curator && selected.public"
          :disabled="busy"
          @click="action('pin', { enabled: !selected.pinned })"
        >
          {{ selected.pinned ? "Unpin" : "Pin official basket" }}
        </button></template
      >
    </div>
    <details v-if="selected.mine">
      <summary>Manage sharing</summary>
      <form class="basket-tools" @submit.prevent="action('invite', { member })">
        <label>Username<input v-model.trim="member" required /></label
        ><button :disabled="busy">Invite</button>
      </form>
      <div v-for="u in selected.invites" :key="u">
        {{ u }}
        <button :disabled="busy" @click="action('uninvite', { member: u })">
          Revoke invitation
        </button>
      </div>
      <form
        class="basket-tools"
        @submit.prevent="action('link', { password: pass })"
      >
        <label
          >Link password (12+ characters)<input
            type="password"
            v-model="pass"
            minlength="12"
            required
            autocomplete="new-password" /></label
        ><button :disabled="busy">
          {{ selected.linkActive ? "Replace link" : "Create private link" }}
        </button>
      </form>
      <label v-if="share"
        >Share link · expires in seven days<input
          readonly
          :value="share"
          @focus="($event.target as HTMLInputElement).select()"
      /></label>
      <button
        v-if="selected.linkActive"
        :disabled="busy"
        @click="action('unlink')"
      >
        Revoke link access
      </button>
    </details>
  </section>
  <section v-if="ready && !editing" class="panel basket-panel">
    <h2>
      {{
        group === "Popular"
          ? "Popular baskets"
          : group === "Joined"
            ? "Joined baskets"
            : "My and shared baskets"
      }}
    </h2>
    <p v-if="!filtered.length">No baskets here yet.</p>
    <button
      v-for="b in filtered"
      :key="b.id"
      class="basket-row"
      @click="select(b.id)"
    >
      <strong>{{ b.name }}</strong
      ><span
        >{{ b.pinned ? "Pinned by Eighred · " : "" }}{{ b.users }}
        {{ b.users === 1 ? "user" : "users" }} participating ·
        {{ b.public ? "Public" : "Private" }}</span
      >
    </button>
  </section>
</template>
<style scoped>
.basket-panel {
  margin: 1rem 0;
  padding: 1rem;
}
.basket-tools {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 0.65rem;
  margin: 0.75rem 0;
}
fieldset {
  min-width: 0;
  border: 0;
  padding: 0;
}
legend,
h2 {
  font-weight: 600;
}
label {
  display: grid;
  gap: 0.25rem;
}
input,
select,
button {
  padding: 0.45rem 0.65rem;
  font: inherit;
  color: inherit;
  background: var(--base);
  border: 1px solid var(--line);
  border-radius: 6px;
  max-width: 100%;
}
button,
summary {
  cursor: pointer;
}
button[aria-pressed="true"] {
  border-color: var(--eighred-blue);
}
button:disabled {
  opacity: 0.5;
}
.basket-row {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  width: 100%;
  text-align: left;
  margin: 0.4rem 0;
}
.basket-row span {
  color: var(--muted);
}
</style>
