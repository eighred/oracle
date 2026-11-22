<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { colors, numeric, type Rule, type Run } from "./report";
const props = defineProps<{ runs: Run[]; active: Rule }>();
const canvas = ref<HTMLCanvasElement>();
let observer: ResizeObserver | undefined;
function draw() {
  const el = canvas.value;
  if (!el) return;
  const ctx = el.getContext("2d");
  if (!ctx) return;
  const { width, height } = el.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  el.width = Math.round(width * dpr);
  el.height = Math.round(height * dpr);
  ctx.scale(dpr, dpr);
  const points = props.runs.map((run) => ({
    rule: run.rule,
    values: [run.opening_nav, ...run.ledger.map((r) => r.nav)].map(numeric),
  }));
  if (!points.length) return;
  const values = points.flatMap((series) => series.values);
  const lo = Math.floor(Math.min(...values) / 2) * 2,
    hi = Math.max(lo + 2, Math.ceil(Math.max(...values) / 2) * 2);
  const y = (v: number) => 12 + ((hi - v) / (hi - lo)) * (height - 27);
  const x = (index: number, count: number) =>
    39 + (index / Math.max(count - 1, 1)) * (width - 49);
  ctx.font = "11px ui-monospace, monospace";
  const styles = getComputedStyle(el);
  ctx.fillStyle = styles.getPropertyValue("--eighred-gray");
  for (let i = 0; i <= 4; i++) {
    const v = lo + ((hi - lo) * i) / 4;
    ctx.fillText(String(v), 0, y(v) + 4);
    ctx.beginPath();
    ctx.moveTo(36, y(v));
    ctx.lineTo(width - 6, y(v));
    ctx.strokeStyle = styles.getPropertyValue("--eighred-gray");
    ctx.setLineDash([1, 4]);
    ctx.lineWidth = 1;
    ctx.stroke();
  }
  for (const series of points.sort(
    (a, b) => Number(a.rule === props.active) - Number(b.rule === props.active),
  )) {
    ctx.beginPath();
    ctx.strokeStyle = styles.getPropertyValue(colors[series.rule]);
    ctx.lineWidth = series.rule === props.active ? 3 : 1.7;
    ctx.setLineDash(series.rule === "dca" ? [5, 3] : []);
    series.values.forEach((value, i) =>
      i
        ? ctx.lineTo(x(i, series.values.length), y(value))
        : ctx.moveTo(x(i, series.values.length), y(value)),
    );
    ctx.stroke();
    ctx.setLineDash([]);
  }
}
onMounted(() => {
  if (canvas.value) {
    observer = new ResizeObserver(draw);
    observer.observe(canvas.value);
  }
});
watch(() => [props.runs, props.active], draw, { flush: "post" });
onBeforeUnmount(() => observer?.disconnect());
</script>
<template>
  <canvas
    ref="canvas"
    aria-label="Minute-close NAV by strategy. Exact values are in the settlement ledger."
    role="img"
  />
</template>
