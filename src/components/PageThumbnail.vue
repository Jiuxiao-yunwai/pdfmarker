<script lang="ts">
let thumbnailQueue = Promise.resolve();
</script>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import type { PDFDocumentProxy } from "pdfjs-dist";

const props = defineProps<{ document: PDFDocumentProxy; page: number; selected: boolean }>();
const emit = defineEmits<{ select: [page: number] }>();
const host = ref<HTMLElement>();
const canvas = ref<HTMLCanvasElement>();
let observer: IntersectionObserver | undefined;
let rendered = false;
let rendering = false;

async function render() {
  if (rendered || rendering || !canvas.value) return;
  rendering = true;
  try {
    const page = await props.document.getPage(props.page);
    const viewport = page.getViewport({ scale: 0.18 });
    const target = canvas.value;
    if (!target) return;
    target.width = viewport.width;
    target.height = viewport.height;
    const task = page.render({ canvas: target, viewport });
    const timeout = window.setTimeout(() => task.cancel(), 8_000);
    try {
      await task.promise;
    } finally {
      window.clearTimeout(timeout);
    }
    rendered = true;
    if (host.value) observer?.unobserve(host.value);
  } finally {
    rendering = false;
  }
}

function scheduleRender() {
  thumbnailQueue = thumbnailQueue.then(render, render);
}

onMounted(() => {
  observer = new IntersectionObserver(
    ([entry]) => entry.isIntersecting && scheduleRender(),
    { rootMargin: "40px" },
  );
  if (host.value) observer.observe(host.value);
});
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <button
    ref="host"
    class="thumbnail"
    :class="{ selected }"
    :aria-current="selected ? 'page' : undefined"
    :aria-label="`预览第 ${page} 页`"
    :data-thumbnail-page="page"
    @click="emit('select', page)"
  >
    <canvas ref="canvas" />
    <span>{{ page }}</span>
  </button>
</template>

<style scoped>
.thumbnail {
  position: relative;
  width: 100%;
  min-height: 146px;
  padding: 7px;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: var(--surface);
  color: var(--text-muted);
  cursor: pointer;
  overflow: hidden;
}
.thumbnail:hover { border-color: var(--accent); }
.thumbnail.selected { border-color: var(--accent); box-shadow: inset 0 0 0 1px var(--accent); color: var(--text); }
canvas { display: block; max-width: 100%; max-height: 130px; margin: 0 auto; background: white; box-shadow: var(--shadow-sm); }
span { position: absolute; right: 9px; bottom: 9px; z-index: 2; padding: 3px 6px; border-radius: 3px; background: rgb(112 102 127 / 62%); color: white; box-shadow: 0 2px 6px rgb(42 27 72 / 10%); backdrop-filter: blur(3px); font-size: 10px; font-variant-numeric: tabular-nums; line-height: 1.2; pointer-events: none; }
</style>
