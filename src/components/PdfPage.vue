<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, onUnmounted, ref, watch } from "vue";
import { TextLayer, type PDFDocumentProxy, type PDFPageProxy, type RenderTask } from "pdfjs-dist";

const props = defineProps<{
  document?: PDFDocumentProxy;
  page: number;
  zoom: number;
  availableWidth: number;
}>();
const host = ref<HTMLElement>();
const canvas = ref<HTMLCanvasElement>();
const surface = ref<HTMLDivElement>();
const textContainer = ref<HTMLDivElement>();
const nearby = ref(false);
const loading = ref(false);
const hasBitmap = ref(false);
const errorMessage = ref("");
const retry = ref(0);
let observer: IntersectionObserver | undefined;
let cachedDocument: PDFDocumentProxy | undefined;
let cachedPage: PDFPageProxy | undefined;
let releaseTimer: number | undefined;
const MAX_RENDER_PIXELS = 18_000_000;
const RELEASE_DELAY_MS = 3_000;

function displayViewport(pdfPage: PDFPageProxy) {
  const base = pdfPage.getViewport({ scale: 1 });
  const fitScale = Math.min(1.4, props.availableWidth / base.width);
  return pdfPage.getViewport({ scale: fitScale * props.zoom });
}

function applyPageLayout(pdfPage: PDFPageProxy) {
  const viewport = displayViewport(pdfPage);
  const pageSurface = surface.value;
  if (pageSurface) {
    pageSurface.style.width = `${viewport.width}px`;
    pageSurface.style.height = `${viewport.height}px`;
    pageSurface.style.setProperty("--total-scale-factor", String(viewport.scale));
  }
  return viewport;
}

function resetPage() {
  window.clearTimeout(releaseTimer);
  cachedPage = undefined;
  cachedDocument = undefined;
  loading.value = false;
  hasBitmap.value = false;
  errorMessage.value = "";
  const target = canvas.value;
  if (target) {
    target.width = 0;
    target.height = 0;
  }
  textContainer.value?.replaceChildren();
  const pageSurface = surface.value;
  if (pageSurface) {
    pageSurface.style.removeProperty("width");
    pageSurface.style.removeProperty("height");
    pageSurface.style.removeProperty("--total-scale-factor");
  }
}

function releaseBitmap() {
  const target = canvas.value;
  if (target) {
    target.width = 0;
    target.height = 0;
  }
  textContainer.value?.replaceChildren();
  hasBitmap.value = false;
}

onMounted(() => {
  observer = new IntersectionObserver(([entry]) => {
    nearby.value = entry.isIntersecting;
  }, {
    root: host.value?.closest(".paper-stage") ?? null,
    rootMargin: "700px 0px",
  });
  if (host.value) observer.observe(host.value);
});
onBeforeUnmount(() => {
  window.clearTimeout(releaseTimer);
  observer?.disconnect();
});
onUnmounted(() => cachedPage?.cleanup());

watch(() => props.document, resetPage, { flush: "sync" });

watch(nearby, (isNearby) => {
  window.clearTimeout(releaseTimer);
  if (!isNearby) releaseTimer = window.setTimeout(releaseBitmap, RELEASE_DELAY_MS);
});

watch(
  () => [props.zoom, props.availableWidth] as const,
  () => {
    if (cachedPage) applyPageLayout(cachedPage);
  },
  { flush: "sync" },
);

watch(
  () => [props.document, nearby.value, retry.value, props.zoom, props.availableWidth] as const,
  async ([pdfDocument, isNearby], _, onCleanup) => {
    if (!pdfDocument || !isNearby) {
      loading.value = false;
      return;
    }
    let cancelled = false;
    let renderTask: RenderTask | undefined;
    let textLayer: TextLayer | undefined;
    onCleanup(() => {
      cancelled = true;
      renderTask?.cancel();
      textLayer?.cancel();
    });
    loading.value = true;
    errorMessage.value = "";
    try {
      await nextTick();
      const pdfPage = cachedDocument === pdfDocument && cachedPage
        ? cachedPage
        : await pdfDocument.getPage(props.page);
      if (cancelled) return;
      cachedDocument = pdfDocument;
      cachedPage = pdfPage;
      const target = canvas.value;
      const layer = textContainer.value;
      if (!target || !layer) throw new Error("预览区域不可用");
      const viewport = applyPageLayout(pdfPage);
      const desiredQuality = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
      const pixelLimitQuality = Math.sqrt(MAX_RENDER_PIXELS / (viewport.width * viewport.height));
      const quality = Math.max(.1, Math.min(desiredQuality, pixelLimitQuality));
      const renderViewport = pdfPage.getViewport({ scale: viewport.scale * quality });
      const nextCanvas = window.document.createElement("canvas");
      nextCanvas.width = Math.ceil(renderViewport.width);
      nextCanvas.height = Math.ceil(renderViewport.height);
      renderTask = pdfPage.render({ canvas: nextCanvas, viewport: renderViewport });
      await renderTask.promise;
      if (cancelled) return;
      target.width = nextCanvas.width;
      target.height = nextCanvas.height;
      const context = target.getContext("2d", { alpha: false });
      if (!context) throw new Error("无法创建页面画布");
      context.drawImage(nextCanvas, 0, 0);
      hasBitmap.value = true;

      try {
        layer.replaceChildren();
        textLayer = new TextLayer({ textContentSource: pdfPage.streamTextContent(), container: layer, viewport });
        await textLayer.render();
      } catch (error) {
        if ((error as Error).name !== "RenderingCancelledException") layer.replaceChildren();
      }
    } catch (error) {
      if (!cancelled && (error as Error).name !== "RenderingCancelledException") {
        if (!hasBitmap.value) errorMessage.value = `第 ${props.page} 页渲染失败：${String(error)}`;
      }
    } finally {
      if (!cancelled) loading.value = false;
    }
  },
  { immediate: true },
);
</script>

<template>
  <article ref="host" class="pdf-page" :aria-label="`PDF 第 ${page} 页`" :aria-busy="loading">
    <span v-if="loading && !hasBitmap" class="message">正在渲染…</span>
    <div v-else-if="errorMessage" class="message error" role="alert">
      <span>{{ errorMessage }}</span><button type="button" @click="retry++">重试</button>
    </div>
    <div
      ref="surface"
      class="page-surface"
      :style="{ minWidth: `${360 * zoom}px`, minHeight: `${540 * zoom}px` }"
    >
      <canvas ref="canvas" />
      <div ref="textContainer" class="text-layer" aria-label="可选择的 PDF 文本"></div>
    </div>
  </article>
</template>

<style scoped>
.pdf-page { position: relative; min-height: 260px; padding: 8px 0 24px; scroll-margin-top: 8px; text-align: center; }
.page-surface { position: relative; margin: 0 auto; background: white; box-shadow: var(--shadow-lg); line-height: 1; }
canvas { position: absolute; inset: 0; display: block; width: 100%; height: 100%; }
.text-layer { position: absolute; inset: 0; z-index: 1; overflow: clip; color-scheme: only light; line-height: 1; text-align: initial; text-size-adjust: none; forced-color-adjust: none; transform-origin: 0 0; caret-color: CanvasText; --min-font-size: 1; --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size)); --min-font-size-inv: calc(1 / var(--min-font-size)); }
.text-layer :deep(span), .text-layer :deep(br) { position: absolute; color: transparent; white-space: pre; cursor: text; transform-origin: 0 0; }
.text-layer :deep(span:not(.markedContent)) { z-index: 1; --font-height: 0; --scale-x: 1; --rotate: 0deg; font-size: calc(var(--text-scale-factor) * var(--font-height)); transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv)); }
.text-layer :deep(.markedContent) { display: contents; }
.text-layer :deep(::selection) { background: rgb(109 69 197 / 28%); }
.text-layer :deep(.endOfContent) { position: absolute; inset: 100% 0 0; display: block; user-select: none; cursor: default; }
.message { position: absolute; top: 18px; left: 50%; z-index: 2; transform: translateX(-50%); padding: 6px 10px; border-radius: 5px; background: var(--text); color: var(--surface); font-size: 12px; }
.message.error { display: flex; gap: 8px; align-items: center; max-width: 80%; border: 1px solid var(--danger); background: var(--surface); color: var(--danger); }
.message.error span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.message button { min-height: 30px; padding: 0 8px; border: 1px solid currentColor; border-radius: 5px; background: transparent; color: inherit; cursor: pointer; }
</style>
