use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfInfo {
    pub path: String,
    pub name: String,
    pub page_count: u32,
    pub document_kind: String,
    pub existing_bookmarks: Vec<BookmarkItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkItem {
    pub id: String,
    pub title: String,
    pub level: u32,
    pub printed_page: Option<String>,
    pub pdf_page: Option<u32>,
    pub confidence: f32,
    pub source_page_index: u32,
    #[serde(default)]
    pub children: Vec<BookmarkItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionRequest {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub input_path: String,
    pub start_page: u32,
    pub end_page: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionImagesRequest {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub images: Vec<String>,
    pub start_page: u32,
    pub end_page: u32,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionResult {
    pub items: Vec<BookmarkItem>,
    pub usage: VisionUsage,
    pub elapsed_ms: u64,
    pub transport: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingRequest {
    pub items: Vec<BookmarkItem>,
    pub anchor_printed: String,
    pub anchor_pdf: u32,
    pub page_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub input_path: String,
    pub items: Vec<BookmarkItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub output_path: String,
    pub bookmark_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRangeExportRequest {
    pub input_path: String,
    pub start_page: u32,
    pub end_page: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRangeExportResult {
    pub output_path: String,
    pub page_count: u32,
}
