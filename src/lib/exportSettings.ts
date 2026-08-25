export interface ExportSettings {
  tocStart: number;
  tocEnd: number;
  anchorPrinted: string;
  anchorPdf: number;
}

export type UnchangedExportSetting = "目录页" | "页码映射";

export function unchangedExportSettings(settings: ExportSettings): UnchangedExportSetting[] {
  const unchanged: UnchangedExportSetting[] = [];
  if (settings.tocStart === 1 && settings.tocEnd === 1) unchanged.push("目录页");
  if (settings.anchorPrinted.trim() === "1" && settings.anchorPdf === 1) unchanged.push("页码映射");
  return unchanged;
}
