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
    @click="emit('select', page)"
  >
    <canvas ref="canvas" />
    <span>第 {{ page }} 页</span>
  </button>
</template>

<style scoped>
.thumbnail {
  width: 100%;
  min-height: 156px;
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--surface);
  color: var(--text-muted);
  cursor: pointer;
}
.thumbnail:hover { border-color: var(--accent); }
.thumbnail.selected { border: 2px solid var(--accent); color: var(--text); }
canvas { display: block; max-width: 100%; max-height: 124px; margin: 0 auto 6px; background: white; box-shadow: var(--shadow-sm); }
span { font-size: 12px; font-variant-numeric: tabular-nums; }
</style>
