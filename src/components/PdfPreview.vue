<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";

const props = defineProps<{ document?: PDFDocumentProxy; page: number }>();
const canvas = ref<HTMLCanvasElement>();
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
    onCleanup(() => {
      cancelled = true;
      task?.cancel();
    });
    loading.value = true;
    renderError.value = "";
    try {
      await nextTick();
      const page = await document.getPage(props.page);
      if (cancelled) return;
      const target = canvas.value;
      if (!target) throw new Error("预览画布不可用");
      const base = page.getViewport({ scale: 1 });
      const available = Math.max(520, (target.parentElement?.clientWidth ?? 800) - 64);
      const viewport = page.getViewport({ scale: Math.min(1.4, available / base.width) });
      target.width = viewport.width;
      target.height = viewport.height;
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
      <canvas ref="canvas" />
    </div>
  </section>
</template>

<style scoped>
.preview { min-width: 0; overflow: hidden; background: var(--canvas); }
.preview-heading { height: 48px; padding: 0 18px; display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); background: var(--surface); }
h2 { margin: 0; font-size: 14px; }
.preview-heading span { color: var(--text-muted); font-size: 13px; font-variant-numeric: tabular-nums; }
.paper-stage { height: calc(100% - 49px); overflow: auto; padding: 32px; position: relative; text-align: center; }
canvas { max-width: 100%; height: auto; background: white; box-shadow: var(--shadow-lg); }
.loading { position: absolute; top: 16px; left: 50%; z-index: 1; transform: translateX(-50%); padding: 6px 10px; border-radius: 6px; background: var(--text); color: var(--surface); font-size: 12px; }
.render-error { position: absolute; top: 16px; left: 50%; z-index: 1; display: flex; gap: 8px; align-items: center; max-width: 80%; transform: translateX(-50%); padding: 8px 10px; border: 1px solid var(--danger); border-radius: 7px; background: var(--surface); color: var(--danger); font-size: 12px; }
.render-error span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.render-error button { min-height: 32px; padding: 0 10px; border: 1px solid var(--danger); border-radius: 5px; background: var(--surface); color: var(--danger); cursor: pointer; }
</style>
