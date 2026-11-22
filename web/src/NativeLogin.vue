<script setup lang="ts">
import { ref } from "vue";
import { signIn } from "./api";
const username = ref(""),
  password = ref(""),
  message = ref(""),
  busy = ref(false);
async function submit() {
  if (busy.value) return;
  busy.value = true;
  message.value = "";
  try {
    await signIn(username.value, password.value);
  } catch (e) {
    message.value = e instanceof Error ? e.message : "Cannot reach Oracle.";
  } finally {
    password.value = "";
    busy.value = false;
  }
}
</script>
<template>
  <form class="panel native-login" @submit.prevent="submit">
    <h2>Connect your paper account</h2>
    <label
      >Username<input
        v-model="username"
        required
        maxlength="128"
        autocomplete="username"
    /></label>
    <label
      >Passphrase<input
        v-model="password"
        type="password"
        required
        maxlength="256"
        autocomplete="current-password"
    /></label>
    <button class="app-button" :disabled="busy">
      {{ busy ? "Connecting…" : "Sign in" }}
    </button>
    <p v-if="message" role="status">{{ message }}</p>
  </form>
</template>
