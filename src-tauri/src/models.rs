use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfInfo {
    pub path: String,
    pub name: String,
    pub page_count: u32,
    pub document_kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TocRawBlock {
    pub text: String,
    pub page_index: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font_size: Option<f32>,
    pub confidence: Option<f32>,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TocExtraction {
    pub blocks: Vec<TocRawBlock>,
    pub items: Vec<BookmarkItem>,
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
