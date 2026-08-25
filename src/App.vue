<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getDocument, GlobalWorkerOptions, type PDFDocumentProxy } from "pdfjs-dist";
import versionInfo from "../version.json";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import appLogo from "./assets/app-logo.png";
import BookmarkEditor from "./components/BookmarkEditor.vue";
import PdfPreview from "./components/PdfPreview.vue";
import ThumbnailList from "./components/ThumbnailList.vue";
import { useBookmarkHistory } from "./composables/useBookmarkHistory";
import { unchangedExportSettings } from "./lib/exportSettings";
import { stepNumberInput } from "./lib/numberInput";
import { extractVisionImageItems, extractVisionItems } from "./lib/tocText";
import { buildWebTocPrompt, parseWebTocResult } from "./lib/webBookmarks";
import type { BookmarkItem, ExportResult, PageRangeExportResult, PdfInfo, VisionResult } from "./types";

GlobalWorkerOptions.workerSrc = workerUrl;
const pdfWasmUrl = new URL("pdfjs/wasm/", document.baseURI).href;

const pdf = ref<PdfInfo>();
const previewDocument = shallowRef<PDFDocumentProxy>();
const previewRevision = ref(0);
const currentPage = ref(1);
const tocStart = ref(1);
const tocEnd = ref(1);
const tocStartDraft = ref("1");
const tocEndDraft = ref("1");
const tocRangeHint = ref("");
const anchorPrinted = ref("1");
const anchorPdf = ref(1);
const apiEndpoint = ref(localStorage.getItem("api.endpoint") ?? "");
const apiKey = ref(localStorage.getItem("api.key") ?? "");
const apiModel = ref(localStorage.getItem("api.model") ?? "");
const apiSettingsOpen = ref(false);
const apiDraft = ref({ endpoint: "", key: "", model: "" });
const webImportOpen = ref(false);
const webImportText = ref("");
const webImportError = ref("");
const promptCopied = ref(false);
const appInfoOpen = ref(false);
type ConfirmationState = {
  title: string;
  message: string;
  confirmLabel: string;
  subtitle?: string;
  footerMessage?: string;
  tone?: "default" | "danger";
  resolve: (confirmed: boolean) => void;
};
const confirmation = ref<ConfirmationState>();
type DeleteChoice = "all" | "promote" | "cancel";
type DeleteConfirmationState = {
  title: string;
  childCount: number;
  resolve: (choice: DeleteChoice) => void;
};
const deleteConfirmation = ref<DeleteConfirmationState>();
const busy = ref(false);
const status = ref("导入 PDF 开始");
const error = ref("");
const history = useBookmarkHistory();
const APP_VERSION = `开发版 ${versionInfo.development}`;
const RELEASE_VERSION = versionInfo.release;
const VERSION_CHANNEL = versionInfo.channel === "development" ? "开发通道" : "正式通道";
type AiPhase = "idle" | "loading" | "analyzing" | "rendering" | "mapping" | "exporting" | "complete" | "failed";
const aiActivity = ref({
  phase: "idle" as AiPhase,
  title: "",
  detail: "",
  current: 0,
  total: 0,
  indeterminate: false,
});
const lastVision = ref<VisionResult>();
const workflowElapsedMs = ref<number>();
const progressPercent = computed(() => aiActivity.value.total
  ? Math.min(100, Math.round(aiActivity.value.current / aiActivity.value.total * 100))
  : 0);
const activityTitle = computed(() => aiActivity.value.phase !== "idle"
  ? aiActivity.value.title
  : error.value || status.value);
const activityDetail = computed(() => aiActivity.value.detail || (error.value
  ? "请检查 API 配置、网络连接或目录页范围后重试"
  : ""));
const apiConfigured = computed(() => Boolean(
  apiEndpoint.value.trim() && apiKey.value.trim() && apiModel.value.trim(),
));
const webTocPrompt = computed(() => buildWebTocPrompt(tocStart.value, tocEnd.value));
let promptCopiedTimer: number | undefined;
let tocRangeHintTimer: number | undefined;
watch([apiEndpoint, apiKey, apiModel], ([endpoint, key, model]) => {
  localStorage.setItem("api.endpoint", endpoint);
  localStorage.setItem("api.key", key);
  localStorage.setItem("api.model", model);
});
function exportableBookmarks(source = history.items.value) {
  const stack: number[] = [];
  return source
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
    const message = String(reason).replace(/^Error:\s*/, "");
    error.value = message;
    if (aiActivity.value.phase !== "failed") setAiActivity("failed", "操作失败", message);
  } finally {
    busy.value = false;
  }
}

function setAiActivity(
  phase: AiPhase,
  title: string,
  detail: string,
  options: { current?: number; total?: number; indeterminate?: boolean } = {},
) {
  aiActivity.value = {
    phase,
    title,
    detail,
    current: options.current ?? 0,
    total: options.total ?? 0,
    indeterminate: options.indeterminate ?? false,
  };
}

async function notify(title: string, body: string) {
  try {
    await invoke("show_app_notification", { title, body });
  } catch {
    // Notifications are an enhancement; parsing results must not depend on them.
  }
}

function formatTokens(value?: number) {
  return value === undefined ? "未提供" : value.toLocaleString("zh-CN");
}

function formatDuration(milliseconds?: number) {
  if (milliseconds === undefined) return "—";
  return milliseconds < 1000 ? `${Math.round(milliseconds)} ms` : `${(milliseconds / 1000).toFixed(1)} 秒`;
}

function openApiSettings() {
  apiDraft.value = { endpoint: apiEndpoint.value, key: apiKey.value, model: apiModel.value };
  promptCopied.value = false;
  webImportOpen.value = false;
  apiSettingsOpen.value = true;
}

function saveApiSettings() {
  apiEndpoint.value = apiDraft.value.endpoint.trim();
  apiKey.value = apiDraft.value.key;
  apiModel.value = apiDraft.value.model.trim();
  apiSettingsOpen.value = false;
}

function showTocRangeHint(message: string) {
  tocRangeHint.value = message;
  window.clearTimeout(tocRangeHintTimer);
  tocRangeHintTimer = window.setTimeout(() => tocRangeHint.value = "", 2400);
}

function restorePageDraft(target: "start" | "end", value: number, message: string) {
  if (target === "start") tocStartDraft.value = String(value);
  else tocEndDraft.value = String(value);
  showTocRangeHint(message);
}

function commitTocStart() {
  const value = Number(tocStartDraft.value);
  const pageCount = pdf.value?.pageCount ?? 1;
  if (!Number.isInteger(value) || value < 1 || value > pageCount) {
    restorePageDraft("start", tocStart.value, `请输入 1–${pageCount}`);
    return;
  }
  tocStart.value = value;
  tocStartDraft.value = String(value);
  if (tocEnd.value < value) {
    tocEnd.value = value;
    tocEndDraft.value = String(value);
    showTocRangeHint(`结束页已同步为 ${value}`);
  } else {
    tocRangeHint.value = "";
  }
}

function commitTocEnd() {
  const value = Number(tocEndDraft.value);
  const pageCount = pdf.value?.pageCount ?? 1;
  if (!Number.isInteger(value) || value < tocStart.value || value > pageCount) {
    restorePageDraft("end", tocEnd.value, value < tocStart.value
      ? `结束页不能小于 ${tocStart.value}`
      : `请输入 ${tocStart.value}–${pageCount}`);
    return;
  }
  tocEnd.value = value;
  tocEndDraft.value = String(value);
  tocRangeHint.value = "";
}

async function copyWebPrompt() {
  try {
    await navigator.clipboard.writeText(webTocPrompt.value);
  } catch {
    const textarea = document.createElement("textarea");
    textarea.value = webTocPrompt.value;
    textarea.style.position = "fixed";
    textarea.style.left = "-9999px";
    document.body.appendChild(textarea);
    textarea.select();
    const copied = document.execCommand("copy");
    textarea.remove();
    if (!copied) {
      error.value = "无法写入剪贴板，请稍后重试";
      return;
    }
  }
  error.value = "";
  status.value = `网页识别提示词已复制，请在网页 AI 中上传 PDF 或第 ${tocStart.value}–${tocEnd.value} 页截图`;
  promptCopied.value = true;
  window.clearTimeout(promptCopiedTimer);
  promptCopiedTimer = window.setTimeout(() => promptCopied.value = false, 2200);
}

function openWebImport() {
  apiSettingsOpen.value = false;
  webImportText.value = "";
  webImportError.value = "";
  webImportOpen.value = true;
}

async function importWebResult() {
  if (!pdf.value) return;
  webImportError.value = "";
  let items: BookmarkItem[];
  try {
    items = parseWebTocResult(webImportText.value, tocStart.value, tocEnd.value);
  } catch (reason) {
    webImportError.value = String(reason).replace(/^Error:\s*/, "");
    return;
  }

  busy.value = true;
  error.value = "";
  try {
    lastVision.value = undefined;
    workflowElapsedMs.value = undefined;
    history.replace(items);
    setAiActivity("mapping", "导入网页结果", `已读取 ${items.length} 条目录，正在计算目标页…`, {
      indeterminate: true,
    });
    const mapped = await applyMapping(false);
    const mappedCount = mapped?.filter((item) => item.pdfPage).length ?? 0;
    webImportOpen.value = false;
    status.value = `已导入网页 AI 结果：${items.length} 条目录，${mappedCount} 条可导出`;
    setAiActivity(
      "complete",
      "网页结果已导入",
      `${items.length} 条目录 · ${mappedCount} 条可导出 · 全程本地处理`,
      { current: 1, total: 1 },
    );
  } catch (reason) {
    webImportError.value = String(reason).replace(/^Error:\s*/, "");
    setAiActivity("failed", "网页结果导入失败", webImportError.value);
  } finally {
    busy.value = false;
  }
}

async function importPdf() {
  lastVision.value = undefined;
  workflowElapsedMs.value = undefined;
  await run(async () => {
    setAiActivity("loading", "导入 PDF", "选择文件", { indeterminate: true });
    const selected = await invoke<PdfInfo | null>("choose_pdf");
    if (!selected) {
      setAiActivity("idle", "", "");
      return;
    }
    setAiActivity("loading", "加载 PDF", selected.name, { indeterminate: true });
    const previousDocument = previewDocument.value;
    const loading = getDocument({
      url: convertFileSrc(selected.path),
      wasmUrl: pdfWasmUrl,
    });
    loading.onProgress = ({ loaded, total }: { loaded: number; total: number }) => {
      if (!total) {
        setAiActivity("loading", "加载 PDF", selected.name, { indeterminate: true });
        return;
      }
      setAiActivity("loading", "加载 PDF", selected.name, {
        current: Math.min(100, Math.round(loaded / total * 100)),
        total: 100,
      });
    };
    previewDocument.value = await loading.promise;
    pdf.value = selected;
    previewRevision.value++;
    currentPage.value = 1;
    tocStart.value = 1;
    tocEnd.value = 1;
    tocStartDraft.value = "1";
    tocEndDraft.value = "1";
    anchorPrinted.value = "1";
    anchorPdf.value = 1;
    history.replace(selected.existingBookmarks);
    await nextTick();
    if (previousDocument) await previousDocument.destroy().catch(() => undefined);
    const bookmarkSummary = selected.existingBookmarks.length ? ` · ${selected.existingBookmarks.length} 条书签` : "";
    status.value = `${selected.name} · ${selected.pageCount} 页${bookmarkSummary}`;
    setAiActivity("complete", "PDF 已导入", status.value, { current: 1, total: 1 });
  });
}

async function extractWithVision() {
  if (!pdf.value) return;
  if (!apiConfigured.value) {
    const openSettings = await requestConfirmation(
      "未设置 API",
      "AI 解析需要先填写 API URL、API Key 和模型名称。",
      "前往设置",
      {
        subtitle: "尚未配置多模态接口",
        footerMessage: "也可以在 API 设置中复制网页提示词。",
      },
    );
    if (openSettings) openApiSettings();
    return;
  }
  const selectedPdf = pdf.value;
  await run(async () => {
    const workflowStarted = performance.now();
    lastVision.value = undefined;
    workflowElapsedMs.value = undefined;
    try {
      const config = { endpoint: apiEndpoint.value, apiKey: apiKey.value, model: apiModel.value };
      setAiActivity(
        "analyzing",
        "AI 解析中",
        `正在提交 PDF 第 ${tocStart.value}–${tocEnd.value} 页，请稍候…`,
        { indeterminate: true },
      );
      let result: VisionResult;
      try {
        result = await extractVisionItems(selectedPdf.path, tocStart.value, tocEnd.value, config);
      } catch (reason) {
        if (!String(reason).includes("PDF_INPUT_UNSUPPORTED")) throw reason;
        if (!previewDocument.value) throw new Error("PDF 预览文档尚未就绪，无法生成高清截图");
        result = await extractVisionImageItems(
          previewDocument.value,
          tocStart.value,
          tocEnd.value,
          config,
          (progress) => {
            if (progress.phase === "analyzing") {
              setAiActivity(
                "analyzing",
                "AI 解析中",
                `已发送 ${progress.total} 张高清截图，正在等待模型返回…`,
                { indeterminate: true },
              );
            } else {
              const detail = progress.completed
                ? `已生成 ${progress.completed}/${progress.total} 张${progress.page ? ` · PDF 第 ${progress.page} 页` : ""}`
                : `准备渲染 ${progress.total} 张高清目录截图`;
              setAiActivity("rendering", "生成高清截图", detail, progress);
            }
          },
        );
      }
      lastVision.value = result;
      history.replace(result.items);
      setAiActivity("mapping", "整理识别结果", `AI 已识别 ${result.items.length} 条目录，正在计算目标页…`, {
        indeterminate: true,
      });
      const mapped = await applyMapping(false);
      workflowElapsedMs.value = Math.round(performance.now() - workflowStarted);
      const mappedCount = mapped?.filter((item) => item.pdfPage).length ?? 0;
      const transport = result.transport === "images" ? "高清截图" : "PDF 文件";
      const tokenSummary = result.usage.totalTokens === undefined
        ? "API 未提供 Token 统计"
        : `${result.usage.totalTokens.toLocaleString("zh-CN")} Token`;
      setAiActivity(
        "complete",
        "AI 解析完成",
        `${result.items.length} 条目录 · ${mappedCount} 条可导出 · ${transport}`,
        { current: 1, total: 1 },
      );
      status.value = `AI 解析完成：${result.items.length} 条目录，${mappedCount} 条可导出`;
      await notify(
        "书签匠 · AI 解析完成",
        `${result.items.length} 条目录，${mappedCount} 条可导出；${tokenSummary}，用时 ${formatDuration(workflowElapsedMs.value)}`,
      );
    } catch (reason) {
      workflowElapsedMs.value = Math.round(performance.now() - workflowStarted);
      const message = String(reason).replace(/^Error:\s*/, "");
      setAiActivity("failed", "AI 解析失败", message);
      await notify("书签匠 · AI 解析失败", message.slice(0, 180));
      throw reason;
    }
  });
}

async function exportTocRange() {
  if (!pdf.value) return;
  if (!await confirmExportSettings("目录 PDF")) return;
  await run(async () => {
    setAiActivity(
      "exporting",
      "导出目录 PDF",
      `第 ${tocStart.value}–${tocEnd.value} 页`,
      { indeterminate: true },
    );
    const result = await invoke<PageRangeExportResult | null>("export_page_range", {
      request: {
        inputPath: pdf.value!.path,
        startPage: tocStart.value,
        endPage: tocEnd.value,
      },
    });
    if (result) {
      status.value = `已导出目录 PDF · ${result.pageCount} 页`;
      setAiActivity("complete", "目录 PDF 已导出", status.value, { current: 1, total: 1 });
    } else {
      status.value = "已取消导出";
      setAiActivity("idle", "", "");
    }
  });
}

async function mapBookmarkItems(items: BookmarkItem[]) {
  if (!pdf.value || !items.length) return items;
  return invoke<BookmarkItem[]>("map_bookmarks", {
    request: {
      items,
      anchorPrinted: anchorPrinted.value,
      anchorPdf: anchorPdf.value,
      pageCount: pdf.value.pageCount,
    },
  });
}

async function applyMapping(showStatus = true) {
  if (!pdf.value || !history.items.value.length) return;
  if (showStatus) setAiActivity("mapping", "映射页码", `${history.items.value.length} 条书签`, { indeterminate: true });
  const mapped = await mapBookmarkItems(history.items.value);
  history.replace(mapped);
  if (showStatus) {
    const mappedCount = mapped.filter((item) => item.pdfPage).length;
    status.value = `已映射 ${mappedCount}/${mapped.length} 条`;
    setAiActivity("complete", "映射完成", status.value, { current: 1, total: 1 });
  }
  return mapped;
}

async function applyMappingManually() {
  await applyMapping(true);
}

async function exportPdf() {
  if (!pdf.value) return;
  if (!await confirmExportSettings("带书签 PDF")) return;
  await run(async () => {
    setAiActivity("mapping", "应用当前页码映射", `${history.items.value.length} 条书签`, { indeterminate: true });
    const mapped = await mapBookmarkItems(history.items.value);
    history.replace(mapped);
    const items = exportableBookmarks(mapped);
    if (!items.length) throw new Error("没有可导出的书签，请至少为一条书签填写有效的 PDF 页码");
    const skipped = history.items.value.length - items.length;
    setAiActivity("exporting", "导出 PDF", `${items.length} 条书签`, { indeterminate: true });
    const result = await invoke<ExportResult | null>("export_pdf", {
      request: { inputPath: pdf.value!.path, items },
    });
    if (result) {
      status.value = `已导出 ${result.bookmarkCount} 条${skipped ? ` · 跳过 ${skipped} 条` : ""}`;
      setAiActivity("complete", "导出完成", status.value, { current: 1, total: 1 });
    } else {
      status.value = "已取消导出";
      setAiActivity("idle", "", "");
    }
  });
}

function updateItem(index: number, patch: Partial<BookmarkItem>) {
  const current = history.items.value[index];
  if (!current) return;
  history.change((items) => {
    const item = items[index];
    const previousLevel = item.level;
    Object.assign(item, patch);
    if (patch.level === undefined || patch.level === previousLevel) return;
    const levelOffset = patch.level - previousLevel;
    for (let cursor = index + 1; cursor < items.length && items[cursor].level > previousLevel; cursor += 1) {
      items[cursor].level = Math.max(0, items[cursor].level + levelOffset);
    }
  });
}

function addItem(index: number, level?: number) {
  history.change((items) => {
    const previousLevel = items[index]?.level ?? 0;
    const nextLevel = items[index + 1]?.level;
    items.splice(index + 1, 0, {
      id: crypto.randomUUID(),
      title: "新书签",
      level: level ?? (nextLevel !== undefined && nextLevel > previousLevel ? nextLevel : previousLevel),
      pdfPage: currentPage.value,
      confidence: 1,
      sourcePageIndex: currentPage.value - 1,
      children: [],
    });
  });
}

async function removeItem(index: number) {
  const item = history.items.value[index];
  if (!item) return;
  const end = findSubtreeEnd(history.items.value, index);
  const childCount = end - index - 1;
  if (!childCount) {
    history.change((items) => items.splice(index, 1));
    return;
  }
  const choice = await requestDeleteChoice(item.title, childCount);
  if (choice === "cancel") return;
  history.change((items) => {
    if (choice === "all") {
      items.splice(index, childCount + 1);
      return;
    }
    items.splice(index, 1);
    for (let cursor = index; cursor < index + childCount; cursor += 1) {
      items[cursor].level = Math.max(0, items[cursor].level - 1);
    }
  });
  status.value = choice === "all"
    ? `已删除“${item.title}”及 ${childCount} 条子书签`
    : `已删除“${item.title}”，并保留 ${childCount} 条子书签`;
}

function moveItem(from: number, to: number) {
  const current = history.items.value;
  if (from === to || !current[from] || !current[to] || current[from].level !== current[to].level) return;
  const sourceEnd = findSubtreeEnd(current, from);
  if (to >= from && to < sourceEnd) return;
  const targetEnd = findSubtreeEnd(current, to);
  history.change((items) => {
    const block = items.splice(from, sourceEnd - from);
    const insertAt = to < from ? to : targetEnd - block.length;
    items.splice(insertAt, 0, ...block);
  });
}

function findSubtreeEnd(items: BookmarkItem[], index: number) {
  const level = items[index]?.level;
  if (level === undefined) return index + 1;
  let end = index + 1;
  while (end < items.length && items[end].level > level) end += 1;
  return end;
}

async function clearBookmarks() {
  if (!history.items.value.length || !await requestConfirmation(
    "清空全部书签",
    `将删除当前的 ${history.items.value.length} 条书签，此操作可以使用“撤销”恢复。`,
    "确认清空",
    {
      subtitle: "此操作不会修改原 PDF",
      footerMessage: "清空后仍可重新识别、导入或撤销恢复。",
      tone: "danger",
    },
  )) return;
  history.change((items) => items.splice(0));
  status.value = "已清空全部书签";
  setAiActivity("complete", "书签已清空", "可使用撤销恢复", { current: 1, total: 1 });
}

async function confirmExportSettings(target: "目录 PDF" | "带书签 PDF") {
  const unchanged = unchangedExportSettings({
    tocStart: tocStart.value,
    tocEnd: tocEnd.value,
    anchorPrinted: anchorPrinted.value,
    anchorPdf: anchorPdf.value,
  });
  if (!unchanged.length) return true;
  const settingNames = unchanged.join("和");
  return requestConfirmation(
    "确认导出设置",
    `${settingNames}仍为初始的 1–1，尚未修改。确认按当前设置导出${target}吗？`,
    "继续导出",
    {
      subtitle: `${settingNames}尚未修改`,
      footerMessage: "请返回修改未调整的设置，或确认沿用初始值。",
    },
  );
}

function requestConfirmation(
  title: string,
  message: string,
  confirmLabel = "继续导出",
  options: Pick<ConfirmationState, "subtitle" | "footerMessage" | "tone"> = {},
) {
  return new Promise<boolean>((resolve) => {
    confirmation.value = { title, message, confirmLabel, ...options, resolve };
  });
}

function resolveConfirmation(confirmed: boolean) {
  const pending = confirmation.value;
  confirmation.value = undefined;
  pending?.resolve(confirmed);
}

function requestDeleteChoice(title: string, childCount: number) {
  return new Promise<DeleteChoice>((resolve) => {
    deleteConfirmation.value = { title, childCount, resolve };
  });
}

function resolveDeleteChoice(choice: DeleteChoice) {
  const pending = deleteConfirmation.value;
  deleteConfirmation.value = undefined;
  pending?.resolve(choice);
}

function keyboardHistory(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (deleteConfirmation.value) {
      resolveDeleteChoice("cancel");
      return;
    }
    if (confirmation.value) {
      resolveConfirmation(false);
      return;
    }
    if (appInfoOpen.value) {
      appInfoOpen.value = false;
      return;
    }
    if (webImportOpen.value) {
      webImportOpen.value = false;
      return;
    }
    if (apiSettingsOpen.value) {
      apiSettingsOpen.value = false;
      return;
    }
  }
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

function disableSystemContextMenu(event: MouseEvent) {
  event.preventDefault();
}

onMounted(() => {
  window.addEventListener("keydown", keyboardHistory);
  window.addEventListener("contextmenu", disableSystemContextMenu);
});
onBeforeUnmount(async () => {
  window.removeEventListener("keydown", keyboardHistory);
  window.removeEventListener("contextmenu", disableSystemContextMenu);
  window.clearTimeout(promptCopiedTimer);
  window.clearTimeout(tocRangeHintTimer);
  resolveConfirmation(false);
  resolveDeleteChoice("cancel");
  await previewDocument.value?.destroy();
});
</script>

<template>
  <main class="app-shell">
    <header class="topbar">
      <div class="brand">
        <button type="button" class="brand-mark" aria-label="查看书签匠版本信息" title="版本信息" @click="appInfoOpen = true">
          <img :src="appLogo" alt="" />
        </button>
        <h1>书签匠</h1>
      </div>
      <div class="top-actions">
        <span v-if="pdf" class="file-meta">{{ pdf.name }} · {{ pdf.pageCount }} 页</span>
        <button type="button" class="secondary" :disabled="busy" @click="importPdf">{{ pdf ? "更换 PDF" : "导入 PDF" }}</button>
        <button type="button" class="primary" :disabled="busy || !history.items.value.length" @click="exportPdf">导出 PDF</button>
      </div>
    </header>

    <section v-if="pdf" class="setup" aria-label="目录提取设置">
      <fieldset>
        <legend>目录页</legend>
        <label>起始页 <input v-model="tocStartDraft" class="page-number-input" type="number" min="1" :max="pdf.pageCount" @change="commitTocStart" @blur="commitTocStart" @keydown.enter.prevent="($event.target as HTMLInputElement).blur()" @wheel="stepNumberInput" /></label>
        <span>至</span>
        <label>结束页 <input v-model="tocEndDraft" class="page-number-input" type="number" :min="tocStart" :max="pdf.pageCount" @change="commitTocEnd" @blur="commitTocEnd" @keydown.enter.prevent="($event.target as HTMLInputElement).blur()" @wheel="stepNumberInput" /></label>
        <span v-if="tocRangeHint" class="range-hint" role="alert">{{ tocRangeHint }}</span>
        <button type="button" class="primary" :disabled="busy" @click="extractWithVision">AI 解析</button>
        <button type="button" class="secondary" :disabled="busy" @click="exportTocRange">导出目录 PDF</button>
      </fieldset>
      <fieldset>
        <legend>页码映射</legend>
        <label>印刷页 <input v-model="anchorPrinted" class="page-number-input" /></label>
        <span>对应</span>
        <label>PDF 页 <input v-model.number="anchorPdf" class="page-number-input" type="number" min="1" :max="pdf.pageCount" @wheel="stepNumberInput" /></label>
        <button type="button" class="secondary" :disabled="busy || !history.items.value.length" @click="run(applyMappingManually)">应用映射</button>
      </fieldset>
      <div class="setup-actions">
        <button type="button" class="secondary web-import-trigger" :disabled="busy" @click="openWebImport">导入 JSON</button>
        <button
          type="button"
          class="api-settings-trigger"
          :class="{ configured: apiConfigured }"
          :disabled="busy"
          @click="openApiSettings"
        ><span aria-hidden="true"></span>{{ apiConfigured ? "API" : "API 设置" }}</button>
      </div>
    </section>

    <section v-if="pdf" class="workspace">
      <ThumbnailList :key="`thumbnails-${previewRevision}`" :document="previewDocument" :page-count="pdf.pageCount" :current-page="currentPage" @select="currentPage = $event" />
      <PdfPreview
        :key="`preview-${previewRevision}`"
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
        @clear="clearBookmarks"
      />
    </section>

    <section v-else class="welcome">
      <div class="welcome-card">
        <div class="document-outline" aria-hidden="true"><span></span><span></span><span></span></div>
        <h2>为 PDF 添加书签</h2>
        <p>导入、识别、导出。</p>
        <button type="button" class="primary large" :disabled="busy" @click="importPdf">导入 PDF</button>
      </div>
    </section>

    <footer
      class="activity-bar"
      :class="{ failed: error || aiActivity.phase === 'failed', working: busy && aiActivity.phase !== 'idle' }"
      aria-live="polite"
    >
      <div class="activity-message">
        <span class="activity-icon" :class="aiActivity.phase" aria-hidden="true">
          <span v-if="busy && aiActivity.phase !== 'rendering'" class="spinner"></span>
          <span v-else-if="aiActivity.phase === 'complete'">✓</span>
          <span v-else-if="aiActivity.phase === 'failed' || error">!</span>
          <span v-else-if="aiActivity.phase === 'rendering'">{{ progressPercent }}%</span>
          <img v-else :src="appLogo" alt="" />
        </span>
        <div class="activity-copy">
          <strong>{{ activityTitle }}</strong>
          <span v-if="activityDetail">· {{ activityDetail }}</span>
        </div>
      </div>

      <div v-if="busy && ['loading', 'analyzing', 'rendering', 'mapping', 'exporting'].includes(aiActivity.phase)" class="progress-area">
        <div class="progress-track" role="progressbar" :aria-valuenow="aiActivity.indeterminate ? undefined : progressPercent">
          <span
            :class="{ indeterminate: aiActivity.indeterminate }"
            :style="aiActivity.indeterminate ? undefined : { width: `${progressPercent}%` }"
          ></span>
        </div>
        <small>{{ aiActivity.indeterminate ? "处理中" : `${progressPercent}%` }}</small>
      </div>

      <div v-if="lastVision || workflowElapsedMs !== undefined" class="run-metrics">
        <div v-if="lastVision" class="metric" :title="`输入 ${formatTokens(lastVision.usage.inputTokens)} · 输出 ${formatTokens(lastVision.usage.outputTokens)}`">
          <span>Token</span>
          <strong>{{ formatTokens(lastVision.usage.totalTokens) }}</strong>
        </div>
        <div class="metric" :title="lastVision ? `API 请求 ${formatDuration(lastVision.elapsedMs)}` : undefined">
          <span>总用时</span>
          <strong>{{ formatDuration(workflowElapsedMs) }}</strong>
        </div>
        <div v-if="lastVision" class="metric transport">
          <span>识别方式</span>
          <strong>{{ lastVision.transport === "images" ? "高清截图" : "PDF" }}</strong>
        </div>
      </div>
    </footer>

    <Teleport to="body">
      <div v-if="apiSettingsOpen" class="modal-backdrop" @mousedown.self="apiSettingsOpen = false">
        <form class="api-dialog" role="dialog" aria-modal="true" aria-labelledby="api-dialog-title" @submit.prevent="saveApiSettings">
          <header>
            <div>
              <h2 id="api-dialog-title">AI API 设置</h2>
              <p>OpenAI 兼容接口</p>
            </div>
            <button type="button" class="dialog-close" aria-label="关闭 API 设置" @click="apiSettingsOpen = false">×</button>
          </header>
          <div class="dialog-fields">
            <label>
              <span>API URL</span>
              <input v-model="apiDraft.endpoint" autofocus placeholder="https://example.com/v1" />
              <small>填写到 <code>/v1</code></small>
            </label>
            <label>
              <span>模型名称</span>
              <input v-model="apiDraft.model" placeholder="支持 PDF 或视觉输入的模型" />
            </label>
            <label>
              <span>API Key</span>
              <input v-model="apiDraft.key" type="password" autocomplete="off" placeholder="仅保存在本机" />
            </label>
            <div class="web-fallback-card">
              <div>
                <strong>API 暂时不可用？</strong>
                <p>复制提示词，改用网页 AI。</p>
              </div>
              <button type="button" class="secondary" @click="copyWebPrompt">{{ promptCopied ? "已复制" : "复制网页提示词" }}</button>
            </div>
          </div>
          <footer>
            <p>仅处理选定目录页</p>
            <div>
              <button type="button" class="secondary" @click="apiSettingsOpen = false">取消</button>
              <button type="submit" class="primary">保存设置</button>
            </div>
          </footer>
        </form>
      </div>

      <div v-if="webImportOpen" class="modal-backdrop" @mousedown.self="webImportOpen = false">
        <form class="api-dialog web-import-dialog" role="dialog" aria-modal="true" aria-labelledby="web-import-title" @submit.prevent="importWebResult">
          <header>
            <div>
              <h2 id="web-import-title">导入网页 AI 结果</h2>
              <p>PDF 第 {{ tocStart }}–{{ tocEnd }} 页</p>
            </div>
            <button type="button" class="dialog-close" aria-label="关闭导入窗口" @click="webImportOpen = false">×</button>
          </header>
          <div class="web-import-body">
            <label for="web-result">粘贴网页 AI 返回的 JSON</label>
            <textarea
              id="web-result"
              v-model="webImportText"
              autofocus
              spellcheck="false"
              placeholder='按 Ctrl+V 粘贴，例如：{"items":[{"title":"第一章","printedPage":"1","level":0,"sourcePage":1}]}'
            ></textarea>
            <p v-if="webImportError" class="dialog-error" role="alert">{{ webImportError }}</p>
            <small>支持 JSON、JSONL 或 Markdown 代码块，最多 500 条</small>
          </div>
          <footer>
            <p>将替换当前书签</p>
            <div>
              <button type="button" class="secondary" @click="webImportOpen = false">取消</button>
              <button type="submit" class="primary" :disabled="busy || !webImportText.trim()">导入并映射</button>
            </div>
          </footer>
        </form>
      </div>

      <div v-if="appInfoOpen" class="modal-backdrop" @mousedown.self="appInfoOpen = false">
        <section class="api-dialog app-info-dialog" role="dialog" aria-modal="true" aria-labelledby="app-info-title">
          <header>
            <div>
              <h2 id="app-info-title">版本信息</h2>
              <p>书签匠</p>
            </div>
            <button type="button" class="dialog-close" aria-label="关闭版本信息" @click="appInfoOpen = false">×</button>
          </header>
          <div class="app-info-body">
            <img class="app-info-mark" :src="appLogo" alt="" />
            <div class="app-version-summary">
              <span>当前版本</span>
              <strong>{{ APP_VERSION }}</strong>
            </div>
            <dl>
              <div><dt>正式基线</dt><dd>{{ RELEASE_VERSION }}</dd></div>
              <div><dt>开发版本</dt><dd>{{ versionInfo.development }}</dd></div>
              <div><dt>版本通道</dt><dd>{{ VERSION_CHANNEL }}</dd></div>
            </dl>
          </div>
          <footer>
            <p>书签匠 {{ APP_VERSION }}</p>
            <div><button type="button" class="primary" @click="appInfoOpen = false">确定</button></div>
          </footer>
        </section>
      </div>

      <div v-if="confirmation" class="modal-backdrop" @mousedown.self="resolveConfirmation(false)">
        <section class="api-dialog confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-message">
          <header>
            <div>
              <h2 id="confirm-title">{{ confirmation.title }}</h2>
              <p>{{ confirmation.subtitle ?? "请检查当前页码设置" }}</p>
            </div>
            <button type="button" class="dialog-close" aria-label="取消导出" @click="resolveConfirmation(false)">×</button>
          </header>
          <div class="confirm-body">
            <span aria-hidden="true">!</span>
            <p id="confirm-message">{{ confirmation.message }}</p>
          </div>
          <footer>
            <p>{{ confirmation.footerMessage ?? "如有需要，请先修改顶部的目录范围和映射。" }}</p>
            <div>
              <button type="button" class="secondary" @click="resolveConfirmation(false)">返回检查</button>
              <button type="button" class="primary" :class="{ 'danger-confirm': confirmation.tone === 'danger' }" autofocus @click="resolveConfirmation(true)">{{ confirmation.confirmLabel }}</button>
            </div>
          </footer>
        </section>
      </div>

      <div v-if="deleteConfirmation" class="modal-backdrop" @mousedown.self="resolveDeleteChoice('cancel')">
        <section class="api-dialog delete-dialog" role="alertdialog" aria-modal="true" aria-labelledby="delete-title" aria-describedby="delete-message">
          <header>
            <div>
              <h2 id="delete-title">删除父书签</h2>
              <p>包含 {{ deleteConfirmation.childCount }} 条子书签</p>
            </div>
            <button type="button" class="dialog-close" aria-label="取消删除" @click="resolveDeleteChoice('cancel')">×</button>
          </header>
          <div class="confirm-body">
            <span aria-hidden="true">?</span>
            <p id="delete-message">删除“{{ deleteConfirmation.title }}”时，如何处理它的子书签？</p>
          </div>
          <footer class="delete-actions">
            <button type="button" class="secondary" @click="resolveDeleteChoice('cancel')">取消</button>
            <div>
              <button type="button" class="secondary" @click="resolveDeleteChoice('promote')">保留并提升一级</button>
              <button type="button" class="danger-confirm" @click="resolveDeleteChoice('all')">全部删除</button>
            </div>
          </footer>
        </section>
      </div>
    </Teleport>
  </main>
</template>

<style>
:root {
  font-family: Inter, "Segoe UI Variable", "Microsoft YaHei UI", sans-serif;
  color: #281f38;
  background: #f1eef7;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  --text: #281f38;
  --text-muted: #756d84;
  --surface: #ffffff;
  --surface-soft: #f8f6fc;
  --canvas: #ebe7f3;
  --border: #ded8e9;
  --border-soft: #eeeaf4;
  --accent: #6d45c5;
  --accent-hover: #5835aa;
  --accent-soft: #eee8fb;
  --accent-glow: #aa91e1;
  --success: #6942b8;
  --success-soft: #eee8fb;
  --warning: #94600e;
  --warning-soft: #fff2d8;
  --danger: #b43e3e;
  --danger-soft: #fff0ef;
  --shadow-sm: 0 2px 10px rgb(64 44 103 / 9%);
  --shadow-lg: 0 18px 48px rgb(49 32 84 / 18%);
}
* { box-sizing: border-box; }
html, body, #app { width: 100%; height: 100%; overflow: hidden; }
body { margin: 0; min-width: 1040px; min-height: 680px; background: radial-gradient(circle at 15% 0%, #ffffff 0, #f2eff8 48%, #eae5f3 100%); }
button, input, textarea, summary { font: inherit; }
button:focus-visible, input:focus-visible, textarea:focus-visible, summary:focus-visible { outline: 3px solid rgb(109 69 197 / 26%); outline-offset: 2px; }
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-thumb { border: 3px solid transparent; border-radius: 999px; background: #b9afca; background-clip: padding-box; }
::-webkit-scrollbar-track { background: transparent; }
.app-shell { height: 100vh; display: grid; grid-template-rows: 78px auto minmax(0, 1fr) 38px; }
.topbar { grid-row: 1; display: flex; align-items: center; justify-content: space-between; padding: 0 22px; border-bottom: 1px solid rgb(222 216 233 / 88%); background: rgb(255 255 255 / 94%); box-shadow: 0 1px 0 rgb(255 255 255 / 75%), var(--shadow-sm); backdrop-filter: blur(16px); }
.brand { display: flex; gap: 11px; align-items: center; }
.brand-mark { width: 46px; height: 46px; min-height: 46px; padding: 0; display: grid; place-items: center; border: 0; border-radius: 10px; background: transparent; box-shadow: 0 7px 18px rgb(82 48 170 / 20%); }
.brand-mark img { width: 46px; height: 46px; display: block; }
.brand-mark:hover:not(:disabled) { transform: none; box-shadow: 0 9px 24px rgb(82 48 170 / 30%); }
h1 { margin: 0; font-family: SimSun, "Songti SC", serif; font-size: 22px; letter-spacing: 1.5px; }
.top-actions { display: flex; gap: 10px; align-items: center; }
.file-meta { max-width: 430px; overflow: hidden; padding: 7px 10px; border: 1px solid var(--border-soft); border-radius: 5px; background: var(--surface-soft); color: var(--text-muted); font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
button { min-height: 40px; padding: 0 15px; border: 1px solid transparent; border-radius: 6px; font-weight: 650; cursor: pointer; transition: transform 150ms ease, background 160ms ease, border-color 160ms ease, box-shadow 160ms ease; }
button:hover:not(:disabled) { transform: translateY(-1px); }
button.primary { background: linear-gradient(135deg, #7b52ce, #6038b4); box-shadow: 0 5px 14px rgb(109 69 197 / 21%); color: white; }
button.primary:hover:not(:disabled) { background: linear-gradient(135deg, #7047c3, #5230a0); box-shadow: 0 7px 18px rgb(109 69 197 / 29%); }
button.secondary { border-color: var(--border); background: rgb(255 255 255 / 88%); color: var(--text); box-shadow: 0 1px 2px rgb(58 40 93 / 5%); }
button.secondary:hover:not(:disabled) { border-color: #b9a5df; background: var(--accent-soft); }
button:disabled { opacity: .45; cursor: not-allowed; }
.setup { grid-row: 2; display: flex; flex-wrap: wrap; gap: 10px 14px; align-items: center; min-height: 72px; padding: 9px 18px; border-bottom: 1px solid var(--border); background: rgb(247 244 252 / 90%); backdrop-filter: blur(12px); }
fieldset { margin: 0; padding: 8px 9px 8px 13px; border: 1px solid var(--border-soft); border-radius: 6px; display: flex; gap: 9px; align-items: center; background: rgb(255 255 255 / 82%); box-shadow: 0 1px 3px rgb(58 40 93 / 4%); }
legend { float: left; margin-right: 4px; color: var(--text); font-size: 13px; font-weight: 750; letter-spacing: .2px; }
label, fieldset > span { color: var(--text-muted); font-size: 13px; }
input { height: 36px; width: 68px; margin-left: 5px; padding: 0 9px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface); box-shadow: inset 0 1px 2px rgb(58 40 93 / 4%); color: var(--text); font-variant-numeric: tabular-nums; }
input[type="number"] { appearance: textfield; -moz-appearance: textfield; }
input[type="number"]::-webkit-inner-spin-button,
input[type="number"]::-webkit-outer-spin-button { margin: 0; appearance: none; }
input:focus { border-color: #9b7bd8; }
input.page-number-input { width: 52px; padding-right: 6px; padding-left: 6px; }
.setup fieldset { position: relative; }
.range-hint { position: absolute; z-index: 8; top: calc(100% + 4px); right: 8px; padding: 4px 7px; border: 1px solid #e4d8b9; border-radius: 3px; background: rgb(255 250 237 / 97%); box-shadow: var(--shadow-sm); color: #805d1b; font-size: 10px; white-space: nowrap; }
.setup-actions { margin-left: auto; display: flex; gap: 8px; align-items: center; }
.api-settings-trigger { min-height: 40px; display: flex; gap: 7px; align-items: center; border-color: var(--border); background: var(--surface); color: var(--text); box-shadow: var(--shadow-sm); }
.api-settings-trigger > span { width: 7px; height: 7px; border-radius: 50%; background: var(--warning); }
.api-settings-trigger.configured > span { background: var(--success); box-shadow: 0 0 0 3px var(--success-soft); }
.web-import-trigger { white-space: nowrap; }
.setup button { min-height: 36px; }
.workspace { grid-row: 3; min-height: 0; display: grid; grid-template-columns: 178px minmax(430px, 1fr) minmax(420px, 500px); background: var(--border); }
.workspace > * { background-clip: padding-box; }
.welcome { grid-row: 3; display: grid; place-items: center; background: radial-gradient(circle at 50% 35%, #ffffff 0, #eee9f6 58%, #e5dff0 100%); }
.welcome-card { width: min(520px, 70vw); padding: 46px; border: 1px solid rgb(255 255 255 / 78%); border-radius: 10px; background: rgb(255 255 255 / 94%); box-shadow: 0 24px 70px rgb(53 35 90 / 16%); text-align: center; backdrop-filter: blur(14px); }
.document-outline { width: 76px; height: 94px; margin: 0 auto 24px; padding: 26px 15px 0; border: 2px solid var(--accent); border-radius: 4px; box-shadow: 8px 9px 0 var(--accent-soft); }
.document-outline span { display: block; height: 3px; margin: 9px 0; background: var(--accent); opacity: .7; }
.welcome h2 { margin: 0; font-family: SimSun, "Songti SC", serif; font-size: 26px; }
.welcome p { margin: 14px auto 26px; color: var(--text-muted); line-height: 1.7; }
.welcome .large { min-height: 48px; padding: 0 24px; }
.welcome small { display: block; margin-top: 18px; color: var(--text-muted); }
.modal-backdrop { position: fixed; z-index: 100; inset: 0; display: grid; place-items: center; padding: 24px; background: rgb(31 20 51 / 42%); backdrop-filter: blur(5px); animation: modal-in 140ms ease-out; }
.api-dialog { width: min(520px, calc(100vw - 48px)); overflow: hidden; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); box-shadow: 0 28px 80px rgb(37 23 65 / 30%); }
.api-dialog > header { display: flex; align-items: flex-start; justify-content: space-between; padding: 18px 20px 15px; border-bottom: 1px solid var(--border-soft); }
.api-dialog h2 { margin: 0; font-size: 17px; }
.api-dialog header p { margin: 5px 0 0; color: var(--text-muted); font-size: 12px; }
.dialog-close { width: 30px; min-height: 30px; padding: 0; border-color: transparent; background: transparent; color: var(--text-muted); font-size: 21px; font-weight: 400; }
.dialog-close:hover { border-color: var(--border); background: var(--surface-soft); }
.dialog-fields { display: grid; gap: 15px; padding: 18px 20px 20px; }
.dialog-fields label { display: grid; gap: 6px; color: var(--text); font-size: 12px; font-weight: 650; }
.dialog-fields input { width: 100%; height: 39px; margin: 0; font-weight: 400; }
.dialog-fields small { color: var(--text-muted); font-size: 10px; font-weight: 400; }
.dialog-fields code { padding: 1px 3px; border-radius: 2px; background: var(--surface-soft); }
.web-fallback-card { display: flex; gap: 14px; align-items: center; justify-content: space-between; padding: 11px 12px; border: 1px solid #d9cdee; border-radius: 5px; background: var(--accent-soft); }
.web-fallback-card strong { display: block; color: var(--text); font-size: 12px; }
.web-fallback-card p { margin: 3px 0 0; color: var(--text-muted); font-size: 10px; line-height: 1.45; }
.web-fallback-card button { min-width: 116px; min-height: 34px; flex: 0 0 auto; background: rgb(255 255 255 / 78%); }
.api-dialog > footer { display: flex; gap: 16px; align-items: center; justify-content: space-between; padding: 12px 20px; border-top: 1px solid var(--border-soft); background: var(--surface-soft); }
.api-dialog > footer p { max-width: 270px; margin: 0; color: var(--text-muted); font-size: 10px; line-height: 1.45; }
.api-dialog > footer > div { display: flex; gap: 8px; }
.api-dialog > footer button { min-height: 34px; }
.web-import-dialog { width: min(620px, calc(100vw - 48px)); }
.web-import-body { display: grid; gap: 8px; padding: 18px 20px 20px; }
.web-import-body label { color: var(--text); font-size: 12px; font-weight: 700; }
.web-import-body textarea { width: 100%; min-height: 280px; resize: vertical; padding: 11px 12px; border: 1px solid var(--border); border-radius: 5px; background: #fcfbfe; color: var(--text); font-family: Consolas, "Cascadia Mono", monospace; font-size: 11px; line-height: 1.55; }
.web-import-body textarea:focus { border-color: #9b7bd8; }
.web-import-body small { color: var(--text-muted); font-size: 10px; }
.dialog-error { margin: 0; padding: 8px 10px; border: 1px solid #efcdca; border-radius: 4px; background: var(--danger-soft); color: var(--danger); font-size: 11px; line-height: 1.45; }
.app-info-dialog { width: min(440px, calc(100vw - 48px)); }
.app-info-body { display: grid; grid-template-columns: 58px 1fr; gap: 14px; align-items: center; padding: 20px; }
.app-info-mark { width: 58px; height: 58px; display: block; filter: drop-shadow(0 8px 12px rgb(82 48 170 / 22%)); }
.app-version-summary { display: grid; gap: 4px; }
.app-version-summary strong { font-size: 15px; }
.app-version-summary span { color: var(--text-muted); font-size: 11px; }
.app-info-body dl { grid-column: 1 / -1; display: grid; gap: 0; margin: 4px 0 0; border: 1px solid var(--border-soft); border-radius: 5px; }
.app-info-body dl > div { display: grid; grid-template-columns: 88px 1fr; padding: 9px 11px; border-top: 1px solid var(--border-soft); font-size: 11px; }
.app-info-body dl > div:first-child { border-top: 0; }
.app-info-body dt { color: var(--text-muted); }
.app-info-body dd { margin: 0; color: var(--text); }
.confirm-dialog { width: min(470px, calc(100vw - 48px)); }
.confirm-body { display: grid; grid-template-columns: 34px 1fr; gap: 12px; align-items: center; padding: 20px; }
.confirm-body > span { width: 34px; height: 34px; display: grid; place-items: center; border-radius: 5px; background: var(--warning-soft); color: var(--warning); font-size: 18px; font-weight: 800; }
.confirm-body p { margin: 0; color: var(--text); font-size: 13px; line-height: 1.65; }
.confirm-dialog button.danger-confirm { background: #a94b4b; box-shadow: 0 5px 14px rgb(169 75 75 / 18%); }
.confirm-dialog button.danger-confirm:hover:not(:disabled) { background: #913d3d; box-shadow: 0 7px 18px rgb(145 61 61 / 22%); }
.delete-dialog { width: min(500px, calc(100vw - 48px)); }
.delete-dialog .confirm-body > span { background: var(--accent-soft); color: var(--accent); }
.delete-dialog > footer.delete-actions { justify-content: space-between; }
.delete-dialog button.danger-confirm { min-height: 34px; padding: 0 15px; border: 0; border-radius: 6px; background: #a94b4b; box-shadow: 0 5px 14px rgb(169 75 75 / 18%); color: white; font-weight: 650; }
.delete-dialog button.danger-confirm:hover:not(:disabled) { background: #913d3d; box-shadow: 0 7px 18px rgb(145 61 61 / 22%); }
@keyframes modal-in { from { opacity: 0; } to { opacity: 1; } }
.activity-bar { grid-row: 4; min-width: 0; display: grid; grid-template-columns: minmax(280px, 1fr) minmax(180px, .65fr) auto; gap: 12px; align-items: center; padding: 2px 12px; overflow: hidden; border-top: 1px solid var(--border); background: rgb(255 255 255 / 97%); box-shadow: 0 -3px 14px rgb(53 35 90 / 6%); color: var(--text); }
.activity-message { min-width: 0; display: flex; gap: 11px; align-items: center; }
.activity-icon { width: 24px; height: 24px; flex: 0 0 24px; display: grid; place-items: center; border-radius: 4px; background: var(--accent-soft); color: var(--accent); font-size: 11px; font-weight: 800; }
.activity-icon > img { width: 22px; height: 22px; display: block; }
.activity-icon.complete { background: var(--success-soft); color: var(--success); font-family: inherit; font-size: 18px; }
.activity-icon.failed { background: var(--danger-soft); color: var(--danger); font-family: inherit; font-size: 18px; }
.activity-icon.rendering, .activity-icon.loading, .activity-icon.exporting { background: var(--accent-soft); color: var(--accent); font-family: inherit; font-size: 10px; }
.activity-copy { min-width: 0; display: flex; gap: 6px; align-items: center; overflow: hidden; white-space: nowrap; }
.activity-copy strong, .activity-copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.activity-copy strong { flex: 0 0 auto; font-size: 13px; }
.activity-copy span { min-width: 0; color: var(--text-muted); font-size: 12px; }
.progress-area { min-width: 0; display: grid; grid-template-columns: minmax(100px, 1fr) 42px; gap: 9px; align-items: center; }
.progress-track { position: relative; height: 7px; overflow: hidden; border-radius: 999px; background: #ebe6f3; }
.progress-track > span { position: absolute; inset: 0 auto 0 0; border-radius: inherit; background: linear-gradient(90deg, #7048c4, #a27de1); transition: width 220ms ease; }
.progress-track > span.indeterminate { width: 38%; animation: progress-sweep 1.25s ease-in-out infinite; }
.progress-area small { color: var(--text-muted); font-size: 11px; text-align: right; }
.run-metrics { display: flex; gap: 7px; justify-content: flex-end; }
.metric { min-width: 68px; display: flex; gap: 5px; align-items: center; padding: 3px 7px; border: 1px solid var(--border-soft); border-radius: 4px; background: var(--surface-soft); }
.metric span { color: var(--text-muted); font-size: 10px; white-space: nowrap; }
.metric strong { font-size: 12px; font-variant-numeric: tabular-nums; white-space: nowrap; }
.activity-bar.failed { background: linear-gradient(90deg, var(--danger-soft), var(--surface) 38%); }
.spinner { width: 16px; height: 16px; border: 2px solid rgb(109 69 197 / 20%); border-top-color: var(--accent); border-radius: 50%; animation: spin .8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
@keyframes progress-sweep { 0% { left: -38%; } 55% { left: 64%; } 100% { left: 105%; } }
@media (max-width: 1180px) {
  .activity-bar { grid-template-columns: minmax(260px, 1fr) minmax(150px, .5fr) auto; gap: 12px; }
  .metric.transport { display: none; }
  .file-meta { max-width: 260px; }
}
@media (prefers-reduced-motion: reduce) { * { scroll-behavior: auto !important; transition: none !important; } }
</style>
