<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";

const props = defineProps<{ document?: PDFDocumentProxy; page: number }>();
const canvas = ref<HTMLCanvasElement>();
const loading = ref(false);
let task: RenderTask | undefined;

async function renderPage() {
  task?.cancel();
  if (!props.document) return;
  loading.value = true;
  await nextTick();
  const page = await props.document.getPage(props.page);
  const base = page.getViewport({ scale: 1 });
  const available = Math.max(520, (canvas.value?.parentElement?.clientWidth ?? 800) - 64);
  const viewport = page.getViewport({ scale: Math.min(1.65, available / base.width) });
  if (!canvas.value) return;
  canvas.value.width = viewport.width;
  canvas.value.height = viewport.height;
  task = page.render({ canvas: canvas.value, viewport });
  try {
    await task.promise;
  } catch (error) {
    if ((error as Error).name !== "RenderingCancelledException") throw error;
  } finally {
    loading.value = false;
  }
}

watch(() => [props.document, props.page], renderPage, { immediate: true });
</script>

<template>
  <section class="preview" aria-label="PDF 页面预览" :aria-busy="loading">
    <div class="preview-heading">
      <h2>预览</h2>
      <span>PDF 第 {{ page }} 页</span>
    </div>
    <div class="paper-stage">
      <span v-if="loading" class="loading">正在渲染…</span>
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
</style>
