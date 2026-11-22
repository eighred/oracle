import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import {
  accountReady,
  apiFetch,
  memberHandle,
  memberLogin,
  native,
  signedIn,
} from "./api";

type Saved = { revision: number; pairs: string[] };
const endpoint = "/api/v1/me/watchlist";

export function useWatchlist() {
  const saved = ref<Saved>();
  const busy = ref(false);
  const notice = ref("");
  let session = 0;
  let opened = false;

  async function sync(edit?: (pairs: string[]) => string[]) {
    if (busy.value) return;
    opened = true;
    if (!signedIn.value && (accountReady.value || native)) {
      notice.value = "Sign in to use your watchlist.";
      return;
    }
    const current = session;
    busy.value = true;
    notice.value = "";
    try {
      if (!edit || !saved.value) {
        const r = await apiFetch(endpoint, { cache: "no-store" });
        if (!r.ok) throw Error("Could not load your watchlist.");
        const snapshot: Saved = await r.json();
        if (current !== session) return;
        saved.value = snapshot;
      }
      if (edit && saved.value) {
        const pairs = edit([...saved.value.pairs]);
        if (pairs.length > 100) throw Error("You can watch up to 100 markets.");
        const r = await apiFetch(endpoint, {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ revision: saved.value.revision, pairs }),
        });
        if (r.status !== 409 && !r.ok)
          throw Error("Could not save your watchlist.");
        const snapshot: Saved = await r.json();
        if (current !== session) return;
        saved.value = snapshot;
        if (r.status === 409)
          notice.value =
            "Your watchlist changed on another device. Review the new order and try again.";
      }
    } catch (error) {
      if (current === session)
        notice.value =
          error instanceof Error ? error.message : "Watchlist unavailable.";
    } finally {
      if (current === session) busy.value = false;
    }
  }

  function toggle(pair: string) {
    if (!signedIn.value && (accountReady.value || native)) {
      void memberLogin();
      return;
    }
    const remove = saved.value?.pairs.includes(pair);
    void sync((pairs) =>
      remove ? pairs.filter((p) => p !== pair) : [...new Set([...pairs, pair])],
    );
  }

  function move(pair: string, offset: number) {
    void sync((pairs) => {
      const from = pairs.indexOf(pair),
        to = from + offset;
      if (from >= 0 && to >= 0 && to < pairs.length) {
        pairs.splice(from, 1);
        pairs.splice(to, 0, pair);
      }
      return pairs;
    });
  }

  function resume() {
    if (document.visibilityState === "visible" && (opened || signedIn.value))
      void sync();
  }
  watch([memberHandle, signedIn], () => {
    session++;
    saved.value = undefined;
    notice.value = "";
    busy.value = false;
    opened = false;
    if (signedIn.value) void sync();
  });
  onMounted(() => {
    if (signedIn.value) void sync();
    window.addEventListener("focus", resume);
    window.addEventListener("online", resume);
    document.addEventListener("visibilitychange", resume);
  });
  onBeforeUnmount(() => {
    session++;
    window.removeEventListener("focus", resume);
    window.removeEventListener("online", resume);
    document.removeEventListener("visibilitychange", resume);
  });
  return { saved, busy, notice, sync, toggle, move };
}
