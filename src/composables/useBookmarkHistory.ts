import { shallowRef, toRaw } from "vue";
import type { BookmarkItem } from "../types";

const copy = (items: BookmarkItem[]) => structuredClone(toRaw(items));

export function useBookmarkHistory() {
  const items = shallowRef<BookmarkItem[]>([]);
  const past = shallowRef<BookmarkItem[][]>([]);
  const future = shallowRef<BookmarkItem[][]>([]);

  function replace(next: BookmarkItem[]) {
    items.value = copy(next);
    past.value = [];
    future.value = [];
  }

  function change(update: (draft: BookmarkItem[]) => void) {
    const previous = items.value;
    past.value = [...past.value, previous].slice(-50);
    future.value = [];
    // Snapshots share untouched items; callers replace any item they modify.
    const draft = previous.slice();
    update(draft);
    items.value = draft;
  }

  function undo() {
    const previous = past.value[past.value.length - 1];
    if (!previous) return;
    past.value = past.value.slice(0, -1);
    future.value = [...future.value, items.value];
    items.value = previous;
  }

  function redo() {
    const next = future.value[future.value.length - 1];
    if (!next) return;
    future.value = future.value.slice(0, -1);
    past.value = [...past.value, items.value].slice(-50);
    items.value = next;
  }

  return { items, past, future, replace, change, undo, redo };
}
