<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";
import type { BookmarkItem } from "../types";

defineProps<{ items: BookmarkItem[]; canUndo: boolean; canRedo: boolean }>();
const emit = defineEmits<{
  update: [index: number, patch: Partial<BookmarkItem>];
  remove: [index: number];
  add: [index: number];
  move: [from: number, to: number];
  select: [page: number];
  undo: [];
  redo: [];
}>();
const dragging = ref<number>();

function startDrag(event: PointerEvent, index: number) {
  if (event.button !== 0) return;
  dragging.value = index;
  window.addEventListener("pointermove", moveDrag);
  window.addEventListener("pointerup", finishDrag);
  window.addEventListener("pointercancel", finishDrag);
}

function moveDrag(event: PointerEvent) {
  if (dragging.value === undefined) return;
  const row = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>("[data-bookmark-index]");
  const target = Number(row?.dataset.bookmarkIndex);
  if (!Number.isInteger(target) || target === dragging.value) return;
  emit("move", dragging.value, target);
  dragging.value = target;
}

function finishDrag() {
  dragging.value = undefined;
  window.removeEventListener("pointermove", moveDrag);
  window.removeEventListener("pointerup", finishDrag);
  window.removeEventListener("pointercancel", finishDrag);
}

onBeforeUnmount(finishDrag);

function confidenceLabel(item: BookmarkItem) {
  if (!item.pdfPage) return "页码未映射";
  if (item.confidence < 0.75) return "需要检查";
  return "高置信度";
}
</script>

<template>
  <aside class="editor" aria-label="书签树编辑器">
    <div class="panel-heading">
      <div>
        <h2>书签树</h2>
        <span>{{ items.length }} 条</span>
      </div>
      <div class="history-actions">
        <button type="button" :disabled="!canUndo" @click="emit('undo')">撤销</button>
        <button type="button" :disabled="!canRedo" @click="emit('redo')">重做</button>
      </div>
    </div>
    <div v-if="!items.length" class="empty-editor">
      <strong>尚未生成书签</strong>
      <p>选择目录页并点击“提取目录”。</p>
    </div>
    <ol v-else class="bookmark-list">
      <li
        v-for="(item, index) in items"
        :key="item.id"
        :data-bookmark-index="index"
        :class="{ dragging: dragging === index }"
        :style="{ '--indent': `${item.level * 16}px` }"
      >
        <div class="row-main">
          <span
            class="drag-handle"
            role="button"
            :aria-label="`拖动第 ${index + 1} 条书签排序`"
            @pointerdown.prevent="startDrag($event, index)"
          >⋮⋮</span>
          <input
            class="title-input"
            type="text"
            :value="item.title"
            :title="item.title"
            :aria-label="`第 ${index + 1} 条书签标题`"
            @change="emit('update', index, { title: ($event.target as HTMLInputElement).value })"
          />
          <input
            class="page-input"
            type="number"
            min="1"
            :value="item.pdfPage"
            :aria-label="`${item.title} 的 PDF 目标页`"
            @change="emit('update', index, { pdfPage: Number(($event.target as HTMLInputElement).value) || undefined })"
            @dblclick="item.pdfPage && emit('select', item.pdfPage)"
          />
        </div>
        <div class="row-meta">
          <span class="badge" :class="{ warning: !item.pdfPage || item.confidence < 0.75 }">{{ confidenceLabel(item) }}</span>
          <span>印刷页 {{ item.printedPage ?? "—" }}</span>
          <div class="row-actions">
            <button type="button" title="上移" aria-label="上移" :disabled="index === 0" @click="emit('move', index, index - 1)">↑</button>
            <button type="button" title="下移" aria-label="下移" :disabled="index === items.length - 1" @click="emit('move', index, index + 1)">↓</button>
            <button type="button" title="减少层级" aria-label="减少层级" :disabled="item.level === 0" @click="emit('update', index, { level: item.level - 1 })">←</button>
            <button type="button" title="增加层级" aria-label="增加层级" :disabled="index === 0 || item.level >= items[index - 1].level + 1" @click="emit('update', index, { level: item.level + 1 })">→</button>
            <button type="button" title="新增书签" aria-label="新增书签" @click="emit('add', index)">＋</button>
            <button type="button" class="danger" title="删除书签" aria-label="删除书签" @click="emit('remove', index)">×</button>
          </div>
        </div>
      </li>
    </ol>
  </aside>
</template>

<style scoped>
.editor { min-width: 0; background: var(--surface); border-left: 1px solid var(--border); overflow: hidden; }
.panel-heading { height: 48px; padding: 0 14px; display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); }
.panel-heading > div:first-child { display: flex; gap: 8px; align-items: baseline; }
h2 { margin: 0; font-size: 14px; }
.panel-heading span { color: var(--text-muted); font-size: 12px; }
.history-actions, .row-actions { display: flex; gap: 4px; }
button { min-height: 32px; padding: 0 9px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); cursor: pointer; }
button:hover:not(:disabled) { border-color: var(--accent); background: var(--accent-soft); }
button:disabled { opacity: .42; cursor: not-allowed; }
.empty-editor { padding: 48px 24px; text-align: center; color: var(--text-muted); }
.empty-editor strong { color: var(--text); }
.bookmark-list { height: calc(100% - 49px); margin: 0; padding: 8px; overflow-y: auto; overflow-x: hidden; list-style: none; }
li { width: calc(100% - var(--indent)); margin-left: var(--indent); padding: 9px 8px; border-bottom: 1px solid var(--border-soft); }
li:focus-within { background: var(--accent-soft); }
li.dragging { background: var(--accent-soft); box-shadow: inset 3px 0 var(--accent); }
.row-main { display: grid; grid-template-columns: 18px minmax(90px, 1fr) 68px; gap: 7px; align-items: center; }
.drag-handle { align-self: stretch; display: grid; place-items: center; min-height: 44px; color: var(--text-muted); cursor: grab; user-select: none; touch-action: none; }
.drag-handle:active { cursor: grabbing; }
input { min-width: 0; box-sizing: border-box; border: 1px solid transparent; border-radius: 5px; background: transparent; color: var(--text); font: inherit; }
input:hover, input:focus { border-color: var(--border); background: var(--surface); }
.title-input { height: 34px; width: 100%; padding: 5px 6px; line-height: 1.4; }
.page-input { height: 36px; text-align: right; font-variant-numeric: tabular-nums; }
.row-meta { margin: 6px 0 0 25px; display: flex; gap: 6px 8px; align-items: center; flex-wrap: wrap; color: var(--text-muted); font-size: 11px; }
.badge { padding: 2px 6px; border-radius: 999px; background: var(--success-soft); color: var(--success); }
.badge.warning { background: var(--warning-soft); color: var(--warning); }
.row-actions { width: 100%; margin-left: 0; flex-wrap: wrap; }
.row-actions button { min-height: 32px; width: 32px; flex: 0 0 32px; padding: 0; font-size: 17px; }
.row-actions .danger { color: var(--danger); }
</style>
