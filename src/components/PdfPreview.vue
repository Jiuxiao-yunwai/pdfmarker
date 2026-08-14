<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { PDFDocumentProxy } from "pdfjs-dist";
import PdfPage from "./PdfPage.vue";

const props = defineProps<{ document?: PDFDocumentProxy; pageCount: number; currentPage: number }>();
const emit = defineEmits<{ select: [page: number] }>();
const stage = ref<HTMLElement>();
const pageEditor = ref<HTMLInputElement>();
const zoom = ref(1);
const editingPage = ref(false);
const pageDraft = ref(1);
let visiblePage = 0;
let frame = 0;
let wheelFrame: number | undefined;
let pendingWheelSteps = 0;
let pendingWheelAnchor: { x: number; y: number } | undefined;

function scrollToPage(page: number) {
  const container = stage.value;
  const target = container?.querySelector<HTMLElement>(`[data-pdf-page="${page}"]`);
  if (!container || !target) return;
  const containerTop = container.getBoundingClientRect().top;
  const targetTop = target.getBoundingClientRect().top;
  container.scrollTop = Math.max(0, container.scrollTop + targetTop - containerTop - 8);
}

function setZoom(value: number, anchor?: { x: number; y: number }) {
  const next = Math.min(2.4, Math.max(.6, Math.round(value * 10) / 10));
  if (next === zoom.value) return;
  const container = stage.value;
  if (!container) {
    zoom.value = next;
    return;
  }
  const containerRect = container.getBoundingClientRect();
  const point = anchor ?? {
    x: containerRect.left + containerRect.width / 2,
    y: containerRect.top + containerRect.height / 2,
  };
  const hovered = document.elementFromPoint(point.x, point.y)?.closest<HTMLElement>("[data-pdf-page]");
  const target = hovered && container.contains(hovered)
    ? hovered
    : container.querySelector<HTMLElement>(`[data-pdf-page="${props.currentPage}"]`);
  const before = target?.getBoundingClientRect();
  const relativeX = before?.width ? (point.x - before.left) / before.width : .5;
  const relativeY = before?.height ? (point.y - before.top) / before.height : .5;
  zoom.value = next;
  void nextTick(() => window.requestAnimationFrame(() => {
    if (!target || !before) return;
    const after = target.getBoundingClientRect();
    container.scrollLeft += after.left + after.width * relativeX - point.x;
    container.scrollTop += after.top + after.height * relativeY - point.y;
  }));
}

function handleZoomWheel(event: WheelEvent) {
  if (!event.ctrlKey) return;
  event.preventDefault();
  pendingWheelSteps += event.deltaY < 0 ? 1 : -1;
  pendingWheelSteps = Math.max(-3, Math.min(3, pendingWheelSteps));
  pendingWheelAnchor = { x: event.clientX, y: event.clientY };
  if (wheelFrame !== undefined) return;
  wheelFrame = window.requestAnimationFrame(() => {
    wheelFrame = undefined;
    const steps = pendingWheelSteps;
    const wheelAnchor = pendingWheelAnchor;
    pendingWheelSteps = 0;
    pendingWheelAnchor = undefined;
    setZoom(zoom.value + steps * .1, wheelAnchor);
  });
}

function handleZoomShortcut(event: KeyboardEvent) {
  if (!event.ctrlKey) return;
  if (["+", "=", "Add"].includes(event.key)) {
    event.preventDefault();
    setZoom(zoom.value + .1);
  } else if (["-", "Subtract"].includes(event.key)) {
    event.preventDefault();
    setZoom(zoom.value - .1);
  } else if (event.key === "0") {
    event.preventDefault();
    setZoom(1);
  }
}

async function beginPageEdit() {
  pageDraft.value = props.currentPage;
  editingPage.value = true;
  await nextTick();
  pageEditor.value?.focus();
  pageEditor.value?.select();
}

function commitPageEdit() {
  if (!editingPage.value) return;
  const page = Math.min(props.pageCount, Math.max(1, Math.floor(Number(pageDraft.value) || props.currentPage)));
  editingPage.value = false;
  if (page !== props.currentPage) emit("select", page);
}

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
  scrollToPage(page);
}, { immediate: true });

watch(() => props.document, () => {
  visiblePage = 1;
  if (stage.value) stage.value.scrollTop = 0;
});

onMounted(() => window.addEventListener("keydown", handleZoomShortcut));
onBeforeUnmount(() => {
  cancelAnimationFrame(frame);
  if (wheelFrame !== undefined) window.cancelAnimationFrame(wheelFrame);
  window.removeEventListener("keydown", handleZoomShortcut);
});
</script>

<template>
  <section class="preview" aria-label="PDF 连续页面预览">
    <div class="reader-toolbar" aria-label="PDF 阅读工具">
      <div class="page-indicator" aria-live="polite">
        <input
          v-if="editingPage"
          ref="pageEditor"
          v-model.number="pageDraft"
          type="number"
          min="1"
          :max="pageCount"
          aria-label="输入要跳转的 PDF 页码"
          @blur="commitPageEdit"
          @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
          @keydown.esc.prevent="editingPage = false"
        />
        <button v-else type="button" title="输入页码跳转" @click="beginPageEdit">{{ currentPage }}</button>
        <span>/ {{ pageCount }}</span>
      </div>
      <span class="tool-divider" aria-hidden="true"></span>
      <button type="button" aria-label="缩小 PDF" title="缩小（Ctrl+-）" :disabled="zoom <= .6" @click="setZoom(zoom - .1)">−</button>
      <button type="button" class="zoom-value" title="恢复适合宽度（Ctrl+0）" @click="setZoom(1)">{{ Math.round(zoom * 100) }}%</button>
      <button type="button" aria-label="放大 PDF" title="放大（Ctrl++）" :disabled="zoom >= 2.4" @click="setZoom(zoom + .1)">＋</button>
    </div>
    <div ref="stage" class="paper-stage" @scroll.passive="trackPage" @wheel="handleZoomWheel">
      <div class="page-stack" :style="{ zoom }">
        <div v-for="page in pageCount" :key="page" class="page-anchor" :data-pdf-page="page">
          <PdfPage :document="document" :page="page" />
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.preview { position: relative; min-width: 0; overflow: hidden; background: var(--canvas); }
.reader-toolbar { position: absolute; z-index: 5; top: 12px; right: 16px; height: 31px; display: flex; gap: 2px; align-items: center; padding: 3px; border: 1px solid rgb(255 255 255 / 48%); border-radius: 4px; background: rgb(111 101 128 / 72%); box-shadow: 0 2px 8px rgb(42 27 72 / 12%); backdrop-filter: blur(8px); color: white; font-size: 11px; font-variant-numeric: tabular-nums; font-weight: 400; }
.reader-toolbar button { min-width: 25px; height: 24px; min-height: 24px; padding: 0 5px; border: 0; border-radius: 3px; background: transparent; color: white; font-size: 11px; font-weight: 400; line-height: 1; transform: none !important; }
.reader-toolbar button:hover:not(:disabled) { background: rgb(255 255 255 / 14%); }
.reader-toolbar button:disabled { opacity: .4; }
.page-indicator { display: flex; gap: 3px; align-items: center; }
.page-indicator button { min-width: 24px; font-size: 11px; font-weight: 400; font-variant-numeric: tabular-nums; }
.page-indicator input { width: 42px; height: 23px; margin: 0; padding: 0 4px; border: 1px solid rgb(255 255 255 / 52%); border-radius: 3px; outline: 0; background: rgb(255 255 255 / 92%); color: #40374d; font-size: 11px; font-weight: 400; text-align: center; }
.page-indicator span { padding-right: 4px; white-space: nowrap; }
.tool-divider { width: 1px; height: 16px; margin: 0 2px; background: rgb(255 255 255 / 24%); }
.reader-toolbar .zoom-value { min-width: 42px; font-size: 11px; font-weight: 400; }
.paper-stage { height: 100%; overflow: auto; overflow-anchor: none; overscroll-behavior: contain; padding: 14px 32px 28px; }
.page-stack { transform-origin: top center; }
.page-anchor + .page-anchor { margin-top: 10px; }
</style>
