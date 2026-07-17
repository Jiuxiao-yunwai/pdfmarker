mod models;
mod pdf;
mod toc;

use models::{BookmarkItem, ExportRequest, ExportResult, MappingRequest, PdfInfo, TocExtraction};
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
            map_bookmarks,
            export_pdf
        ])
        .run(tauri::generate_context!())
        .expect("error while running bookmark craftsman");
}
