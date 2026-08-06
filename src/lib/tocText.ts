import { invoke } from "@tauri-apps/api/core";
import type { PDFDocumentProxy } from "pdfjs-dist";
import type { VisionResult } from "../types";

export interface VisionConfig {
  endpoint: string;
  apiKey: string;
  model: string;
}

/** Send one selected PDF page range to a PDF-capable Responses API. */
export function extractVisionItems(
  inputPath: string,
  startPage: number,
  endPage: number,
  config: VisionConfig,
) {
  return invoke<VisionResult>("vision_toc", {
    request: { ...config, inputPath, startPage, endPage },
  });
}

/** Render the selected pages locally and use image input when PDF input is unavailable. */
export async function extractVisionImageItems(
  document: PDFDocumentProxy,
  startPage: number,
  endPage: number,
  config: VisionConfig,
  onProgress: (progress: { phase: "rendering" | "analyzing"; completed: number; total: number; page?: number }) => void,
) {
  if (endPage - startPage + 1 > 20) {
    throw new Error("高清截图降级模式一次最多处理 20 页，请缩小目录页范围");
  }
  const images: string[] = [];
  let totalLength = 0;
  const total = endPage - startPage + 1;
  onProgress({ phase: "rendering", completed: 0, total });
  for (let pageNumber = startPage; pageNumber <= endPage; pageNumber++) {
    const image = await renderPagePng(document, pageNumber, 3000);
    if (image.length > 16 * 1024 * 1024) throw new Error(`第 ${pageNumber} 页高清截图过大`);
    totalLength += image.length;
    if (totalLength > 64 * 1024 * 1024) throw new Error("高清目录截图总大小过大，请缩小页码范围");
    images.push(image);
    onProgress({ phase: "rendering", completed: images.length, total, page: pageNumber });
  }
  onProgress({ phase: "analyzing", completed: total, total });
  return invoke<VisionResult>("vision_toc_images", {
    request: { ...config, images, startPage, endPage },
  });
}

async function renderPagePng(document: PDFDocumentProxy, pageNumber: number, longEdge: number) {
  const page = await document.getPage(pageNumber);
  const base = page.getViewport({ scale: 1 });
  const viewport = page.getViewport({ scale: longEdge / Math.max(base.width, base.height) });
  const canvas = globalThis.document.createElement("canvas");
  canvas.width = Math.ceil(viewport.width);
  canvas.height = Math.ceil(viewport.height);
  const context = canvas.getContext("2d");
  if (!context) throw new Error("无法创建高清目录截图画布");
  await page.render({ canvas, canvasContext: context, viewport, background: "white" }).promise;
  return canvas.toDataURL("image/png").split(",", 2)[1];
}
