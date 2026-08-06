export interface PdfInfo {
  path: string;
  name: string;
  pageCount: number;
  documentKind: "文本型" | "扫描型" | "混合型";
  existingBookmarks: BookmarkItem[];
}

export interface BookmarkItem {
  id: string;
  title: string;
  level: number;
  printedPage?: string;
  pdfPage?: number;
  confidence: number;
  sourcePageIndex: number;
  children: BookmarkItem[];
}

export interface ExportResult {
  outputPath: string;
  bookmarkCount: number;
}

export interface PageRangeExportResult {
  outputPath: string;
  pageCount: number;
}

export interface VisionUsage {
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
}

export interface VisionResult {
  items: BookmarkItem[];
  usage: VisionUsage;
  elapsedMs: number;
  transport: "pdf" | "images";
}
