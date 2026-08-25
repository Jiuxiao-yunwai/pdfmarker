<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import type { PDFDocumentProxy } from "pdfjs-dist";
import PageThumbnail from "./PageThumbnail.vue";

const props = defineProps<{ document?: PDFDocumentProxy; pageCount: number; currentPage: number }>();
const emit = defineEmits<{ select: [page: number] }>();
const list = ref<HTMLElement>();

watch(() => props.currentPage, async (page) => {
  await nextTick();
  const container = list.value;
  const target = container?.querySelector<HTMLElement>(`[data-thumbnail-page="${page}"]`);
  if (!container || !target) return;
  const containerRect = container.getBoundingClientRect();
  const targetRect = target.getBoundingClientRect();
  if (targetRect.top < containerRect.top) {
    container.scrollTop -= containerRect.top - targetRect.top + 6;
  } else if (targetRect.bottom > containerRect.bottom) {
    container.scrollTop += targetRect.bottom - containerRect.bottom + 6;
  }
}, { immediate: true });
</script>

<template>
  <aside class="thumbnails" aria-label="PDF 页面缩略图">
    <div class="panel-heading">
      <h2>页面</h2>
      <span>{{ pageCount }} 页</span>
    </div>
    <div v-if="document" ref="list" class="thumbnail-list">
      <PageThumbnail
        v-for="page in pageCount"
        :key="page"
        :document="document"
        :page="page"
        :selected="page === currentPage"
        @select="emit('select', $event)"
      />
    </div>
  </aside>
</template>

<style scoped>
.thumbnails { min-width: 0; background: var(--surface-soft); overflow: hidden; }
.panel-heading { height: 50px; padding: 0 14px; display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); background: rgb(255 255 255 / 72%); }
h2 { margin: 0; font-size: 14px; font-weight: 720; }
.panel-heading span { color: var(--text-muted); font-size: 12px; }
.thumbnail-list { height: calc(100% - 51px); overflow-y: auto; padding: 11px; display: grid; gap: 11px; }
</style>
