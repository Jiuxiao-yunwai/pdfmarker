mod models;
mod notification;
mod pdf;
mod toc;
mod vision;

use models::{
    BookmarkItem, ExportRequest, ExportResult, MappingRequest, PageRangeExportRequest,
    PageRangeExportResult, PdfInfo, VisionImagesRequest, VisionRequest, VisionResult,
};
use tauri::AppHandle;

#[tauri::command]
async fn choose_pdf(app: AppHandle) -> Result<Option<PdfInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || pdf::choose_pdf(&app))
        .await
        .map_err(|error| format!("导入任务异常终止：{error}"))?
}

#[tauri::command]
async fn vision_toc(request: VisionRequest) -> Result<VisionResult, String> {
    vision::recognize_toc(request).await
}

#[tauri::command]
async fn vision_toc_images(request: VisionImagesRequest) -> Result<VisionResult, String> {
    vision::recognize_toc_images(request).await
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

#[tauri::command]
async fn export_page_range(
    request: PageRangeExportRequest,
) -> Result<Option<PageRangeExportResult>, String> {
    tauri::async_runtime::spawn_blocking(move || pdf::export_page_range(request))
        .await
        .map_err(|error| format!("目录 PDF 导出任务异常终止：{error}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|_| {
            let _ = notification::register_identity();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            choose_pdf,
            notification::show_app_notification,
            vision_toc,
            vision_toc_images,
            map_bookmarks,
            export_page_range,
            export_pdf
        ])
        .run(tauri::generate_context!())
        .expect("error while running bookmark craftsman");
}
