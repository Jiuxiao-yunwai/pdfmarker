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
use tauri_plugin_opener::OpenerExt;

fn parse_external_url(url: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "PDF 链接地址无效".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("只允许打开 HTTP 或 HTTPS 链接".to_string());
    }
    Ok(parsed)
}

#[tauri::command]
fn open_external_url(app: AppHandle, url: String) -> Result<(), String> {
    let parsed = parse_external_url(&url)?;
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|error| format!("无法打开 PDF 链接：{error}"))
}

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
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
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
            export_pdf,
            open_external_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running bookmark craftsman");
}

#[cfg(test)]
mod external_url_tests {
    use super::parse_external_url;

    #[test]
    fn accepts_http_and_https_links() {
        assert!(parse_external_url("https://example.com/manual.pdf#page=3").is_ok());
        assert!(parse_external_url("http://example.com").is_ok());
    }

    #[test]
    fn rejects_non_web_and_invalid_links() {
        assert!(parse_external_url("file:///C:/secret.txt").is_err());
        assert!(parse_external_url("javascript:alert(1)").is_err());
        assert!(parse_external_url("not a url").is_err());
    }
}
