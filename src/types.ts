export interface PdfInfo {
  path: string;
  name: string;
  pageCount: number;
  documentKind: "文本型" | "扫描型" | "混合型";
}

export interface TocRawBlock {
  text: string;
  pageIndex: number;
  x: number;
  y: number;
  width: number;
  height: number;
  fontSize?: number;
  confidence?: number;
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

export interface TocExtraction {
  blocks: TocRawBlock[];
  items: BookmarkItem[];
}

export interface ExportResult {
  outputPath: string;
  bookmarkCount: number;
}
