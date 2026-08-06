import type { BookmarkItem } from "../types";

const MAX_IMPORT_LENGTH = 2 * 1024 * 1024;
const MAX_ITEMS = 500;

export function buildWebTocPrompt(startPage: number, endPage: number) {
  const selectedCount = endPage - startPage + 1;
  return `请识别我随本消息上传的 PDF 或目录页截图中的目录结构。

处理范围：原 PDF 第 ${startPage}–${endPage} 页，共 ${selectedCount} 页。如果上传的是完整原 PDF，只分析这个页码范围；如果上传的是由“书签匠”导出的目录区域 PDF，它的第 1 页对应原 PDF 第 ${startPage} 页；如果上传的是截图，则按上传顺序分别对应原 PDF 第 ${startPage}–${endPage} 页。

要求：
1. 逐项识别全部目录条目，严格保持视觉阅读顺序和标题原文。
2. 正确处理双栏、跨行标题、点线引导符、罗马数字和中文数字。
3. 只返回能形成有效 PDF 书签的目录内容，优先保留带有明确目标页码的条目。没有页码的页眉、页脚、装饰文字和孤立的“目录/Contents”标题不要返回；仅当无页码标题明确统领后续有页码条目、且缺少它会破坏目录层级时，才将它作为父级返回。
4. printedPage 只填写目录中印刷出来的目标页码；没有页码时填写 null，不要计算最终 PDF 跳转页。
5. level 从 0 开始表示书签层级，父子层级每次最多增加 1。
6. sourcePage 使用所选目录页中的相对序号：1 表示原 PDF 第 ${startPage} 页，${selectedCount} 表示原 PDF 第 ${endPage} 页。
7. 只返回一个合法 JSON 对象，不要使用 Markdown 代码块，不要添加解释或其他文字。

严格使用以下格式：
{"items":[{"title":"完整标题","printedPage":"12","level":0,"sourcePage":1}]}`;
}

export function parseWebTocResult(
  input: string,
  startPage: number,
  endPage: number,
): BookmarkItem[] {
  const text = input.trim();
  if (!text) throw new Error("请先粘贴网页 AI 返回的目录 JSON");
  if (text.length > MAX_IMPORT_LENGTH) throw new Error("粘贴内容超过 2 MB，请确认只复制了目录识别结果");
  if (startPage < 1 || endPage < startPage) throw new Error("当前目录页范围无效");

  const jsonText = extractJson(text);
  let value: unknown;
  try {
    value = JSON.parse(jsonText);
  } catch (reason) {
    try {
      const jsonLines = jsonText
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => JSON.parse(line));
      if (!jsonLines.length || !jsonLines.every(isRecord)) throw reason;
      value = jsonLines;
    } catch {
      throw new Error(`无法解析网页 AI 返回的 JSON/JSONL：${String(reason).replace(/^SyntaxError:\s*/, "")}`);
    }
  }

  const rawItems = Array.isArray(value)
    ? value
    : isRecord(value) && Array.isArray(value.items)
      ? value.items
      : undefined;
  if (!rawItems) throw new Error('网页 AI 返回值应为 {"items": [...]}、JSON 数组或 JSONL');
  if (!rawItems.length) throw new Error("网页 AI 没有返回目录条目");
  if (rawItems.length > MAX_ITEMS) throw new Error(`一次最多导入 ${MAX_ITEMS} 条书签`);

  const records = rawItems.filter(isRecord);
  const baseLevel = Math.min(...records.map((entry) => integerValue(entry.level) ?? 0));
  const pageCount = endPage - startPage + 1;
  let previousLevel = 0;
  const items: BookmarkItem[] = [];

  for (const [index, entry] of records.entries()) {
    const title = typeof entry.title === "string" ? entry.title.trim() : "";
    if (!title) continue;

    const rawLevel = Math.max(0, Math.min(8, (integerValue(entry.level) ?? 0) - baseLevel));
    const level = items.length ? Math.min(rawLevel, previousLevel + 1) : 0;
    previousLevel = level;
    const printedPage = pageText(entry.printedPage ?? entry.page);
    const sourcePage = normalizeSourcePage(integerValue(entry.sourcePage), startPage, endPage);

    items.push({
      id: `web-${startPage}-${index}-${crypto.randomUUID()}`,
      title: Array.from(title).slice(0, 200).join(""),
      level,
      printedPage,
      confidence: printedPage ? 0.98 : 0.75,
      sourcePageIndex: startPage + Math.min(sourcePage, pageCount) - 2,
      children: [],
    });
  }

  if (!items.length) throw new Error("没有找到包含有效标题的目录条目");
  if (!items.some((item) => item.printedPage)) {
    throw new Error("返回结果中没有有效的目录页码，请让网页 AI 按提示词重新输出");
  }
  return items;
}

function extractJson(input: string) {
  const fenced = /```(?:json)?\s*([\s\S]*?)```/i.exec(input)?.[1]?.trim() ?? input;
  const itemsObjectStart = fenced.search(/\{\s*"items"\s*:/);
  const objectStart = fenced.indexOf("{");
  const arrayStart = fenced.indexOf("[");
  const starts = (itemsObjectStart >= 0 ? [itemsObjectStart] : [objectStart, arrayStart])
    .filter((index) => index >= 0);
  if (!starts.length) throw new Error("粘贴内容中没有找到 JSON 对象或数组");
  const start = Math.min(...starts);
  const closing = fenced[start] === "{" ? "}" : "]";
  const end = fenced.lastIndexOf(closing);
  if (end < start) throw new Error("网页 AI 返回的 JSON 不完整");
  return fenced.slice(start, end + 1);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function integerValue(value: unknown) {
  const number = typeof value === "number" ? value : typeof value === "string" ? Number(value.trim()) : NaN;
  return Number.isFinite(number) ? Math.max(0, Math.floor(number)) : undefined;
}

function pageText(value: unknown) {
  if (typeof value !== "string" && typeof value !== "number") return undefined;
  const text = String(value).trim();
  return text && text.toLowerCase() !== "null" ? Array.from(text).slice(0, 24).join("") : undefined;
}

function normalizeSourcePage(value: number | undefined, startPage: number, endPage: number) {
  const selectedCount = endPage - startPage + 1;
  if (value !== undefined && value >= 1 && value <= selectedCount) return value;
  if (value !== undefined && value >= startPage && value <= endPage) return value - startPage + 1;
  return 1;
}
