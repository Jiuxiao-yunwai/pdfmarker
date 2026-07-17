<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import type { PDFDocumentProxy } from "pdfjs-dist";
import PdfPage from "./PdfPage.vue";

const props = defineProps<{ document?: PDFDocumentProxy; pageCount: number; currentPage: number }>();
const emit = defineEmits<{ select: [page: number] }>();
const stage = ref<HTMLElement>();
let visiblePage = 0;
let frame = 0;

function trackPage() {
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(() => {
    const container = stage.value;
    if (!container) return;
    const top = container.getBoundingClientRect().top + 12;
    let nearest = visiblePage || 1;
    let distance = Number.POSITIVE_INFINITY;
    for (const element of container.querySelectorAll<HTMLElement>("[data-pdf-page]")) {
      const currentDistance = Math.abs(element.getBoundingClientRect().top - top);
      if (currentDistance < distance) {
        nearest = Number(element.dataset.pdfPage);
        distance = currentDistance;
      }
    }
    if (nearest !== visiblePage) {
      visiblePage = nearest;
      emit("select", nearest);
    }
  });
}

watch(() => props.currentPage, async (page) => {
  if (!page || page === visiblePage) return;
  visiblePage = page;
  await nextTick();
  stage.value?.querySelector<HTMLElement>(`[data-pdf-page="${page}"]`)?.scrollIntoView({ block: "start" });
}, { immediate: true });

watch(() => props.document, () => {
  visiblePage = 1;
  if (stage.value) stage.value.scrollTop = 0;
});

onBeforeUnmount(() => cancelAnimationFrame(frame));
</script>

<template>
  <section class="preview" aria-label="PDF 连续页面预览">
    <div class="preview-heading">
      <h2>连续预览</h2>
      <span>PDF 第 {{ currentPage }} / {{ pageCount }} 页</span>
    </div>
    <div ref="stage" class="paper-stage" @scroll.passive="trackPage">
      <div v-for="page in pageCount" :key="page" class="page-anchor" :data-pdf-page="page">
        <PdfPage :document="document" :page="page" />
      </div>
    </div>
  </section>
</template>

<style scoped>
.preview { min-width: 0; overflow: hidden; background: var(--canvas); }
.preview-heading { height: 48px; padding: 0 18px; display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); background: var(--surface); }
h2 { margin: 0; font-size: 14px; }
.preview-heading span { color: var(--text-muted); font-size: 13px; font-variant-numeric: tabular-nums; }
.paper-stage { height: calc(100% - 49px); overflow: auto; padding: 20px 32px 32px; }
.page-anchor + .page-anchor { border-top: 1px solid var(--border); }
</style>
