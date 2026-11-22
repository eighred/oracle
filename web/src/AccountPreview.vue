<script setup lang="ts">
import { ref } from "vue";

const step = ref<"closed" | "methods" | "card" | "transfer" | "identity">(
  "closed",
);
const unit = ref("TRY");
</script>
<template>
  <section class="account-services" aria-label="Account services">
    <button @click="step = 'methods'">Funding methods</button>
    <button @click="step = 'identity'">Identity verification</button>
    <div v-if="step !== 'closed'" class="service-sheet">
      <button
        class="dismiss"
        aria-label="Close account preview"
        @click="step = 'closed'"
      >
        ×
      </button>
      <template v-if="step === 'identity'">
        <h2>Identity verification</h2>
        <p class="caption">Preview · Verification provider pending</p>
        <ol>
          <li>Confirm legal name and residency</li>
          <li>Verify an identity document</li>
          <li>Complete a liveness check</li>
          <li>Receive a review decision</li>
        </ol>
        <p>
          Document collection opens when a verification provider is connected.
        </p>
      </template>
      <template v-else>
        <h2>Add funds</h2>
        <p class="caption">Preview · Live deposits unavailable</p>
        <label
          >Deposit currency
          <select v-model="unit">
            <option>TRY</option>
            <option>USD</option>
            <option>EUR</option>
          </select></label
        >
        <div class="methods">
          <button :aria-pressed="step === 'card'" @click="step = 'card'">
            Bank card
          </button>
          <button
            :aria-pressed="step === 'transfer'"
            @click="step = 'transfer'"
          >
            Bank transfer (IBAN)
          </button>
        </div>
        <p v-if="step === 'card'">
          Card funding in {{ unit }} will open a secure payment page after
          identity verification.
        </p>
        <p v-if="step === 'transfer'">
          Your beneficiary, IBAN and unique deposit reference will appear here
          once a banking partner supports {{ unit }}.
        </p>
      </template>
    </div>
  </section>
</template>
<style scoped>
.account-services {
  margin: 14px 0 22px;
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.account-services button,
.account-services select {
  font: inherit;
  border: 1px solid var(--line);
  padding: 7px 12px;
  color: inherit;
  background: var(--base);
  border-radius: 6px;
}
.service-sheet {
  position: relative;
  flex-basis: 100%;
  padding: 20px;
  background: var(--color-obsidian);
  border-radius: 8px;
}
.service-sheet h2 {
  margin-right: 32px;
}
.service-sheet p,
.service-sheet ol {
  margin: 12px 0;
}
.service-sheet ol {
  padding-left: 22px;
  line-height: 1.7;
}
.dismiss {
  position: absolute;
  right: 12px;
  top: 12px;
}
.methods {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin: 12px 0;
}
.methods [aria-pressed="true"] {
  color: var(--signal);
}
</style>
