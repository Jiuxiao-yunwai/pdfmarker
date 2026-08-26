<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { PDFDocumentProxy } from "pdfjs-dist";
import PageThumbnail from "./PageThumbnail.vue";

const props = defineProps<{ document?: PDFDocumentProxy; pageCount: number; currentPage: number }>();
const emit = defineEmits<{ select: [page: number] }>();
const list = ref<HTMLElement>();
const scrollTop = ref(0);
const viewportHeight = ref(0);
const ROW_HEIGHT = 156;
const THUMBNAIL_HEIGHT = 146;
const LIST_PADDING = 10;
const OVERSCAN_ROWS = 4;
let scrollFrame: number | undefined;
let resizeObserver: ResizeObserver | undefined;

const visiblePages = computed(() => {
  if (!props.pageCount) return [];
  const firstVisible = Math.floor(Math.max(0, scrollTop.value - LIST_PADDING) / ROW_HEIGHT) + 1;
  const visibleRows = Math.max(1, Math.ceil(viewportHeight.value / ROW_HEIGHT));
  const start = Math.max(1, firstVisible - OVERSCAN_ROWS);
  const end = Math.min(props.pageCount, firstVisible + visibleRows + OVERSCAN_ROWS);
  return Array.from({ length: end - start + 1 }, (_, index) => start + index);
});

const trackHeight = computed(() => Math.max(THUMBNAIL_HEIGHT, props.pageCount * ROW_HEIGHT - (ROW_HEIGHT - THUMBNAIL_HEIGHT)));

function updateViewport() {
  const container = list.value;
  if (!container) return;
  scrollTop.value = container.scrollTop;
  viewportHeight.value = container.clientHeight;
}

function handleScroll() {
  if (scrollFrame !== undefined) return;
  scrollFrame = window.requestAnimationFrame(() => {
    scrollFrame = undefined;
    updateViewport();
  });
}

watch(() => props.currentPage, async (page) => {
  await nextTick();
  const container = list.value;
  if (!container || page < 1 || page > props.pageCount) return;
  const targetTop = LIST_PADDING + (page - 1) * ROW_HEIGHT;
  const targetBottom = targetTop + THUMBNAIL_HEIGHT;
  if (targetTop < container.scrollTop + 6) {
    container.scrollTop = Math.max(0, targetTop - 6);
  } else if (targetBottom > container.scrollTop + container.clientHeight - 6) {
    container.scrollTop = targetBottom - container.clientHeight + 6;
  }
  updateViewport();
}, { immediate: true });

watch(() => props.document, async () => {
  await nextTick();
  if (list.value) list.value.scrollTop = 0;
  updateViewport();
});

onMounted(() => {
  updateViewport();
  if (list.value) {
    resizeObserver = new ResizeObserver(updateViewport);
    resizeObserver.observe(list.value);
  }
});

onBeforeUnmount(() => {
  if (scrollFrame !== undefined) window.cancelAnimationFrame(scrollFrame);
  resizeObserver?.disconnect();
});
</script>

<template>
  <aside class="thumbnails" aria-label="PDF 页面缩略图">
    <div class="panel-heading">
      <h2>页面</h2>
      <span>{{ pageCount }} 页</span>
    </div>
    <div v-if="document" ref="list" class="thumbnail-list" @scroll.passive="handleScroll">
      <div class="thumbnail-track" :style="{ height: `${trackHeight}px` }">
        <PageThumbnail
          v-for="page in visiblePages"
          :key="page"
          :document="document"
          :page="page"
          :selected="page === currentPage"
          :style="{ top: `${(page - 1) * ROW_HEIGHT}px` }"
          @select="emit('select', $event)"
        />
      </div>
    </div>
  </aside>
</template>

<style scoped>
.thumbnails { min-width: 0; border-right: 1px solid var(--border); background: var(--surface-soft); overflow: hidden; }
.panel-heading { height: 48px; padding: 0 12px; display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); }
h2 { margin: 0; font-size: 14px; }
.panel-heading span { color: var(--text-muted); font-size: 12px; }
.thumbnail-list { height: calc(100% - 49px); overflow-y: auto; padding: 10px; contain: layout paint style; }
.thumbnail-track { position: relative; width: 100%; }
.thumbnail-track :deep(.thumbnail) { position: absolute; right: 0; left: 0; height: 146px; }
</style>
