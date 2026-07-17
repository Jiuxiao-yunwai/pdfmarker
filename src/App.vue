<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, shallowRef } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getDocument, GlobalWorkerOptions, type PDFDocumentProxy } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import BookmarkEditor from "./components/BookmarkEditor.vue";
import PdfPreview from "./components/PdfPreview.vue";
import ThumbnailList from "./components/ThumbnailList.vue";
import { useBookmarkHistory } from "./composables/useBookmarkHistory";
import { extractLayoutBlocks, extractOcrBlocks } from "./lib/tocText";
import type { BookmarkItem, ExportResult, PdfInfo, TocExtraction } from "./types";

GlobalWorkerOptions.workerSrc = workerUrl;

const pdf = ref<PdfInfo>();
const previewDocument = shallowRef<PDFDocumentProxy>();
const currentPage = ref(1);
const tocStart = ref(1);
const tocEnd = ref(2);
const anchorPrinted = ref("1");
const anchorPdf = ref(3);
const busy = ref(false);
const status = ref("请选择一本 PDF 开始制作书签");
const error = ref("");
const history = useBookmarkHistory();
function exportableBookmarks() {
  const stack: number[] = [];
  return history.items.value
    .filter((item) => item.pdfPage && item.pdfPage >= 1 && item.pdfPage <= (pdf.value?.pageCount ?? 0))
    .map((item) => {
      while (stack.length && stack[stack.length - 1] >= item.level) stack.pop();
      const exported = { ...item, level: stack.length };
      stack.push(item.level);
      return exported;
    });
}

async function run(task: () => Promise<void>) {
  busy.value = true;
  error.value = "";
  try {
    await task();
  } catch (reason) {
    error.value = String(reason);
  } finally {
    busy.value = false;
  }
}

async function importPdf() {
  await run(async () => {
    const selected = await invoke<PdfInfo | null>("choose_pdf");
    if (!selected) return;
    await previewDocument.value?.destroy();
    const loading = getDocument(convertFileSrc(selected.path));
    previewDocument.value = await loading.promise;
    pdf.value = selected;
    currentPage.value = 1;
    tocStart.value = 1;
    tocEnd.value = Math.min(3, selected.pageCount);
    anchorPdf.value = Math.min(selected.pageCount, tocEnd.value + 1);
    history.replace(selected.existingBookmarks);
    status.value = selected.existingBookmarks.length
      ? `已读取 PDF 中的 ${selected.existingBookmarks.length} 条现有书签，可直接编辑或导出`
      : `已导入 ${selected.name}，请确认目录页范围`;
  });
}

async function extractToc(useOcr = false) {
  if (!pdf.value) return;
  await run(async () => {
    let result: TocExtraction;
    if (useOcr) {
      if (!previewDocument.value) throw new Error("PDF 预览文档尚未就绪");
      const blocks = await extractOcrBlocks(previewDocument.value, tocStart.value, tocEnd.value, (page) => {
        status.value = `正在用系统 OCR 识别第 ${page}/${tocEnd.value} 页…`;
      });
      result = await invoke<TocExtraction>("parse_toc_blocks", { blocks });
    } else {
      status.value = "正在按页面版式提取目录…";
      try {
        if (!previewDocument.value) throw new Error("PDF 预览文档尚未就绪");
        const blocks = await extractLayoutBlocks(previewDocument.value, tocStart.value, tocEnd.value);
        result = await invoke<TocExtraction>("parse_toc_blocks", { blocks });
      } catch {
        result = await invoke<TocExtraction>("extract_toc", {
          path: pdf.value!.path,
          startPage: tocStart.value,
          endPage: tocEnd.value,
        });
      }
    }
    history.replace(result.items);
    status.value = `已识别 ${result.items.length} 条目录，正在应用页码映射`;
    await applyMapping();
  });
}

async function applyMapping() {
  if (!pdf.value || !history.items.value.length) return;
  const mapped = await invoke<BookmarkItem[]>("map_bookmarks", {
    request: {
      items: history.items.value,
      anchorPrinted: anchorPrinted.value,
      anchorPdf: anchorPdf.value,
      pageCount: pdf.value.pageCount,
    },
  });
  history.replace(mapped);
  status.value = `映射完成：${mapped.filter((item) => item.pdfPage).length}/${mapped.length} 条可导出`;
}

async function exportPdf() {
  if (!pdf.value) return;
  await run(async () => {
    const items = exportableBookmarks();
    if (!items.length) throw new Error("没有可导出的书签，请至少为一条书签填写有效的 PDF 页码");
    const skipped = history.items.value.length - items.length;
    status.value = "正在写入新 PDF…";
    const result = await invoke<ExportResult | null>("export_pdf", {
      request: { inputPath: pdf.value!.path, items },
    });
    if (result) status.value = `已导出 ${result.bookmarkCount} 条书签${skipped ? `，跳过 ${skipped} 条未映射项` : ""}：${result.outputPath}`;
    else status.value = "已取消导出";
  });
}

function updateItem(index: number, patch: Partial<BookmarkItem>) {
  history.change((items) => Object.assign(items[index], patch));
}

function addItem(index: number) {
  history.change((items) => items.splice(index + 1, 0, {
    id: crypto.randomUUID(),
    title: "新书签",
    level: items[index]?.level ?? 0,
    pdfPage: currentPage.value,
    confidence: 1,
    sourcePageIndex: currentPage.value - 1,
    children: [],
  }));
}

function removeItem(index: number) {
  history.change((items) => items.splice(index, 1));
}

function moveItem(from: number, to: number) {
  if (from === to) return;
  history.change((items) => items.splice(to, 0, items.splice(from, 1)[0]));
}

function keyboardHistory(event: KeyboardEvent) {
  const target = event.target as HTMLElement;
  if (!event.ctrlKey || ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
  if (event.key.toLowerCase() === "z") {
    event.preventDefault();
    event.shiftKey ? history.redo() : history.undo();
  } else if (event.key.toLowerCase() === "y") {
    event.preventDefault();
    history.redo();
  }
}

onMounted(() => window.addEventListener("keydown", keyboardHistory));
onBeforeUnmount(async () => {
  window.removeEventListener("keydown", keyboardHistory);
  await previewDocument.value?.destroy();
});
</script>

<template>
  <main class="app-shell">
    <header class="topbar">
      <div class="brand">
        <div class="brand-mark" aria-hidden="true">书</div>
        <div><h1>书签匠</h1><p>从目录页制作可靠的 PDF 书签</p></div>
      </div>
      <div class="top-actions">
        <span v-if="pdf" class="file-meta">{{ pdf.name }} · {{ pdf.documentKind }} · {{ pdf.pageCount }} 页</span>
        <button type="button" class="secondary" :disabled="busy" @click="importPdf">{{ pdf ? "更换 PDF" : "导入 PDF" }}</button>
        <button type="button" class="primary" :disabled="busy || !history.items.value.length" @click="exportPdf">导出带书签 PDF</button>
      </div>
    </header>

    <section v-if="pdf" class="setup" aria-label="目录提取设置">
      <fieldset>
        <legend>目录页范围</legend>
        <label>起始页 <input v-model.number="tocStart" type="number" min="1" :max="pdf.pageCount" /></label>
        <span>至</span>
        <label>结束页 <input v-model.number="tocEnd" type="number" :min="tocStart" :max="pdf.pageCount" /></label>
        <button type="button" class="primary" :disabled="busy" @click="extractToc()">提取目录</button>
        <button type="button" class="secondary" title="适合扫描版或文字提取不准确的目录" :disabled="busy" @click="extractToc(true)">系统 OCR</button>
      </fieldset>
      <fieldset>
        <legend>单锚点映射</legend>
        <label>印刷页 <input v-model="anchorPrinted" class="short" /></label>
        <span>对应</span>
        <label>PDF 页 <input v-model.number="anchorPdf" type="number" min="1" :max="pdf.pageCount" /></label>
        <button type="button" class="secondary" :disabled="busy || !history.items.value.length" @click="run(applyMapping)">应用映射</button>
      </fieldset>
    </section>

    <section v-if="pdf" class="workspace">
      <ThumbnailList :document="previewDocument" :page-count="pdf.pageCount" :current-page="currentPage" @select="currentPage = $event" />
      <PdfPreview
        :document="previewDocument"
        :page-count="pdf.pageCount"
        :current-page="currentPage"
        @select="currentPage = $event"
      />
      <BookmarkEditor
        :items="history.items.value"
        :can-undo="history.past.value.length > 0"
        :can-redo="history.future.value.length > 0"
        @update="updateItem"
        @remove="removeItem"
        @add="addItem"
        @move="moveItem"
        @select="currentPage = $event"
        @undo="history.undo"
        @redo="history.redo"
      />
    </section>

    <section v-else class="welcome">
      <div class="welcome-card">
        <div class="document-outline" aria-hidden="true"><span></span><span></span><span></span></div>
        <h2>把没有书签的电子书变得好读</h2>
        <p>导入 PDF，选择目录页，校正页码后导出。原文件始终保持不变。</p>
        <button type="button" class="primary large" :disabled="busy" @click="importPdf">选择 PDF 文件</button>
        <small>支持文本型和扫描型 PDF；扫描目录请使用“系统 OCR”。</small>
      </div>
    </section>

    <footer class="statusbar" :class="{ failed: error }" aria-live="polite">
      <span class="status-dot" aria-hidden="true"></span>
      {{ error || status }}
    </footer>
  </main>
</template>

<style>
:root {
  font-family: "Segoe UI", "Microsoft YaHei UI", sans-serif;
  color: #242923;
  background: #eceee9;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  --text: #242923;
  --text-muted: #626a60;
  --surface: #fffefa;
  --surface-soft: #f7f6f0;
  --canvas: #e7e8e3;
  --border: #d8dbd3;
  --border-soft: #e8e9e4;
  --accent: #2f6651;
  --accent-hover: #285744;
  --accent-soft: #e7f1eb;
  --success: #22603f;
  --success-soft: #e5f2e9;
  --warning: #7a4b0c;
  --warning-soft: #fff0d6;
  --danger: #a13632;
  --shadow-sm: 0 2px 8px rgb(35 42 34 / 12%);
  --shadow-lg: 0 12px 36px rgb(35 42 34 / 18%);
}
* { box-sizing: border-box; }
body { margin: 0; min-width: 1040px; min-height: 680px; overflow: hidden; }
button, input { font: inherit; }
button:focus-visible, input:focus-visible { outline: 3px solid #8db9a6; outline-offset: 2px; }
.app-shell { height: 100vh; display: grid; grid-template-rows: 72px auto minmax(0, 1fr) 30px; }
.topbar { grid-row: 1; display: flex; align-items: center; justify-content: space-between; padding: 0 20px; border-bottom: 1px solid var(--border); background: var(--surface); }
.brand { display: flex; gap: 11px; align-items: center; }
.brand-mark { width: 40px; height: 40px; display: grid; place-items: center; border-radius: 10px; background: var(--accent); color: white; font-family: SimSun, serif; font-size: 22px; font-weight: 700; }
h1 { margin: 0; font-family: SimSun, "Songti SC", serif; font-size: 21px; letter-spacing: 1px; }
.brand p { margin: 2px 0 0; color: var(--text-muted); font-size: 12px; }
.top-actions { display: flex; gap: 10px; align-items: center; }
.file-meta { max-width: 430px; overflow: hidden; color: var(--text-muted); font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
button { min-height: 40px; padding: 0 14px; border: 1px solid transparent; border-radius: 7px; font-weight: 600; cursor: pointer; transition: background 160ms ease, border-color 160ms ease; }
button.primary { background: var(--accent); color: white; }
button.primary:hover:not(:disabled) { background: var(--accent-hover); }
button.secondary { border-color: var(--border); background: var(--surface); color: var(--text); }
button.secondary:hover:not(:disabled) { border-color: var(--accent); background: var(--accent-soft); }
button:disabled { opacity: .45; cursor: not-allowed; }
.setup { grid-row: 2; display: flex; gap: 28px; align-items: center; min-height: 70px; padding: 10px 20px; border-bottom: 1px solid var(--border); background: var(--surface-soft); }
fieldset { margin: 0; padding: 0; border: 0; display: flex; gap: 9px; align-items: center; }
legend { float: left; margin-right: 4px; color: var(--text); font-size: 13px; font-weight: 700; }
label, fieldset > span { color: var(--text-muted); font-size: 12px; }
input { height: 36px; width: 68px; margin-left: 5px; padding: 0 8px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); font-variant-numeric: tabular-nums; }
input.short { width: 64px; }
.setup button { min-height: 36px; }
.workspace { grid-row: 3; min-height: 0; display: grid; grid-template-columns: 172px minmax(430px, 1fr) minmax(420px, 500px); }
.welcome { grid-row: 3; display: grid; place-items: center; background: var(--canvas); }
.welcome-card { width: min(560px, 70vw); padding: 48px; border: 1px solid var(--border); border-radius: 16px; background: var(--surface); box-shadow: var(--shadow-lg); text-align: center; }
.document-outline { width: 76px; height: 94px; margin: 0 auto 24px; padding: 26px 15px 0; border: 2px solid var(--accent); border-radius: 5px; }
.document-outline span { display: block; height: 3px; margin: 9px 0; background: var(--accent); opacity: .7; }
.welcome h2 { margin: 0; font-family: SimSun, "Songti SC", serif; font-size: 26px; }
.welcome p { margin: 14px auto 26px; color: var(--text-muted); line-height: 1.7; }
.welcome .large { min-height: 48px; padding: 0 24px; }
.welcome small { display: block; margin-top: 18px; color: var(--text-muted); }
.statusbar { grid-row: 4; display: flex; gap: 8px; align-items: center; padding: 0 14px; overflow: hidden; border-top: 1px solid var(--border); background: var(--surface); color: var(--text-muted); font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
.status-dot { width: 7px; height: 7px; flex: 0 0 auto; border-radius: 50%; background: var(--success); }
.statusbar.failed { color: var(--danger); }
.statusbar.failed .status-dot { background: var(--danger); }
@media (prefers-reduced-motion: reduce) { * { scroll-behavior: auto !important; transition: none !important; } }
</style>
