<script setup lang="ts">
import sprite from "./assets/openai-icons.svg?no-inline";

defineProps<{
  name:
    | "Sidebar"
    | "Chart"
    | "Compare"
    | "Notebook"
    | "Info"
    | "Regenerate"
    | "XCrossed"
    | "ChevronRightMd"
    | "Folder"
    | "Download"
    | "Clock";
}>();
</script>

<template>
  <svg class="icon" aria-hidden="true" focusable="false">
    <use :href="sprite + '#' + name" />
  </svg>
</template>
