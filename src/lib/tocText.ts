import type { PDFDocumentProxy } from "pdfjs-dist";
import { invoke } from "@tauri-apps/api/core";
import type { TocRawBlock } from "../types";

interface TextRun {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

const COMPLETE_ENTRY = /(?:(?:[.．…⋯‥·•_—–-]\s*)+|\s{2,}|[：:])\s*(?:第\s*)?(?:[PpＳS]\.?\s*)?(?:\d+|[ivxlcdm]+|[一二三四五六七八九十百〇零]+)\s*(?:页)?\s*$/i;
const PAGE_ONLY = /^(?:第\s*)?(?:[PpＳS]\.?\s*)?(?:\d+|[ivxlcdm]+|[一二三四五六七八九十百〇零]+)\s*(?:页)?$/i;

function rowSegments(row: TextRun[], pageWidth: number): string[] {
  const sorted = [...row].sort((a, b) => a.x - b.x);
  const segments: string[] = [];
  let text = "";
  let right = 0;
  for (let index = 0; index < sorted.length; index++) {
    const run = sorted[index];
    const gap = text ? run.x - right : 0;
    const separator = text && PAGE_ONLY.test(run.text.trim())
      ? "  "
      : gap > Math.max(2, run.height * 0.3) ? " " : "";
    text += separator + run.text.trim();
    right = run.x + run.width;
    const nextGap = sorted[index + 1] ? sorted[index + 1].x - right : 0;
    const complete = COMPLETE_ENTRY.test(text) || (PAGE_ONLY.test(run.text.trim()) && text !== run.text.trim());
    if (complete && nextGap > Math.max(18, pageWidth * 0.025)) {
      segments.push(text.trim());
      text = "";
    }
  }
  if (text.trim()) segments.push(text.trim());
  return segments;
}

/** Rebuild visual lines from PDF.js text runs so columns and detached page numbers stay ordered. */
export async function extractLayoutBlocks(document: PDFDocumentProxy, startPage: number, endPage: number) {
  const blocks: TocRawBlock[] = [];
  for (let pageNumber = startPage; pageNumber <= endPage; pageNumber++) {
    const page = await document.getPage(pageNumber);
    const viewport = page.getViewport({ scale: 1 });
    const content = await page.getTextContent();
    const runs: TextRun[] = content.items.flatMap((item) => {
      if (!("str" in item) || !item.str.trim()) return [];
      return [{
        text: item.str,
        x: Number(item.transform[4]),
        y: Number(item.transform[5]),
        width: Number(item.width),
        height: Math.max(1, Number(item.height) || Math.abs(Number(item.transform[3]))),
      }];
    });
    runs.sort((a, b) => b.y - a.y || a.x - b.x);
    const rows: TextRun[][] = [];
    for (const run of runs) {
      const row = rows.find((candidate) => Math.abs(candidate[0].y - run.y) <= Math.max(2, run.height * 0.45));
      row ? row.push(run) : rows.push([run]);
    }
    rows.sort((a, b) => b[0].y - a[0].y);
    for (const row of rows) {
      for (const text of rowSegments(row, viewport.width)) {
        const left = Math.min(...row.map((run) => run.x));
        blocks.push({
          text,
          pageIndex: pageNumber - 1,
          x: left,
          y: row[0].y,
          width: Math.max(...row.map((run) => run.x + run.width)) - left,
          height: Math.max(...row.map((run) => run.height)),
          confidence: 1,
        });
      }
    }
  }
  return blocks;
}

/** Render selected pages at OCR resolution and recognize them with the native Windows engine. */
export async function extractOcrBlocks(
  document: PDFDocumentProxy,
  startPage: number,
  endPage: number,
  onPage: (page: number) => void,
) {
  const blocks: TocRawBlock[] = [];
  for (let pageNumber = startPage; pageNumber <= endPage; pageNumber++) {
    onPage(pageNumber);
    const page = await document.getPage(pageNumber);
    const base = page.getViewport({ scale: 1 });
    const scale = Math.min(2.5, 2400 / Math.max(base.width, base.height));
    const viewport = page.getViewport({ scale });
    const canvas = globalThis.document.createElement("canvas");
    canvas.width = Math.ceil(viewport.width);
    canvas.height = Math.ceil(viewport.height);
    const context = canvas.getContext("2d");
    if (!context) throw new Error("无法创建 OCR 画布");
    await page.render({ canvas, viewport, background: "white" }).promise;
    const pngBase64 = canvas.toDataURL("image/png").split(",", 2)[1];
    blocks.push(...await invoke<TocRawBlock[]>("ocr_page", { pngBase64, pageIndex: pageNumber - 1 }));
  }
  return blocks;
}
