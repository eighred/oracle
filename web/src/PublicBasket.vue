<script setup lang="ts">
import { onMounted, ref } from "vue";
import { apiFetch, memberLogin, accountReady } from "./api";
const invited=new URLSearchParams(location.hash.split("?")[1]).has("share");
const selection = new URLSearchParams(location.hash.split("?")[1]).get("id");
type Basket = {
  id: string;
  name: string;
  creator: string;
  revision: number;
  users: number;
  items: { pair: string; bps: number }[];
};
const rows = ref<Basket[]>([]),
  message = ref("Loading public baskets…");
onMounted(async () => {
  if(invited){message.value="Sign in to open this private invitation.";return;}
  try {
    const res = await apiFetch(
      "/api/discover/baskets" +
        (selection ? "?id=" + encodeURIComponent(selection) : ""),
    );
    if (!res.ok) throw Error();
    rows.value = (await res.json()).baskets;
    message.value = rows.value.length ? "" : "No published basket available.";
  } catch {
    message.value = "Public baskets unavailable.";
  }
});
</script>
<template>
  <section class="public-library">
    <p v-if="message" role="status">{{ message }}</p>
    <article v-for="b in rows" :key="b.id">
      <h2>
        <a :href="'#baskets?id=' + b.id">{{ b.name }}</a>
      </h2>
      <p class="caption">
        {{ b.creator }} · Revision {{ b.revision }} · {{ b.users }} funded
        participants
      </p>
      <dl v-if="selection">
        <div v-for="i in b.items" :key="i.pair + '-' + i.bps">
          <dt>{{ i.pair }}</dt>
          <dd>{{ i.bps / 100 }}%</dd>
        </div>
      </dl>
    </article>
    <button v-if="accountReady" class="app-button" @click="memberLogin()">
      Sign in to copy or fund a basket
    </button>
  </section>
</template>
<style scoped>
.public-library article {
  padding: 14px 0;
  border-bottom: 1px solid var(--line);
}
.public-library dl div {
  display: flex;
  justify-content: space-between;
  padding: 6px 0;
  max-width: 340px;
}
.public-library button {
  margin-top: 16px;
}
</style>
