<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { TextLayer, type PDFDocumentProxy, type RenderTask } from "pdfjs-dist";

const props = defineProps<{ document?: PDFDocumentProxy; page: number }>();
const host = ref<HTMLElement>();
const canvas = ref<HTMLCanvasElement>();
const surface = ref<HTMLDivElement>();
const textContainer = ref<HTMLDivElement>();
const active = ref(false);
const loading = ref(false);
const errorMessage = ref("");
const retry = ref(0);
let observer: IntersectionObserver | undefined;

onMounted(() => {
  observer = new IntersectionObserver(([entry]) => {
    if (!entry.isIntersecting) return;
    active.value = true;
    observer?.disconnect();
  }, { rootMargin: "700px 0px" });
  if (host.value) observer.observe(host.value);
});
onBeforeUnmount(() => observer?.disconnect());

watch(
  () => [props.document, active.value, retry.value] as const,
  async ([document, isActive], _, onCleanup) => {
    if (!document || !isActive) return;
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
      const pdfPage = await document.getPage(props.page);
      if (cancelled) return;
      const target = canvas.value;
      const pageSurface = surface.value;
      const layer = textContainer.value;
      if (!target || !pageSurface || !layer) throw new Error("预览区域不可用");
      const base = pdfPage.getViewport({ scale: 1 });
      const available = Math.max(360, (host.value?.parentElement?.clientWidth ?? 760) - 64);
      const viewport = pdfPage.getViewport({ scale: Math.min(1.4, available / base.width) });
      pageSurface.style.width = `${viewport.width}px`;
      pageSurface.style.height = `${viewport.height}px`;
      pageSurface.style.setProperty("--total-scale-factor", String(viewport.scale));
      target.width = Math.ceil(viewport.width);
      target.height = Math.ceil(viewport.height);
      renderTask = pdfPage.render({ canvas: target, viewport });
      await renderTask.promise;
      if (cancelled) return;
      layer.replaceChildren();
      textLayer = new TextLayer({ textContentSource: pdfPage.streamTextContent(), container: layer, viewport });
      await textLayer.render().catch(() => undefined);
    } catch (error) {
      if (!cancelled && (error as Error).name !== "RenderingCancelledException") {
        errorMessage.value = `第 ${props.page} 页渲染失败：${String(error)}`;
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
    <div class="page-label">第 {{ page }} 页</div>
    <span v-if="loading" class="message">正在渲染…</span>
    <div v-else-if="errorMessage" class="message error" role="alert">
      <span>{{ errorMessage }}</span><button type="button" @click="retry++">重试</button>
    </div>
    <div ref="surface" class="page-surface">
      <canvas ref="canvas" />
      <div ref="textContainer" class="text-layer" aria-label="可选择的 PDF 文本"></div>
    </div>
  </article>
</template>

<style scoped>
.pdf-page { position: relative; min-height: 680px; padding: 12px 0 28px; scroll-margin-top: 12px; text-align: center; }
.page-label { margin-bottom: 8px; color: var(--text-muted); font-size: 12px; font-variant-numeric: tabular-nums; }
.page-surface { position: relative; min-width: 360px; min-height: 540px; margin: 0 auto; background: white; box-shadow: var(--shadow-lg); line-height: 1; }
canvas { position: absolute; inset: 0; display: block; width: 100%; height: 100%; }
.text-layer { position: absolute; inset: 0; z-index: 1; overflow: clip; color-scheme: only light; line-height: 1; text-align: initial; text-size-adjust: none; forced-color-adjust: none; transform-origin: 0 0; caret-color: CanvasText; --min-font-size: 1; --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size)); --min-font-size-inv: calc(1 / var(--min-font-size)); }
.text-layer :deep(span), .text-layer :deep(br) { position: absolute; color: transparent; white-space: pre; cursor: text; transform-origin: 0 0; }
.text-layer :deep(span:not(.markedContent)) { z-index: 1; --font-height: 0; --scale-x: 1; --rotate: 0deg; font-size: calc(var(--text-scale-factor) * var(--font-height)); transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv)); }
.text-layer :deep(.markedContent) { display: contents; }
.text-layer :deep(::selection) { background: rgb(47 102 81 / 30%); }
.text-layer :deep(.endOfContent) { position: absolute; inset: 100% 0 0; display: block; user-select: none; cursor: default; }
.message { position: absolute; top: 42px; left: 50%; z-index: 2; transform: translateX(-50%); padding: 6px 10px; border-radius: 6px; background: var(--text); color: var(--surface); font-size: 12px; }
.message.error { display: flex; gap: 8px; align-items: center; max-width: 80%; border: 1px solid var(--danger); background: var(--surface); color: var(--danger); }
.message.error span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.message button { min-height: 30px; padding: 0 8px; border: 1px solid currentColor; border-radius: 5px; background: transparent; color: inherit; cursor: pointer; }
</style>
