import { ref, toRaw } from "vue";
import type { BookmarkItem } from "../types";

const copy = (items: BookmarkItem[]) => structuredClone(toRaw(items));

export function useBookmarkHistory() {
  const items = ref<BookmarkItem[]>([]);
  const past = ref<BookmarkItem[][]>([]);
  const future = ref<BookmarkItem[][]>([]);

  function replace(next: BookmarkItem[]) {
    items.value = copy(next);
    past.value = [];
    future.value = [];
  }

  function change(update: (draft: BookmarkItem[]) => void) {
    past.value.push(copy(items.value));
    if (past.value.length > 50) past.value.shift();
    future.value = [];
    const draft = copy(items.value);
    update(draft);
    items.value = draft;
  }

  function undo() {
    const previous = past.value.pop();
    if (!previous) return;
    future.value.push(copy(items.value));
    items.value = previous;
  }

  function redo() {
    const next = future.value.pop();
    if (!next) return;
    past.value.push(copy(items.value));
    items.value = next;
  }

  return { items, past, future, replace, change, undo, redo };
}
