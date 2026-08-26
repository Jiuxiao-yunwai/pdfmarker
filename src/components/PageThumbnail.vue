<script lang="ts">
interface ThumbnailRenderJob {
  cancelled: boolean;
  started: boolean;
  run: () => Promise<void>;
}

const pendingThumbnailJobs: ThumbnailRenderJob[] = [];
let activeThumbnailJobs = 0;
const MAX_ACTIVE_THUMBNAILS = 2;

function runThumbnailJobs() {
  while (activeThumbnailJobs < MAX_ACTIVE_THUMBNAILS) {
    const job = pendingThumbnailJobs.shift();
    if (!job) return;
    if (job.cancelled) continue;
    job.started = true;
    activeThumbnailJobs += 1;
    void job.run()
      .catch(() => undefined)
      .finally(() => {
        activeThumbnailJobs -= 1;
        runThumbnailJobs();
      });
  }
}

function queueThumbnailRender(run: () => Promise<void>) {
  const job: ThumbnailRenderJob = { cancelled: false, started: false, run };
  pendingThumbnailJobs.push(job);
  queueMicrotask(runThumbnailJobs);
  return () => {
    job.cancelled = true;
    if (job.started) return;
    const index = pendingThumbnailJobs.indexOf(job);
    if (index >= 0) pendingThumbnailJobs.splice(index, 1);
  };
}
</script>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";

const props = defineProps<{ document: PDFDocumentProxy; page: number; selected: boolean }>();
const emit = defineEmits<{ select: [page: number] }>();
const host = ref<HTMLElement>();
const canvasHost = ref<HTMLDivElement>();
const loading = ref(false);
let observer: IntersectionObserver | undefined;
let renderTask: RenderTask | undefined;
let currentCanvas: HTMLCanvasElement | undefined;
let cancelQueuedRender: (() => void) | undefined;
let retryTimer: number | undefined;
let rendered = false;
let rendering = false;
let rerenderRequested = false;
let disposed = false;
let intersecting = false;
let retryAttempts = 0;
const RENDER_TIMEOUT_MS = 8_000;
const RETRY_DELAY_MS = 600;
const MAX_AUTO_RETRIES = 2;

function disposeCanvas(target?: HTMLCanvasElement) {
  if (!target) return;
  target.remove();
  target.width = 0;
  target.height = 0;
}

function handleContextLoss(event: Event) {
  event.preventDefault();
  if (event.currentTarget !== currentCanvas || disposed) return;
  disposeCanvas(currentCanvas);
  currentCanvas = undefined;
  rendered = false;
  if (intersecting) scheduleRender();
}

async function render() {
  if (disposed || rendered || rendering || !canvasHost.value) return;
  rendering = true;
  loading.value = true;
  let nextCanvas: HTMLCanvasElement | undefined;
  try {
    const page = await props.document.getPage(props.page);
    if (disposed) return;
    const viewport = page.getViewport({ scale: 0.18 });
    nextCanvas = window.document.createElement("canvas");
    nextCanvas.width = Math.max(1, Math.ceil(viewport.width));
    nextCanvas.height = Math.max(1, Math.ceil(viewport.height));
    const context = nextCanvas.getContext("2d", { alpha: false });
    if (!context) throw new Error("无法创建缩略图画布");
    renderTask = page.render({ canvas: nextCanvas, viewport, background: "#ffffff" });
    let timedOut = false;
    const timeout = window.setTimeout(() => {
      timedOut = true;
      renderTask?.cancel();
    }, RENDER_TIMEOUT_MS);
    try {
      await renderTask.promise;
    } catch (error) {
      if (timedOut) throw new Error("缩略图渲染超时");
      throw error;
    } finally {
      window.clearTimeout(timeout);
    }
    if (disposed || !canvasHost.value) return;
    if (context.isContextLost?.()) throw new Error("缩略图画布上下文已丢失");
    nextCanvas.addEventListener("contextlost", handleContextLoss, { once: true });
    const previousCanvas = currentCanvas;
    canvasHost.value.replaceChildren(nextCanvas);
    currentCanvas = nextCanvas;
    nextCanvas = undefined;
    disposeCanvas(previousCanvas);
    rendered = true;
    retryAttempts = 0;
    if (host.value) observer?.unobserve(host.value);
  } catch (error) {
    rendered = false;
    if (!disposed && intersecting && (error as Error).name !== "RenderingCancelledException" && retryAttempts < MAX_AUTO_RETRIES) {
      retryAttempts += 1;
      window.clearTimeout(retryTimer);
      retryTimer = window.setTimeout(scheduleRender, RETRY_DELAY_MS * retryAttempts);
    }
  } finally {
    disposeCanvas(nextCanvas);
    renderTask = undefined;
    rendering = false;
    loading.value = false;
  }
}

function scheduleRender() {
  if (disposed || rendered) return;
  if (rendering) {
    rerenderRequested = true;
    return;
  }
  if (cancelQueuedRender) return;
  cancelQueuedRender = queueThumbnailRender(async () => {
    cancelQueuedRender = undefined;
    rerenderRequested = false;
    await render();
    const shouldRenderAgain = rerenderRequested && intersecting && !disposed && !rendered;
    rerenderRequested = false;
    if (shouldRenderAgain) scheduleRender();
  });
}

onMounted(() => {
  observer = new IntersectionObserver(
    ([entry]) => {
      if (entry.isIntersecting && !intersecting) retryAttempts = 0;
      intersecting = entry.isIntersecting;
      if (intersecting) scheduleRender();
      else if (!rendered) {
        cancelQueuedRender?.();
        cancelQueuedRender = undefined;
        renderTask?.cancel();
      }
    },
    { root: host.value?.closest(".thumbnail-list") ?? null, rootMargin: "240px 0px" },
  );
  if (host.value) observer.observe(host.value);
});
onBeforeUnmount(() => {
  disposed = true;
  window.clearTimeout(retryTimer);
  cancelQueuedRender?.();
  renderTask?.cancel();
  observer?.disconnect();
  disposeCanvas(currentCanvas);
  currentCanvas = undefined;
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
    <div ref="canvasHost" class="thumbnail-canvas" :class="{ loading }"></div>
    <span class="page-number">{{ page }}</span>
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
.thumbnail-canvas { width: 100%; height: 130px; display: grid; place-items: center; background: white; box-shadow: var(--shadow-sm); }
.thumbnail-canvas.loading::after { content: ""; width: 16px; height: 16px; border: 2px solid rgb(109 69 197 / 16%); border-top-color: var(--accent); border-radius: 50%; animation: thumbnail-spin .8s linear infinite; }
.thumbnail-canvas :deep(canvas) { display: block; max-width: 100%; max-height: 130px; margin: 0 auto; background: white; }
.page-number { position: absolute; right: 9px; bottom: 9px; z-index: 2; padding: 3px 6px; border-radius: 3px; background: rgb(112 102 127 / 62%); color: white; box-shadow: 0 2px 6px rgb(42 27 72 / 10%); backdrop-filter: blur(3px); font-size: 10px; font-variant-numeric: tabular-nums; line-height: 1.2; pointer-events: none; }
@keyframes thumbnail-spin { to { transform: rotate(360deg); } }
</style>
