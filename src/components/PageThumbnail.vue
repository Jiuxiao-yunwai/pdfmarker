<script lang="ts">
let thumbnailQueue = Promise.resolve();
</script>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";

const props = defineProps<{ document: PDFDocumentProxy; page: number; selected: boolean }>();
const emit = defineEmits<{ select: [page: number] }>();
const host = ref<HTMLElement>();
const canvas = ref<HTMLCanvasElement>();
let observer: IntersectionObserver | undefined;
let renderTask: RenderTask | undefined;
let rendered = false;
let rendering = false;
let disposed = false;

async function render() {
  if (disposed || rendered || rendering || !canvas.value) return;
  rendering = true;
  try {
    const page = await props.document.getPage(props.page);
    if (disposed) return;
    const viewport = page.getViewport({ scale: 0.18 });
    const target = canvas.value;
    if (!target) return;
    target.width = viewport.width;
    target.height = viewport.height;
    renderTask = page.render({ canvas: target, viewport });
    const timeout = window.setTimeout(() => renderTask?.cancel(), 8_000);
    try {
      await renderTask.promise;
    } finally {
      window.clearTimeout(timeout);
    }
    if (disposed) return;
    rendered = true;
    if (host.value) observer?.unobserve(host.value);
  } catch (error) {
    if (!disposed && (error as Error).name !== "RenderingCancelledException") rendered = false;
  } finally {
    renderTask = undefined;
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
onBeforeUnmount(() => {
  disposed = true;
  renderTask?.cancel();
  observer?.disconnect();
});
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
  border-radius: 9px;
  background: var(--surface);
  color: var(--text-muted);
  cursor: pointer;
  overflow: hidden;
}
.thumbnail:hover { border-color: #aeb9e9; box-shadow: 0 5px 14px rgb(34 50 84 / 8%); }
.thumbnail.selected { border-color: var(--accent); box-shadow: inset 0 0 0 1px var(--accent), 0 6px 16px rgb(70 88 214 / 14%); color: var(--text); }
canvas { display: block; max-width: 100%; max-height: 130px; margin: 0 auto; background: white; box-shadow: var(--shadow-sm); }
span { position: absolute; right: 9px; bottom: 9px; z-index: 2; padding: 3px 7px; border-radius: 6px; background: rgb(25 34 58 / 68%); color: white; box-shadow: 0 2px 6px rgb(25 34 58 / 12%); backdrop-filter: blur(4px); font-size: 10px; font-variant-numeric: tabular-nums; line-height: 1.2; pointer-events: none; }
</style>
