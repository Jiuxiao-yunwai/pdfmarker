<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { TextLayer, type PDFDocumentProxy, type RenderTask } from "pdfjs-dist";

const props = defineProps<{ document?: PDFDocumentProxy; page: number }>();
const canvas = ref<HTMLCanvasElement>();
const pageSurface = ref<HTMLDivElement>();
const textLayerContainer = ref<HTMLDivElement>();
const loading = ref(false);
const renderError = ref("");
const retry = ref(0);

watch(
  () => [props.document, props.page, retry.value] as const,
  async ([document], _, onCleanup) => {
    if (!document) {
      loading.value = false;
      return;
    }
    let cancelled = false;
    let task: RenderTask | undefined;
    let textLayer: TextLayer | undefined;
    onCleanup(() => {
      cancelled = true;
      task?.cancel();
      textLayer?.cancel();
    });
    loading.value = true;
    renderError.value = "";
    try {
      await nextTick();
      const page = await document.getPage(props.page);
      if (cancelled) return;
      const target = canvas.value;
      const surface = pageSurface.value;
      const textContainer = textLayerContainer.value;
      if (!target || !surface || !textContainer) throw new Error("预览区域不可用");
      const base = page.getViewport({ scale: 1 });
      const available = Math.max(520, (surface.parentElement?.clientWidth ?? 800) - 64);
      const viewport = page.getViewport({ scale: Math.min(1.4, available / base.width) });
      surface.style.width = `${viewport.width}px`;
      surface.style.height = `${viewport.height}px`;
      surface.style.setProperty("--total-scale-factor", String(viewport.scale));
      target.width = Math.ceil(viewport.width);
      target.height = Math.ceil(viewport.height);
      task = page.render({ canvas: target, viewport });
      let timeout = 0;
      try {
        await Promise.race([
          task.promise,
          new Promise<never>((_, reject) => {
            timeout = window.setTimeout(() => {
              task?.cancel();
              reject(new Error("渲染超过 20 秒，请重试"));
            }, 20_000);
          }),
        ]);
      } finally {
        window.clearTimeout(timeout);
      }
      if (cancelled) return;
      loading.value = false;
      textContainer.replaceChildren();
      textLayer = new TextLayer({
        textContentSource: page.streamTextContent(),
        container: textContainer,
        viewport,
      });
      // A malformed text layer must not hide an otherwise usable rendered page.
      await textLayer.render().catch(() => undefined);
    } catch (error) {
      if (!cancelled && (error as Error).name !== "RenderingCancelledException") {
        renderError.value = `页面渲染失败：${String(error)}`;
      }
    } finally {
      if (!cancelled) loading.value = false;
    }
  },
  { immediate: true },
);
</script>

<template>
  <section class="preview" aria-label="PDF 页面预览" :aria-busy="loading">
    <div class="preview-heading">
      <h2>预览</h2>
      <span>PDF 第 {{ page }} 页</span>
    </div>
    <div class="paper-stage">
      <span v-if="loading" class="loading">正在渲染…</span>
      <div v-else-if="renderError" class="render-error" role="alert">
        <span>{{ renderError }}</span>
        <button type="button" @click="retry++">重试</button>
      </div>
      <div ref="pageSurface" class="page-surface">
        <canvas ref="canvas" />
        <div ref="textLayerContainer" class="text-layer" aria-label="可选择的 PDF 文本"></div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.preview { min-width: 0; overflow: hidden; background: var(--canvas); }
.preview-heading { height: 48px; padding: 0 18px; display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); background: var(--surface); }
h2 { margin: 0; font-size: 14px; }
.preview-heading span { color: var(--text-muted); font-size: 13px; font-variant-numeric: tabular-nums; }
.paper-stage { height: calc(100% - 49px); overflow: auto; padding: 32px; position: relative; text-align: center; }
.page-surface { position: relative; margin: 0 auto; background: white; box-shadow: var(--shadow-lg); line-height: 1; }
canvas { position: absolute; inset: 0; display: block; width: 100%; height: 100%; }
.text-layer { position: absolute; inset: 0; z-index: 1; overflow: clip; color-scheme: only light; line-height: 1; text-align: initial; text-size-adjust: none; forced-color-adjust: none; transform-origin: 0 0; caret-color: CanvasText; --min-font-size: 1; --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size)); --min-font-size-inv: calc(1 / var(--min-font-size)); }
.text-layer :deep(span), .text-layer :deep(br) { position: absolute; color: transparent; white-space: pre; cursor: text; transform-origin: 0 0; }
.text-layer :deep(span:not(.markedContent)) { z-index: 1; --font-height: 0; --scale-x: 1; --rotate: 0deg; font-size: calc(var(--text-scale-factor) * var(--font-height)); transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv)); }
.text-layer :deep(.markedContent) { display: contents; }
.text-layer :deep(::selection) { background: rgb(47 102 81 / 30%); }
.text-layer :deep(.endOfContent) { position: absolute; inset: 100% 0 0; display: block; user-select: none; cursor: default; }
.loading { position: absolute; top: 16px; left: 50%; z-index: 1; transform: translateX(-50%); padding: 6px 10px; border-radius: 6px; background: var(--text); color: var(--surface); font-size: 12px; }
.render-error { position: absolute; top: 16px; left: 50%; z-index: 1; display: flex; gap: 8px; align-items: center; max-width: 80%; transform: translateX(-50%); padding: 8px 10px; border: 1px solid var(--danger); border-radius: 7px; background: var(--surface); color: var(--danger); font-size: 12px; }
.render-error span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.render-error button { min-height: 32px; padding: 0 10px; border: 1px solid var(--danger); border-radius: 5px; background: var(--surface); color: var(--danger); cursor: pointer; }
</style>
