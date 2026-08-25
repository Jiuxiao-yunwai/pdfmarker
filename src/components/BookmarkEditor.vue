<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { stepNumberInput } from "../lib/numberInput";
import type { BookmarkItem } from "../types";

const props = defineProps<{ items: BookmarkItem[]; canUndo: boolean; canRedo: boolean }>();
const emit = defineEmits<{
  update: [index: number, patch: Partial<BookmarkItem>];
  remove: [index: number];
  add: [index: number, level?: number];
  move: [from: number, to: number];
  select: [page: number];
  undo: [];
  redo: [];
  clear: [];
}>();
const draggingId = ref<string>();
const contextMenu = ref<{ index: number; x: number; y: number }>();
const editingTitle = ref<number>();
const editingPage = ref<number>();
const collapsedIds = ref(new Set<string>());
const visibleBookmarks = computed(() => {
  const result: Array<{ item: BookmarkItem; index: number; hasChildren: boolean; insertAfterIndex: number; insertLevel: number }> = [];
  let hiddenBelowLevel: number | undefined;
  for (const [index, item] of props.items.entries()) {
    if (hiddenBelowLevel !== undefined && item.level > hiddenBelowLevel) continue;
    if (hiddenBelowLevel !== undefined) hiddenBelowLevel = undefined;
    const subtreeEnd = findSubtreeEnd(index);
    const hasChildren = subtreeEnd > index + 1;
    const collapsed = hasChildren && collapsedIds.value.has(item.id);
    const nextLevel = props.items[index + 1]?.level;
    result.push({
      item,
      index,
      hasChildren,
      insertAfterIndex: collapsed ? subtreeEnd - 1 : index,
      insertLevel: collapsed
        ? item.level
        : nextLevel !== undefined && nextLevel > item.level ? nextLevel : item.level,
    });
    if (collapsed) hiddenBelowLevel = item.level;
  }
  return result;
});
let selectTimer: number | undefined;
let dragFrame: number | undefined;
let pendingDragPoint: { x: number; y: number } | undefined;
let lastDragMoveY = 0;
let dragCooldownUntil = 0;

function findSubtreeEnd(index: number) {
  const level = props.items[index]?.level;
  if (level === undefined) return index + 1;
  let end = index + 1;
  while (end < props.items.length && props.items[end].level > level) end += 1;
  return end;
}

function toggleCollapse(item: BookmarkItem) {
  const next = new Set(collapsedIds.value);
  next.has(item.id) ? next.delete(item.id) : next.add(item.id);
  collapsedIds.value = next;
}

function isCollapsed(item: BookmarkItem) {
  return collapsedIds.value.has(item.id);
}

function expandAll() {
  collapsedIds.value = new Set();
  closeContextMenu();
}

function scheduleSelect(item: BookmarkItem) {
  window.clearTimeout(selectTimer);
  if (!item.pdfPage) return;
  selectTimer = window.setTimeout(() => emit("select", item.pdfPage!), 220);
}

async function beginTitleEdit(index: number) {
  window.clearTimeout(selectTimer);
  editingPage.value = undefined;
  editingTitle.value = index;
  await nextTick();
  const editor = document.querySelector<HTMLInputElement>(`[data-title-editor="${index}"]`);
  editor?.focus();
  editor?.select();
}

function commitTitle(index: number, event: FocusEvent) {
  if (editingTitle.value !== index) return;
  const value = (event.target as HTMLInputElement).value.trim();
  if (value) emit("update", index, { title: value });
  editingTitle.value = undefined;
}

async function beginPageEdit(index: number) {
  editingTitle.value = undefined;
  editingPage.value = index;
  await nextTick();
  const editor = document.querySelector<HTMLInputElement>(`[data-page-editor="${index}"]`);
  editor?.focus();
  editor?.select();
}

function commitPage(index: number, event: FocusEvent) {
  if (editingPage.value !== index) return;
  const value = Number((event.target as HTMLInputElement).value);
  emit("update", index, { pdfPage: value >= 1 ? value : undefined });
  editingPage.value = undefined;
}

function openContextMenu(event: MouseEvent, index: number) {
  finishDrag();
  const menuWidth = 156;
  const menuHeight = 186;
  contextMenu.value = {
    index,
    x: Math.max(8, Math.min(event.clientX, window.innerWidth - menuWidth - 8)),
    y: Math.max(8, Math.min(event.clientY, window.innerHeight - menuHeight - 8)),
  };
}

function changeLevelFromContext(offset: -1 | 1) {
  const index = contextMenu.value?.index;
  if (index === undefined) return;
  const item = props.items[index];
  if (!item) return;
  emit("update", index, { level: item.level + offset });
  closeContextMenu();
}

function closeContextMenu() {
  contextMenu.value = undefined;
}

function moveFromContext(offset: -1 | 1) {
  const index = contextMenu.value?.index;
  if (index === undefined) return;
  const target = siblingIndex(index, offset);
  if (target === undefined) return;
  emit("move", index, target);
  closeContextMenu();
}

function siblingIndex(index: number, direction: -1 | 1) {
  const level = props.items[index]?.level;
  if (level === undefined) return undefined;
  if (direction < 0) {
    for (let cursor = index - 1; cursor >= 0; cursor -= 1) {
      if (props.items[cursor].level === level) return cursor;
      if (props.items[cursor].level < level) return undefined;
    }
    return undefined;
  }
  for (let cursor = findSubtreeEnd(index); cursor < props.items.length; cursor += 1) {
    if (props.items[cursor].level === level) return cursor;
    if (props.items[cursor].level < level) return undefined;
  }
  return undefined;
}

function closeContextMenuWithKeyboard(event: KeyboardEvent) {
  if (event.key === "Escape") closeContextMenu();
}

function startDrag(event: PointerEvent, index: number) {
  if (event.button !== 0) return;
  draggingId.value = props.items[index]?.id;
  lastDragMoveY = event.clientY;
  dragCooldownUntil = 0;
  window.addEventListener("pointermove", moveDrag);
  window.addEventListener("pointerup", finishDrag);
  window.addEventListener("pointercancel", finishDrag);
}

function moveDrag(event: PointerEvent) {
  if (!draggingId.value) return;
  pendingDragPoint = { x: event.clientX, y: event.clientY };
  if (dragFrame === undefined) dragFrame = window.requestAnimationFrame(processDragMove);
}

function processDragMove() {
  dragFrame = undefined;
  const point = pendingDragPoint;
  const draggedId = draggingId.value;
  if (!point || !draggedId) return;
  const from = props.items.findIndex((item) => item.id === draggedId);
  if (from < 0 || performance.now() < dragCooldownUntil || Math.abs(point.y - lastDragMoveY) < 3) return;
  const row = document.elementFromPoint(point.x, point.y)?.closest<HTMLElement>("[data-bookmark-index]");
  const target = Number(row?.dataset.bookmarkIndex);
  if (!Number.isInteger(target) || target === from || props.items[target]?.level !== props.items[from].level) return;
  const rect = row!.getBoundingClientRect();
  const crossedDropLine = target > from
    ? point.y >= rect.top + rect.height * .54
    : point.y <= rect.top + rect.height * .46;
  if (!crossedDropLine) return;
  emit("move", from, target);
  lastDragMoveY = point.y;
  dragCooldownUntil = performance.now() + 55;
}

function finishDrag() {
  draggingId.value = undefined;
  pendingDragPoint = undefined;
  if (dragFrame !== undefined) window.cancelAnimationFrame(dragFrame);
  dragFrame = undefined;
  window.removeEventListener("pointermove", moveDrag);
  window.removeEventListener("pointerup", finishDrag);
  window.removeEventListener("pointercancel", finishDrag);
}

onMounted(() => {
  window.addEventListener("pointerdown", closeContextMenu);
  window.addEventListener("blur", closeContextMenu);
  window.addEventListener("resize", closeContextMenu);
  window.addEventListener("keydown", closeContextMenuWithKeyboard);
});

onBeforeUnmount(() => {
  window.clearTimeout(selectTimer);
  finishDrag();
  window.removeEventListener("pointerdown", closeContextMenu);
  window.removeEventListener("blur", closeContextMenu);
  window.removeEventListener("resize", closeContextMenu);
  window.removeEventListener("keydown", closeContextMenuWithKeyboard);
});
</script>

<template>
  <aside class="editor" aria-label="书签树编辑器">
    <div class="panel-heading">
      <div>
        <h2>书签</h2>
        <span>{{ items.length }} 条</span>
      </div>
      <div class="history-actions">
        <button type="button" class="clear-action" :disabled="!items.length" title="清空全部书签" @click="emit('clear')">清空</button>
        <button type="button" :disabled="!canUndo" @click="emit('undo')">撤销</button>
        <button type="button" :disabled="!canRedo" @click="emit('redo')">重做</button>
      </div>
    </div>
    <div v-if="!items.length" class="empty-editor">
      <strong>暂无书签</strong>
    </div>
    <TransitionGroup v-else tag="ol" name="bookmark" class="bookmark-list">
      <li
        v-for="entry in visibleBookmarks"
        :key="entry.item.id"
        :data-bookmark-index="entry.index"
        :class="{ dragging: draggingId === entry.item.id }"
        :style="{ '--indent': `${entry.item.level * 12}px` }"
        @contextmenu.stop.prevent="openContextMenu($event, entry.index)"
      >
        <div class="row-main">
          <button
            v-if="entry.hasChildren"
            type="button"
            class="tree-toggle"
            :class="{ collapsed: isCollapsed(entry.item) }"
            :aria-label="isCollapsed(entry.item) ? `展开 ${entry.item.title}` : `折叠 ${entry.item.title}`"
            @click.stop="toggleCollapse(entry.item)"
          ><svg viewBox="0 0 12 12" aria-hidden="true"><path d="m3 4 3 3 3-3" /></svg></button>
          <span v-else class="tree-spacer" aria-hidden="true"></span>
          <span
            class="drag-handle"
            role="button"
            :aria-label="`拖动第 ${entry.index + 1} 条书签排序`"
            @pointerdown.prevent="startDrag($event, entry.index)"
          >⋮⋮</span>
          <input
            v-if="editingTitle === entry.index"
            :data-title-editor="entry.index"
            class="title-input"
            type="text"
            :value="entry.item.title"
            :title="entry.item.title"
            :aria-label="`第 ${entry.index + 1} 条书签标题`"
            @blur="commitTitle(entry.index, $event)"
            @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
            @keydown.esc.prevent="editingTitle = undefined"
          />
          <button
            v-else
            type="button"
            class="title-display"
            :class="{ mapped: entry.item.pdfPage }"
            :title="`${entry.item.title}${entry.item.pdfPage ? ` · 第 ${entry.item.pdfPage} 页` : ''}；双击编辑`"
            @click.stop="scheduleSelect(entry.item)"
            @dblclick.stop.prevent="beginTitleEdit(entry.index)"
          >{{ entry.item.title }}</button>
          <input
            v-if="editingPage === entry.index"
            :data-page-editor="entry.index"
            class="page-input"
            type="number"
            min="1"
            :value="entry.item.pdfPage"
            :aria-label="`${entry.item.title} 的 PDF 目标页`"
            @wheel="stepNumberInput"
            @blur="commitPage(entry.index, $event)"
            @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
            @keydown.esc.prevent="editingPage = undefined"
          />
          <button
            v-else
            type="button"
            class="page-display"
            title="单击编辑页码"
            :aria-label="`${entry.item.title} 的 PDF 目标页：${entry.item.pdfPage ?? '未设置'}，单击编辑`"
            @click.stop="beginPageEdit(entry.index)"
          >{{ entry.item.pdfPage ?? "—" }}</button>
          <div class="row-actions">
            <button type="button" class="danger" :title="entry.hasChildren ? '删除书签并选择子书签处理方式' : '删除书签'" :aria-label="entry.hasChildren ? '删除书签并选择子书签处理方式' : '删除书签'" @click="emit('remove', entry.index)">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3m3 0-1 13H7L6 7m4 4v5m4-5v5" /></svg>
            </button>
          </div>
        </div>
        <button
          type="button"
          class="insert-gap"
          aria-label="在此处添加书签"
          title="单击添加书签"
          @click.stop="emit('add', entry.insertAfterIndex, entry.insertLevel)"
        ><span aria-hidden="true">＋</span></button>
      </li>
    </TransitionGroup>
  </aside>

  <Teleport to="body">
    <div
      v-if="contextMenu"
      class="bookmark-context-menu"
      role="menu"
      aria-label="书签操作"
      :style="{ left: `${contextMenu.x}px`, top: `${contextMenu.y}px` }"
      @pointerdown.stop
      @contextmenu.prevent
    >
      <button
        type="button"
        role="menuitem"
        :disabled="collapsedIds.size === 0"
        @click="expandAll"
      ><span aria-hidden="true">⌄</span>全部展开</button>
      <div class="menu-separator" role="separator"></div>
      <button
        type="button"
        role="menuitem"
        :disabled="siblingIndex(contextMenu.index, -1) === undefined"
        @click="moveFromContext(-1)"
      ><span aria-hidden="true">↑</span>上移书签</button>
      <button
        type="button"
        role="menuitem"
        :disabled="siblingIndex(contextMenu.index, 1) === undefined"
        @click="moveFromContext(1)"
      ><span aria-hidden="true">↓</span>下移书签</button>
      <div class="menu-separator" role="separator"></div>
      <button
        type="button"
        role="menuitem"
        :disabled="items[contextMenu.index]?.level === 0"
        @click="changeLevelFromContext(-1)"
      ><span aria-hidden="true">←</span>左移层级</button>
      <button
        type="button"
        role="menuitem"
        :disabled="contextMenu.index === 0 || items[contextMenu.index].level >= items[contextMenu.index - 1].level + 1"
        @click="changeLevelFromContext(1)"
      ><span aria-hidden="true">→</span>右移层级</button>
    </div>
  </Teleport>
</template>

<style scoped>
.editor { min-width: 0; background: var(--surface); overflow: hidden; }
.panel-heading { height: 50px; padding: 0 14px; display: flex; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--border); background: rgb(255 255 255 / 92%); }
.panel-heading > div:first-child { display: flex; gap: 8px; align-items: baseline; }
h2 { margin: 0; font-size: 15px; font-weight: 720; }
.panel-heading span { color: var(--text-muted); font-size: 12px; }
.history-actions { display: flex; gap: 4px; }
.history-actions .clear-action { border-color: transparent; color: var(--text-muted); }
.history-actions .clear-action:hover:not(:disabled) { border-color: #dfcaca; background: #fff5f4; color: var(--danger); }
button { min-height: 28px; padding: 0 8px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); color: var(--text); cursor: pointer; }
button:hover:not(:disabled) { border-color: var(--accent); background: var(--accent-soft); }
button:disabled { opacity: .42; cursor: not-allowed; }
.empty-editor { padding: 48px 24px; text-align: center; color: var(--text-muted); }
.empty-editor strong { color: var(--text); }
.bookmark-list { height: calc(100% - 51px); margin: 0; padding: 7px 8px 14px; overflow-y: auto; overflow-x: hidden; list-style: none; }
li { position: relative; width: 100%; padding: 7px 5px; border-bottom: 1px solid var(--border-soft); border-radius: 7px; transition: background 120ms ease, box-shadow 150ms ease, opacity 150ms ease; }
li:hover { background: var(--surface-soft); }
li:has(.insert-gap:hover), li:has(.insert-gap:focus-visible) { background: var(--surface); }
li:focus-within { background: var(--accent-soft); }
li.dragging { z-index: 3; opacity: .82; background: var(--accent-soft); box-shadow: inset 3px 0 var(--accent), 0 7px 18px rgb(76 47 139 / 16%); }
.bookmark-move { transition: transform 125ms cubic-bezier(.22, 1, .36, 1) !important; will-change: transform; }
.row-main { display: grid; grid-template-columns: 16px 12px minmax(90px, 1fr) 58px 30px; gap: 5px; align-items: center; padding-left: var(--indent); transition: padding-left 150ms ease; }
.tree-toggle { width: 16px; min-height: 24px; padding: 0; border: 0; background: transparent; color: #8d849b; transform: none !important; }
.tree-toggle:hover:not(:disabled) { border-color: transparent; background: #f0edf5; color: var(--accent); }
.tree-toggle svg { width: 12px; height: 12px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; transition: transform 150ms ease; }
.tree-toggle.collapsed svg { transform: rotate(-90deg); }
.tree-spacer { width: 16px; }
.drag-handle { align-self: stretch; display: grid; place-items: center; min-height: 36px; color: #a59ab9; cursor: grab; user-select: none; touch-action: none; font-size: 13px; transition: color 120ms ease, transform 120ms ease; }
.drag-handle:active { cursor: grabbing; }
li.dragging .drag-handle { color: var(--accent); transform: scaleY(1.18); }
input { min-width: 0; box-sizing: border-box; border: 1px solid transparent; border-radius: 3px; background: transparent; color: var(--text); font: inherit; font-size: 14px; }
input:hover, input:focus { border-color: var(--border); background: var(--surface); }
.title-input { height: 36px; width: 100%; padding: 4px 6px; line-height: 1.35; }
.page-input { width: 100%; height: 34px; padding: 0 6px; outline: none !important; box-shadow: inset 0 0 0 1px var(--border); text-align: right; font-size: 14px; font-variant-numeric: tabular-nums; transform: none !important; transition: border-color 120ms ease, box-shadow 120ms ease; }
.page-input:focus { border-color: #aa98cb; box-shadow: inset 0 0 0 1px #aa98cb; transform: none !important; }
.title-display, .page-display { min-width: 0; height: 36px; min-height: 36px; overflow: hidden; padding: 0 6px; border: 1px solid transparent; border-radius: 3px; background: transparent; color: var(--text); font-size: 14px; transform: none !important; }
.title-display { display: block; text-align: left; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; cursor: default; }
.title-display.mapped { cursor: pointer; }
.title-display:hover, .page-display:hover { border-color: var(--border-soft); background: var(--surface-soft); }
.page-display { text-align: right; color: var(--text-muted); font-variant-numeric: tabular-nums; }
.row-actions { display: flex; flex: 0 0 auto; justify-content: flex-end; overflow: hidden; border: 1px solid var(--border-soft); border-radius: 7px; background: var(--surface); }
.row-actions button { min-height: 30px; width: 28px; flex: 0 0 28px; padding: 0; border: 0; border-radius: 0; background: transparent; color: var(--text-muted); font-size: 15px; line-height: 1; }
.row-actions button:hover:not(:disabled) { background: var(--accent-soft); color: var(--accent); }
.row-actions .danger { color: var(--danger); }
.row-actions button:hover { transform: none; }
.row-actions svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
.insert-gap { position: absolute; z-index: 4; right: 0; bottom: -9px; left: 0; height: 18px; min-height: 18px; overflow: visible; padding: 0; border: 0; border-radius: 0; background: transparent; transform: none !important; }
.insert-gap::before { content: ""; position: absolute; right: 0; left: 21px; top: 8px; height: 1px; background: transparent; transition: background 120ms ease; }
.insert-gap span { position: absolute; top: 8px; left: -2px; width: 22px; height: 22px; display: grid; place-items: center; border: 1px solid transparent; border-radius: 50%; background: transparent; color: #958ba1; font-size: 18px; font-weight: 400; line-height: 1; opacity: 0; transform: translateY(-50%); transition: opacity 120ms ease, color 120ms ease, background 120ms ease, border-color 120ms ease; }
.insert-gap:hover, .insert-gap:focus-visible { background: rgb(70 88 214 / 2.5%); }
.insert-gap:hover::before, .insert-gap:focus-visible::before { background: #d9d1e5; }
.insert-gap:hover span, .insert-gap:focus-visible span { border-color: #d8d0e2; background: #f4f1f7; color: #756b82; opacity: .92; }
.bookmark-context-menu { position: fixed; z-index: 200; width: 148px; overflow: hidden; padding: 5px; border: 1px solid var(--border); border-radius: 9px; background: rgb(255 255 255 / 98%); box-shadow: 0 14px 34px rgb(25 34 58 / 22%); backdrop-filter: blur(12px); animation: context-menu-in 110ms ease-out; }
.bookmark-context-menu button { width: 100%; min-height: 32px; display: flex; gap: 9px; align-items: center; padding: 0 10px; border: 0; border-radius: 3px; background: transparent; color: var(--text); font-size: 12px; font-weight: 550; text-align: left; }
.bookmark-context-menu button:hover:not(:disabled) { background: var(--accent-soft); color: var(--accent); transform: none; }
.bookmark-context-menu button:disabled { opacity: .38; }
.bookmark-context-menu button span { width: 14px; color: var(--text-muted); font-size: 14px; text-align: center; }
.menu-separator { height: 1px; margin: 3px 7px; background: var(--border-soft); }
@keyframes context-menu-in { from { opacity: 0; transform: translateY(-3px); } to { opacity: 1; transform: translateY(0); } }
</style>
