<script setup lang="ts">
import { onMounted, ref } from "vue";
import { apiFetch, accountReady, memberLogin, signedIn } from "./api";
type Post = {
  id: string;
  author: string;
  avatar: string;
  body: string;
  pair: string;
  at: string;
  status: string;
};
type Profile = {
  handle: string;
  avatar: string;
  bio: string;
  public: boolean;
  ranking: boolean;
  following: boolean;
};
const props = defineProps<{ own?: boolean }>();
const tabs = ["global", "following", "profile", "rankings", "review"] as const;
const view = ref<(typeof tabs)[number]>(props.own ? "profile" : "global"),
  person = ref("");
const items = ref<Post[]>([]),
  me = ref(""),
  profile = ref<Profile>(),
  moderator = ref(false),
  busy = ref(false),
  notice = ref(""),
  next = ref("");
const ranks = ref<
    { rank: number; handle: string; returnPct: string; started: string }[]
  >([]),
  unit = ref("USDT"),
  asOf = ref("");
const bio = ref(""),
  photo = ref(""),
  visible = ref(false),
  ranking = ref(false),
  body = ref(""),
  symbol = ref("BTC/USDT"),
  lookup = ref("");
let previous: { text: string; id: string } | undefined;
const privateEntry = () => accountReady.value && !signedIn.value;
async function read(older = false) {
  const q = new URLSearchParams({ view: view.value, currency: unit.value });
  if (person.value && view.value === "profile") q.set("handle", person.value);
  if (older && next.value) q.set("before", next.value);
  const r = await apiFetch("/api/community?" + q, { cache: "no-store" });
  if (!r.ok)
    throw Error(
      r.status === 404
        ? "Profile unavailable."
        : "Sign in to open the member community.",
    );
  const data = await r.json();
  if (view.value === "rankings") {
    ranks.value = data.rankings;
    asOf.value = data.asOf;
    return;
  }
  me.value = data.viewer;
  moderator.value = data.moderator;
  profile.value = data.profile ?? undefined;
  if (
    view.value === "profile" &&
    (!person.value || person.value === me.value)
  ) {
    bio.value = profile.value?.bio ?? "";
    photo.value = profile.value?.avatar ?? "";
    visible.value = profile.value?.public ?? false;
    ranking.value = profile.value?.ranking ?? false;
  }
  const page: Post[] = data.posts.slice(0, 25);
  items.value = older ? [...items.value, ...page] : page;
  next.value = data.posts.length > 25 ? page[24]!.id : "";
}
async function work(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  notice.value = "";
  try {
    await fn();
  } catch (e) {
    notice.value = e instanceof Error ? e.message : "Community unavailable.";
  } finally {
    busy.value = false;
  }
}
function display(tab: (typeof tabs)[number], handle = "") {
  if (busy.value) return;
  view.value = tab;
  person.value = handle;
  items.value = [];
  profile.value = undefined;
  next.value = "";
  void work(() => read());
}
async function change(op: string, fields: Record<string, unknown>) {
  const r = await apiFetch("/api/community", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ op, ...fields }),
  });
  if (!r.ok)
    throw Error(
      r.status === 403
        ? "This action requires an eligible public profile or curator."
        : "Could not save this change.",
    );
}
function act(op: string, fields: Record<string, unknown>) {
  void work(async () => {
    await change(op, fields);
    if (op === "mute" || op === "block") {
      view.value = "global";
      person.value = "";
    }
    await read();
    notice.value = "Saved.";
  });
}
async function imageSelected(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  try {
    if (
      !["image/png", "image/jpeg", "image/webp"].includes(file.type) ||
      file.size > 5_000_000
    )
      throw Error();
    const bitmap = await createImageBitmap(file);
    const c = document.createElement("canvas");
    c.width = 96;
    c.height = 96;
    const ctx = c.getContext("2d");
    if (!ctx) throw Error();
    const side = Math.min(bitmap.width, bitmap.height);
    ctx.drawImage(
      bitmap,
      (bitmap.width - side) / 2,
      (bitmap.height - side) / 2,
      side,
      side,
      0,
      0,
      96,
      96,
    );
    bitmap.close();
    photo.value = c.toDataURL("image/jpeg", 0.8);
  } catch {
    notice.value = "Choose a JPEG, PNG or WebP image under 5 MB.";
  } finally {
    input.value = "";
  }
}
function publish() {
  const key = JSON.stringify([body.value, symbol.value]);
  if (previous?.text !== key) previous = { text: key, id: crypto.randomUUID() };
  const request = previous.id;
  void work(async () => {
    await change("post", { request, body: body.value, pair: symbol.value });
    body.value = "";
    previous = undefined;
    await read();
    notice.value = "Post submitted for review.";
  });
}
onMounted(() => {
  if (!privateEntry()) void work(() => read());
});
</script>
<template>
  <section class="trader-area">
    <h2>{{ own ? "Trader profile" : "Community feed" }}</h2>
    <button v-if="privateEntry()" class="app-button" @click="memberLogin()">
      Sign in to community
    </button>
    <template v-else>
      <nav aria-label="Feed views">
        <button
          v-for="tab in tabs.filter((t) => t !== 'review' || moderator)"
          :key="tab"
          :aria-pressed="view === tab"
          @click="display(tab)"
        >
          {{
            {
              global: "Global",
              following: "Following",
              profile: "My profile",
              rankings: "Rankings",
              review: "Moderation",
            }[tab]
          }}
        </button>
      </nav>
      <form class="lookup" @submit.prevent="display('profile', lookup.trim())">
        <label
          >Trader handle<input
            v-model="lookup"
            placeholder="@username"
            required
            maxlength="128" /></label
        ><button>Open profile</button>
      </form>
      <section v-if="view === 'rankings'">
        <h3>Paper portfolio rankings</h3>
        <form @submit.prevent="work(() => read())">
          <label
            >Quote currency<input
              v-model.trim="unit"
              maxlength="16"
              required
              pattern="[A-Z0-9]+" /></label
          ><button>Update rankings</button>
        </form>
        <p class="caption">
          Lifetime net gain divided by cumulative paper deposits. Includes
          opted-in traded portfolios with open positions marked in the last ten
          minutes. Each member�s start date is shown.
        </p>
        <p v-if="asOf" class="caption">
          As of {{ new Date(asOf).toLocaleString() }}
        </p>
        <article v-for="r in ranks" :key="r.handle">
          <button @click="display('profile', r.handle)">
            #{{ r.rank }} {{ r.handle }}
          </button>
          � {{ r.returnPct }}% � Since
          {{ new Date(r.started).toLocaleDateString() }}
        </article>
        <p v-if="!ranks.length">No eligible portfolios in this currency.</p>
      </section>
      <template v-if="view === 'profile'">
        <form
          v-if="!person || person === me"
          @submit.prevent="
            act('profile', { public: visible, ranking, bio, avatar: photo })
          "
        >
          <strong>{{ me }}</strong
          ><img
            v-if="photo"
            :src="photo"
            alt="Your profile photo"
            width="48"
            height="48"
          />
          <label
            >Profile photo<input
              type="file"
              accept="image/jpeg,image/png,image/webp"
              @change="imageSelected"
          /></label>
          <button type="button" v-if="photo" @click="photo = ''">
            Remove photo
          </button>
          <label>Bio<textarea v-model="bio" maxlength="240" /></label>
          <label
            ><input type="checkbox" v-model="visible" /> Publish my trader
            profile</label
          >
          <label
            ><input type="checkbox" v-model="ranking" /> Include my paper return
            in rankings when my profile is public</label
          >
          <button class="app-button" :disabled="busy">
            Save trader profile
          </button>
        </form>
        <div v-else-if="profile" class="person">
          <img
            v-if="profile.avatar"
            :src="profile.avatar"
            alt=""
            width="40"
            height="40"
          />
          <h3>{{ profile.handle }}</h3>
          <p>{{ profile.bio }}</p>
          <button
            @click="
              act('follow', { handle: person, enabled: !profile.following })
            "
          >
            {{ profile.following ? "Unfollow" : "Follow" }}
          </button>
          <button @click="act('mute', { handle: person, enabled: true })">
            Mute</button
          ><button @click="act('block', { handle: person, enabled: true })">
            Block
          </button>
        </div>
        <form
          v-if="profile?.handle === me && profile.public"
          @submit.prevent="publish"
        >
          <label
            >Spot pair<input
              v-model.trim="symbol"
              required
              maxlength="129" /></label
          ><label
            >Instrument thesis<textarea
              v-model.trim="body"
              maxlength="600"
              required
            />
          </label>
          <button :disabled="busy">Submit post</button>
        </form>
      </template>
      <p v-if="busy" role="status">Loading…</p>
      <p v-if="notice" role="status">{{ notice }}</p>
      <article v-for="p in items" :key="p.id">
        <header>
          <img v-if="p.avatar" :src="p.avatar" width="28" height="28" alt="" />
          <button @click="display('profile', p.author)">{{ p.author }}</button
          ><time>{{ new Date(p.at).toLocaleString() }}</time>
        </header>
        <strong>{{ p.pair }}</strong>
        <p>{{ p.body }}</p>
        <small v-if="p.status !== 'published'">{{ p.status }}</small>
        <button
          v-if="p.author === me && p.status !== 'hidden'"
          @click="act('remove', { id: p.id })"
        >
          Remove post
        </button>
        <template v-if="view === 'review'"
          ><button @click="act('moderate', { id: p.id, enabled: true })">
            Publish</button
          ><button @click="act('moderate', { id: p.id, enabled: false })">
            Hide
          </button></template
        >
      </article>
      <p
        v-if="view !== 'rankings' && !items.length && !busy && !notice"
        class="caption"
      >
        No posts in this view.
      </p>
      <button v-if="next" :disabled="busy" @click="work(() => read(true))">
        Older posts
      </button>
    </template>
  </section>
</template>
<style scoped>
.trader-area {
  margin: 24px 0;
  max-width: 720px;
}
.trader-area nav,
.lookup,
.person,
article header {
  display: flex;
  gap: 10px;
  align-items: center;
  flex-wrap: wrap;
}
.trader-area nav {
  margin: 12px 0;
  border-bottom: 1px solid var(--line);
}
.trader-area button {
  padding: 6px 10px;
}
.trader-area nav [aria-pressed="true"] {
  color: var(--signal);
  border-bottom: 2px solid currentColor;
}
.trader-area form {
  display: grid;
  gap: 10px;
  padding: 12px 0;
}
.trader-area .lookup {
  display: flex;
  align-items: end;
}
.trader-area label {
  display: grid;
  gap: 5px;
}
.trader-area label:has([type="checkbox"]) {
  display: block;
}
.trader-area input:not([type="checkbox"]),
.trader-area textarea {
  font: inherit;
  color: inherit;
  background: var(--color-obsidian);
  border: 1px solid var(--line);
  padding: 8px;
  max-width: 100%;
  border-radius: 4px;
}
.trader-area button:not(.app-button):hover {
  color: var(--signal);
}
.trader-area article,
.trader-area .person {
  border-top: 1px solid var(--line);
  padding: 14px 0;
}
.trader-area img {
  border-radius: 50%;
  object-fit: cover;
}
.trader-area time {
  color: var(--muted);
  font-size: 12px;
}
.trader-area article p {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  margin: 6px 0;
}
</style>
