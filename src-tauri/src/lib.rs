mod models;
mod ocr;
mod pdf;
mod toc;

use models::{
    BookmarkItem, ExportRequest, ExportResult, MappingRequest, PdfInfo, TocExtraction, TocRawBlock,
};
use tauri::AppHandle;

#[tauri::command]
async fn choose_pdf(app: AppHandle) -> Result<Option<PdfInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || pdf::choose_pdf(&app))
        .await
        .map_err(|error| format!("导入任务异常终止：{error}"))?
}

#[tauri::command]
async fn extract_toc(
    path: String,
    start_page: u32,
    end_page: u32,
) -> Result<TocExtraction, String> {
    tauri::async_runtime::spawn_blocking(move || pdf::extract_toc(&path, start_page, end_page))
        .await
        .map_err(|error| format!("提取任务异常终止：{error}"))?
}

#[tauri::command]
async fn ocr_page(png_base64: String, page_index: u32) -> Result<Vec<TocRawBlock>, String> {
    tauri::async_runtime::spawn_blocking(move || ocr::recognize_png(&png_base64, page_index))
        .await
        .map_err(|error| format!("OCR 任务异常终止：{error}"))?
}

#[tauri::command]
fn parse_toc_blocks(blocks: Vec<TocRawBlock>) -> Result<TocExtraction, String> {
    let items = toc::parse_blocks(&blocks);
    if items.is_empty() {
        return Err("没有识别到目录条目，请检查目录页范围".to_owned());
    }
    toc::validate_candidate(&items)?;
    Ok(TocExtraction { blocks, items })
}

#[tauri::command]
fn map_bookmarks(request: MappingRequest) -> Result<Vec<BookmarkItem>, String> {
    let mut items = request.items;
    toc::apply_single_anchor(
        &mut items,
        &request.anchor_printed,
        request.anchor_pdf,
        request.page_count,
    )?;
    Ok(items)
}

#[tauri::command]
async fn export_pdf(request: ExportRequest) -> Result<Option<ExportResult>, String> {
    tauri::async_runtime::spawn_blocking(move || pdf::export_pdf(request))
        .await
        .map_err(|error| format!("导出任务异常终止：{error}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            choose_pdf,
            extract_toc,
            ocr_page,
            parse_toc_blocks,
            map_bookmarks,
            export_pdf
        ])
        .run(tauri::generate_context!())
        .expect("error while running bookmark craftsman");
}
