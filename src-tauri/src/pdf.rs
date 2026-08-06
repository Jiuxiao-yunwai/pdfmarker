use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use lopdf::{Bookmark, Document, Object};
use tauri::{AppHandle, Manager};

use crate::models::{
    BookmarkItem, ExportRequest, ExportResult, PageRangeExportRequest, PageRangeExportResult,
    PdfInfo,
};

const PAGE_TEXT_LIMIT: usize = 16 * 1024 * 1024;

pub fn choose_pdf(app: &AppHandle) -> Result<Option<PdfInfo>, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("PDF 文档", &["pdf"])
        .pick_file()
    else {
        return Ok(None);
    };
    let document = load_pdf(&path)?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| format!("无法授权 PDF 预览：{error}"))?;

    let pages = document.get_pages();
    let sampled: Vec<u32> = pages.keys().copied().take(12).collect();
    let text_pages = sampled
        .iter()
        .filter(|page| {
            document
                .extract_text_with_limit(&[**page], PAGE_TEXT_LIMIT)
                .map(|text| text.chars().filter(|c| !c.is_whitespace()).count() > 20)
                .unwrap_or(false)
        })
        .count();
    let document_kind = match (text_pages, sampled.len()) {
        (0, _) => "扫描型",
        (text, total) if text == total => "文本型",
        _ => "混合型",
    };
    let existing_bookmarks = read_existing_bookmarks(&document);

    Ok(Some(PdfInfo {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("未命名.pdf")
            .to_owned(),
        page_count: pages.len() as u32,
        document_kind: document_kind.to_owned(),
        existing_bookmarks,
    }))
}

fn read_existing_bookmarks(document: &Document) -> Vec<BookmarkItem> {
    document
        .get_toc()
        .map(|toc| {
            toc.toc
                .into_iter()
                .enumerate()
                .map(|(index, item)| BookmarkItem {
                    id: format!("existing-{index}"),
                    title: item.title,
                    level: item.level.saturating_sub(1) as u32,
                    printed_page: None,
                    pdf_page: Some(item.page as u32),
                    confidence: 1.0,
                    source_page_index: item.page.saturating_sub(1) as u32,
                    children: Vec::new(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn selected_page_range(path: &str, start_page: u32, end_page: u32) -> Result<Vec<u8>, String> {
    let mut document = load_pdf(Path::new(path))?;
    let page_count = document.get_pages().len() as u32;
    if start_page == 0 || start_page > end_page || end_page > page_count {
        return Err(format!("目录页范围必须在 1 到 {page_count} 之间"));
    }

    let removed = (1..=page_count)
        .filter(|page| *page < start_page || *page > end_page)
        .collect::<Vec<_>>();
    document.delete_pages(&removed);
    if let Ok(catalog) = document.catalog_mut() {
        catalog.remove(b"Outlines");
    }
    document.bookmarks.clear();
    document.bookmark_table.clear();
    document.prune_objects();
    document.compress();
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .map_err(|error| format!("无法生成目录页 PDF：{error}"))?;
    Ok(bytes)
}

pub fn export_page_range(
    request: PageRangeExportRequest,
) -> Result<Option<PageRangeExportResult>, String> {
    let input = checked_pdf_path(Path::new(&request.input_path))?;
    let file_stem = input
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("book");
    let bytes = selected_page_range(
        input.to_string_lossy().as_ref(),
        request.start_page,
        request.end_page,
    )?;
    let Some(output) = rfd::FileDialog::new()
        .add_filter("PDF 文档", &["pdf"])
        .set_file_name(format!(
            "{file_stem}_toc_{}-{}.pdf",
            request.start_page, request.end_page
        ))
        .save_file()
    else {
        return Ok(None);
    };
    let output = ensure_pdf_extension(output);
    let output = output.canonicalize().unwrap_or_else(|_| output.clone());
    if output == input {
        return Err("禁止覆盖原始 PDF，请选择新的输出文件名".to_owned());
    }
    if output.exists() {
        return Err("输出文件已存在，请换一个文件名以避免意外覆盖".to_owned());
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let temporary = output.with_file_name(format!(".{file_stem}.toc.{nonce}.part"));
    std::fs::write(&temporary, bytes).map_err(|error| format!("临时 PDF 写入失败：{error}"))?;
    if let Err(error) = std::fs::rename(&temporary, &output) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("目录 PDF 写入失败：{error}"));
    }

    Ok(Some(PageRangeExportResult {
        output_path: output.to_string_lossy().into_owned(),
        page_count: request.end_page - request.start_page + 1,
    }))
}

pub fn export_pdf(request: ExportRequest) -> Result<Option<ExportResult>, String> {
    let input = checked_pdf_path(Path::new(&request.input_path))?;
    let file_stem = input
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("book");
    let Some(output) = rfd::FileDialog::new()
        .add_filter("PDF 文档", &["pdf"])
        .set_file_name(format!("{file_stem}_bookmarked.pdf"))
        .save_file()
    else {
        return Ok(None);
    };
    let output = ensure_pdf_extension(output);
    let output = output.canonicalize().unwrap_or_else(|_| output.clone());
    if output == input {
        return Err("禁止覆盖原始 PDF，请选择新的输出文件名".to_owned());
    }
    if output.exists() {
        return Err("输出文件已存在，请换一个文件名以避免意外覆盖".to_owned());
    }

    let mut document = load_pdf(&input)?;
    write_outline(&mut document, &request.items)?;
    document.compress();

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let temporary = output.with_file_name(format!(".{file_stem}.{nonce}.part"));
    document
        .save(&temporary)
        .map_err(|error| format!("临时 PDF 写入失败：{error}"))?;
    if let Err(error) = std::fs::rename(&temporary, &output) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("输出文件写入失败：{error}"));
    }

    Ok(Some(ExportResult {
        output_path: output.to_string_lossy().into_owned(),
        bookmark_count: request.items.len(),
    }))
}

fn write_outline(document: &mut Document, items: &[BookmarkItem]) -> Result<(), String> {
    let pages = document.get_pages();
    validate_bookmarks(items, pages.len() as u32)?;
    if let Ok(catalog) = document.catalog_mut() {
        catalog.remove(b"Outlines");
    }
    document.bookmarks.clear();
    document.bookmark_table.clear();
    document.max_bookmark_id = 0;

    let mut parent_stack = Vec::new();
    for item in items {
        parent_stack.truncate(item.level as usize);
        let parent = item
            .level
            .checked_sub(1)
            .and_then(|level| parent_stack.get(level as usize))
            .copied();
        let page = pages[&item.pdf_page.expect("validated bookmark page")];
        let id = document.add_bookmark(
            Bookmark::new(item.title.trim().to_owned(), [0.16, 0.20, 0.18], 0, page),
            parent,
        );
        parent_stack.push(id);
    }
    if let Some(outline) = document.build_outline() {
        document
            .catalog_mut()
            .map_err(|error| format!("PDF 目录结构无效：{error}"))?
            .set("Outlines", Object::Reference(outline));
    }
    Ok(())
}

fn load_pdf(path: &Path) -> Result<Document, String> {
    let path = checked_pdf_path(path)?;
    let document = Document::load(&path).map_err(|error| format!("无法打开 PDF：{error}"))?;
    if document.is_encrypted() {
        return Err("PDF 已加密，MVP 暂不支持密码文档".to_owned());
    }
    Ok(document)
}

fn checked_pdf_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        return Err("请选择有效的 PDF 文件".to_owned());
    }
    path.canonicalize()
        .map_err(|error| format!("无法读取 PDF 路径：{error}"))
}

fn ensure_pdf_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        path
    } else {
        path.with_extension("pdf")
    }
}

fn validate_bookmarks(items: &[BookmarkItem], page_count: u32) -> Result<(), String> {
    if items.is_empty() {
        return Err("没有可导出的书签".to_owned());
    }
    let mut previous_level = 0;
    for (index, item) in items.iter().enumerate() {
        if item.title.trim().is_empty() {
            return Err(format!("第 {} 条书签标题为空", index + 1));
        }
        let page = item
            .pdf_page
            .ok_or_else(|| format!("“{}”尚未映射到 PDF 页", item.title))?;
        if page == 0 || page > page_count {
            return Err(format!(
                "“{}”的目标页超出 1 到 {page_count} 范围",
                item.title
            ));
        }
        if index == 0 && item.level != 0 || item.level > previous_level + 1 {
            return Err(format!("“{}”的层级跳跃不合法", item.title));
        }
        previous_level = item.level;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn test_document(page_count: u32) -> Document {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let mut page_ids = Vec::new();
        for _ in 0..page_count {
            page_ids.push(document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Resources" => dictionary! {},
            }));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => i64::from(page_count),
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    #[test]
    fn rejects_unmapped_and_jumping_bookmarks() {
        let item = BookmarkItem {
            id: "1".into(),
            title: "第一章".into(),
            level: 2,
            printed_page: Some("1".into()),
            pdf_page: None,
            confidence: 1.0,
            source_page_index: 0,
            children: vec![],
        };
        assert!(validate_bookmarks(&[item], 10).is_err());
    }

    #[test]
    fn builds_a_real_pdf_outline() {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! {},
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);

        let item = BookmarkItem {
            id: "1".into(),
            title: "Introduction".into(),
            level: 0,
            printed_page: Some("1".into()),
            pdf_page: Some(1),
            confidence: 1.0,
            source_page_index: 0,
            children: vec![],
        };
        write_outline(&mut document, &[item]).unwrap();
        assert!(document.catalog().unwrap().get(b"Outlines").is_ok());
        let imported = read_existing_bookmarks(&document);
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].title, "Introduction");
        assert_eq!(imported[0].pdf_page, Some(1));
    }

    #[test]
    fn creates_a_pdf_containing_only_the_selected_pages() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("pdfmarker-range-{nonce}.pdf"));
        test_document(4).save(&path).unwrap();

        let bytes = selected_page_range(path.to_str().unwrap(), 2, 3).unwrap();
        let selected = Document::load_mem(&bytes).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(selected.get_pages().len(), 2);
    }
}
