<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { AnnotationType, type PDFDocumentProxy, type PDFPageProxy, type RenderTask } from "pdfjs-dist";
import { TextLayerBuilder } from "pdfjs-dist/web/pdf_viewer.mjs";
import { queuePdfRender, type QueuedPdfRender } from "../lib/pdfRenderQueue";

const props = defineProps<{
  document?: PDFDocumentProxy;
  page: number;
  zoom: number;
  availableWidth: number;
  renderPriority: number;
}>();
const emit = defineEmits<{ navigate: [page: number] }>();
const host = ref<HTMLElement>();
const canvasHost = ref<HTMLDivElement>();
const surface = ref<HTMLDivElement>();
const textContainer = ref<HTMLDivElement>();
const linkContainer = ref<HTMLDivElement>();
const nearby = ref(false);
const loading = ref(false);
const hasBitmap = ref(false);
const errorMessage = ref("");
const retry = ref(0);
let observer: IntersectionObserver | undefined;
let cachedDocument: PDFDocumentProxy | undefined;
let cachedPage: PDFPageProxy | undefined;
let currentCanvas: HTMLCanvasElement | undefined;
let queuedPageRender: QueuedPdfRender | undefined;
let releaseTimer: number | undefined;
const MAX_RENDER_PIXELS = 10_000_000;
const MAX_RENDER_DIMENSION = 8_192;
const RELEASE_DELAY_MS = 3_000;
const RENDER_TIMEOUT_MS = 15_000;

type PdfLinkAnnotation = {
  action?: string;
  annotationType?: number;
  dest?: string | unknown[];
  rect?: number[];
  subtype?: string;
  title?: string;
  unsafeUrl?: string;
  url?: string;
};

function safeExternalUrl(annotation: PdfLinkAnnotation) {
  const candidate = annotation.url ?? annotation.unsafeUrl;
  if (!candidate) return undefined;
  try {
    const parsed = new URL(candidate);
    return ["http:", "https:"].includes(parsed.protocol) ? parsed.href : undefined;
  } catch {
    return undefined;
  }
}

async function destinationPage(destination: string | unknown[]) {
  const pdfDocument = props.document;
  if (!pdfDocument) return undefined;
  const explicit = typeof destination === "string"
    ? await pdfDocument.getDestination(destination)
    : destination;
  if (!Array.isArray(explicit) || explicit.length === 0) return undefined;
  const reference = explicit[0];
  const pageIndex = typeof reference === "number"
    ? reference
    : await pdfDocument.getPageIndex(reference as Parameters<PDFDocumentProxy["getPageIndex"]>[0]);
  return Number.isInteger(pageIndex) ? pageIndex + 1 : undefined;
}

function namedActionPage(action?: string) {
  switch (action) {
    case "FirstPage": return 1;
    case "LastPage": return props.document?.numPages;
    case "NextPage": return Math.min(props.document?.numPages ?? props.page, props.page + 1);
    case "PrevPage": return Math.max(1, props.page - 1);
    default: return undefined;
  }
}

async function activatePdfLink(annotation: PdfLinkAnnotation) {
  try {
    const externalUrl = safeExternalUrl(annotation);
    if (externalUrl) {
      if (isTauri()) await invoke("open_external_url", { url: externalUrl });
      else window.open(externalUrl, "_blank", "noopener,noreferrer");
      return;
    }
    const targetPage = annotation.dest
      ? await destinationPage(annotation.dest)
      : namedActionPage(annotation.action);
    if (targetPage) emit("navigate", targetPage);
  } catch (error) {
    console.error("无法打开 PDF 链接", error);
  }
}

async function renderLinkLayer(pdfPage: PDFPageProxy, viewport: ReturnType<PDFPageProxy["getViewport"]>) {
  const container = linkContainer.value;
  if (!container) return;
  const annotations = await pdfPage.getAnnotations({ intent: "display" }) as PdfLinkAnnotation[];
  const links = annotations.filter((annotation) =>
    (annotation.annotationType === AnnotationType.LINK || annotation.subtype === "Link") && annotation.rect?.length === 4 &&
    (safeExternalUrl(annotation) || annotation.dest || namedActionPage(annotation.action)),
  );
  const fragment = document.createDocumentFragment();
  for (const annotation of links) {
    const converted = viewport.convertToViewportRectangle(annotation.rect!);
    const left = Math.min(converted[0], converted[2]);
    const top = Math.min(converted[1], converted[3]);
    const width = Math.abs(converted[2] - converted[0]);
    const height = Math.abs(converted[3] - converted[1]);
    if (![left, top, width, height].every(Number.isFinite) || width < 1 || height < 1) continue;
    const anchor = document.createElement("a");
    anchor.className = "pdf-link";
    anchor.href = safeExternalUrl(annotation) ?? "#";
    anchor.ariaLabel = annotation.title?.trim() || "打开 PDF 链接";
    anchor.title = annotation.title?.trim() || (safeExternalUrl(annotation) ? "在默认浏览器中打开链接" : "跳转到 PDF 目标页");
    anchor.style.left = `${left}px`;
    anchor.style.top = `${top}px`;
    anchor.style.width = `${width}px`;
    anchor.style.height = `${height}px`;
    anchor.addEventListener("click", (event) => {
      event.preventDefault();
      void activatePdfLink(annotation);
    });
    fragment.append(anchor);
  }
  container.replaceChildren(fragment);
}

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
  errorMessage.value = "";
  releaseBitmap();
  textContainer.value?.replaceChildren();
  const pageSurface = surface.value;
  if (pageSurface) {
    pageSurface.style.removeProperty("width");
    pageSurface.style.removeProperty("height");
    pageSurface.style.removeProperty("--total-scale-factor");
  }
}

function disposeCanvas(target?: HTMLCanvasElement) {
  if (!target) return;
  target.remove();
  target.width = 0;
  target.height = 0;
}

function releaseBitmap() {
  canvasHost.value?.replaceChildren();
  disposeCanvas(currentCanvas);
  currentCanvas = undefined;
  textContainer.value?.replaceChildren();
  linkContainer.value?.replaceChildren();
  hasBitmap.value = false;
}

function renderQualityLevels(viewport: ReturnType<PDFPageProxy["getViewport"]>) {
  const desiredQuality = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
  const pixelQuality = Math.sqrt(MAX_RENDER_PIXELS / Math.max(1, viewport.width * viewport.height));
  const dimensionQuality = Math.min(
    MAX_RENDER_DIMENSION / Math.max(1, viewport.width),
    MAX_RENDER_DIMENSION / Math.max(1, viewport.height),
  );
  const preferred = Math.max(.1, Math.min(desiredQuality, pixelQuality, dimensionQuality));
  return [preferred, preferred * .72, preferred * .5]
    .map((quality) => Math.max(.1, quality))
    .filter((quality, index, levels) => index === 0 || Math.abs(quality - levels[index - 1]) > .01);
}

async function renderPageCanvas(
  pdfPage: PDFPageProxy,
  viewport: ReturnType<PDFPageProxy["getViewport"]>,
  isCancelled: () => boolean,
  setRenderTask: (task?: RenderTask) => void,
) {
  let lastError: unknown;
  for (const quality of renderQualityLevels(viewport)) {
    if (isCancelled()) return;
    const renderViewport = pdfPage.getViewport({ scale: viewport.scale * quality });
    const nextCanvas = window.document.createElement("canvas");
    nextCanvas.width = Math.max(1, Math.ceil(renderViewport.width));
    nextCanvas.height = Math.max(1, Math.ceil(renderViewport.height));
    try {
      const context = nextCanvas.getContext("2d", { alpha: false });
      if (!context) throw new Error("无法创建页面画布");
      const task = pdfPage.render({ canvas: nextCanvas, viewport: renderViewport, background: "#ffffff" });
      setRenderTask(task);
      let timedOut = false;
      const timeout = window.setTimeout(() => {
        timedOut = true;
        task.cancel();
      }, RENDER_TIMEOUT_MS);
      try {
        await task.promise;
      } catch (error) {
        if (timedOut) throw new Error("页面渲染超时");
        throw error;
      } finally {
        window.clearTimeout(timeout);
      }
      setRenderTask(undefined);
      if (isCancelled()) {
        disposeCanvas(nextCanvas);
        return;
      }
      if (context.isContextLost?.()) throw new Error("页面画布上下文已丢失");
      return nextCanvas;
    } catch (error) {
      setRenderTask(undefined);
      disposeCanvas(nextCanvas);
      if (isCancelled() || (error as Error).name === "RenderingCancelledException") throw error;
      lastError = error;
    }
  }
  throw lastError ?? new Error("页面画布渲染失败");
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
onUnmounted(() => {
  releaseBitmap();
  cachedPage?.cleanup();
});

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
  () => props.renderPriority,
  (priority) => queuedPageRender?.reprioritize(priority),
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
    let queuedRender: QueuedPdfRender | undefined;
    let renderedCanvas: HTMLCanvasElement | undefined;
    let textLayer: TextLayerBuilder | undefined;
    onCleanup(() => {
      cancelled = true;
      queuedRender?.cancel();
      renderTask?.cancel();
      textLayer?.cancel();
      if (renderedCanvas && renderedCanvas !== currentCanvas) disposeCanvas(renderedCanvas);
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
      const targetHost = canvasHost.value;
      const layer = textContainer.value;
      const links = linkContainer.value;
      if (!targetHost || !layer || !links) throw new Error("预览区域不可用");
      links.replaceChildren();
      const viewport = applyPageLayout(pdfPage);
      queuedRender = queuePdfRender(async () => {
        renderedCanvas = await renderPageCanvas(
          pdfPage,
          viewport,
          () => cancelled,
          (task) => { renderTask = task; },
        );
      }, props.renderPriority);
      queuedPageRender = queuedRender;
      const completed = await queuedRender.promise;
      if (!completed || cancelled || !renderedCanvas) return;
      renderedCanvas.className = "page-bitmap";
      const previousCanvas = currentCanvas;
      targetHost.replaceChildren(renderedCanvas);
      currentCanvas = renderedCanvas;
      disposeCanvas(previousCanvas);
      hasBitmap.value = true;

      try {
        layer.replaceChildren();
        textLayer = new TextLayerBuilder({ pdfPage });
        layer.replaceChildren(textLayer.div);
        await textLayer.render({ viewport, images: undefined as never });
      } catch (error) {
        if ((error as Error).name !== "RenderingCancelledException") {
          textLayer?.cancel();
          layer.replaceChildren();
        }
      }
      try {
        if (!cancelled) await renderLinkLayer(pdfPage, viewport);
      } catch (error) {
        if (!cancelled) links.replaceChildren();
      }
    } catch (error) {
      if (!cancelled && (error as Error).name !== "RenderingCancelledException") {
        errorMessage.value = `第 ${props.page} 页渲染失败：${String(error)}`;
      }
    } finally {
      if (queuedPageRender === queuedRender) queuedPageRender = undefined;
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
      <div ref="canvasHost" class="canvas-layer"></div>
      <div ref="textContainer" class="text-layer-host" aria-label="可选择的 PDF 文本"></div>
      <div ref="linkContainer" class="link-layer" aria-label="PDF 链接"></div>
    </div>
  </article>
</template>

<style scoped>
.pdf-page { position: relative; min-height: 260px; padding: 8px 0 24px; scroll-margin-top: 8px; text-align: center; }
.page-surface { position: relative; margin: 0 auto; background: white; box-shadow: var(--shadow-lg); line-height: 1; }
.canvas-layer { position: absolute; inset: 0; background: white; }
.canvas-layer :deep(canvas) { display: block; width: 100%; height: 100%; }
.text-layer-host { position: absolute; inset: 0; z-index: 1; overflow: clip; }
.text-layer-host :deep(.textLayer) { position: absolute; inset: 0; z-index: 0; overflow: clip; color-scheme: only light; opacity: 1; line-height: 1; text-align: initial; text-size-adjust: none; forced-color-adjust: none; transform-origin: 0 0; caret-color: CanvasText; --min-font-size: 1; --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size)); --min-font-size-inv: calc(1 / var(--min-font-size)); }
.text-layer-host :deep(.textLayer :is(span, br)) { position: absolute; color: transparent; white-space: pre; cursor: text; transform-origin: 0 0; }
.text-layer-host :deep(.textLayer > :not(.markedContent)), .text-layer-host :deep(.textLayer .markedContent span:not(.markedContent)) { z-index: 1; --font-height: 0; --scale-x: 1; --rotate: 0deg; font-size: calc(var(--text-scale-factor) * var(--font-height)); transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv)); }
.text-layer-host :deep(.textLayer .markedContent) { display: contents; }
.text-layer-host :deep(.textLayer span[role="img"]) { user-select: none; cursor: default; }
.text-layer-host :deep(.textLayer ::selection) { background: rgb(109 69 197 / 24%); }
.text-layer-host :deep(.textLayer br::selection) { background: transparent; }
.text-layer-host :deep(.textLayer .endOfContent) { position: absolute; inset: 100% 0 0; z-index: 0; display: block; user-select: none; cursor: default; }
.text-layer-host :deep(.textLayer.selecting .endOfContent) { top: 0; }
.link-layer { position: absolute; inset: 0; z-index: 2; overflow: clip; pointer-events: none; }
.link-layer :deep(.pdf-link) { position: absolute; pointer-events: auto; cursor: pointer; border-radius: 2px; background: rgb(109 69 197 / 0%); outline-offset: 1px; transition: background 120ms ease, box-shadow 120ms ease; }
.link-layer :deep(.pdf-link:hover) { background: rgb(109 69 197 / 12%); box-shadow: inset 0 0 0 1px rgb(109 69 197 / 28%); }
.link-layer :deep(.pdf-link:focus-visible) { background: rgb(109 69 197 / 14%); box-shadow: inset 0 0 0 2px rgb(109 69 197 / 52%); outline: 0; }
.message { position: absolute; top: 18px; left: 50%; z-index: 2; transform: translateX(-50%); padding: 6px 10px; border-radius: 5px; background: var(--text); color: var(--surface); font-size: 12px; }
.message.error { display: flex; gap: 8px; align-items: center; max-width: 80%; border: 1px solid var(--danger); background: var(--surface); color: var(--danger); }
.message.error span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.message button { min-height: 30px; padding: 0 8px; border: 1px solid currentColor; border-radius: 5px; background: transparent; color: inherit; cursor: pointer; }
</style>
