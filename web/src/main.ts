import { createApp } from "vue";
import { native } from "./api";
import App from "./App.vue";
import "./style.css";

createApp(App).mount("#app");

if (!native && import.meta.env.PROD && "serviceWorker" in navigator) {
  navigator.serviceWorker.register("/sw.js").catch(() => {});
}
