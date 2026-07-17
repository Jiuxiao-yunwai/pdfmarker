use crate::models::TocRawBlock;

const MAX_PNG_BASE64: usize = 32 * 1024 * 1024;

#[cfg(target_os = "windows")]
pub fn recognize_png(png_base64: &str, page_index: u32) -> Result<Vec<TocRawBlock>, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use windows::{
        Graphics::Imaging::BitmapDecoder,
        Media::Ocr::OcrEngine,
        Storage::Streams::{DataWriter, InMemoryRandomAccessStream},
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    };

    if png_base64.len() > MAX_PNG_BASE64 {
        return Err("OCR 页面图像过大，请缩小目录页范围后重试".to_owned());
    }
    let bytes = STANDARD
        .decode(png_base64)
        .map_err(|error| format!("OCR 页面图像无效：{error}"))?;
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map_err(|error| format!("无法初始化 Windows OCR：{error}"))?;
    struct RuntimeApartment;
    impl Drop for RuntimeApartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    let _apartment = RuntimeApartment;
    let stream = InMemoryRandomAccessStream::new().map_err(ocr_error)?;
    let writer = DataWriter::CreateDataWriter(&stream).map_err(ocr_error)?;
    writer.WriteBytes(&bytes).map_err(ocr_error)?;
    writer
        .StoreAsync()
        .map_err(ocr_error)?
        .get()
        .map_err(ocr_error)?;
    writer
        .FlushAsync()
        .map_err(ocr_error)?
        .get()
        .map_err(ocr_error)?;
    stream.Seek(0).map_err(ocr_error)?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(ocr_error)?
        .get()
        .map_err(ocr_error)?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(ocr_error)?
        .get()
        .map_err(ocr_error)?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(|_| {
        "Windows 没有可用的 OCR 语言包，请在系统语言设置中安装中文（简体）后重试".to_owned()
    })?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(ocr_error)?
        .get()
        .map_err(ocr_error)?;
    let lines = result.Lines().map_err(ocr_error)?;
    let mut blocks = Vec::new();
    for index in 0..lines.Size().map_err(ocr_error)? {
        let line = lines.GetAt(index).map_err(ocr_error)?;
        let words = line.Words().map_err(ocr_error)?;
        if words.Size().map_err(ocr_error)? == 0 {
            continue;
        }
        let mut left = f32::MAX;
        let mut top = f32::MAX;
        let mut right = 0.0_f32;
        let mut bottom = 0.0_f32;
        for word_index in 0..words.Size().map_err(ocr_error)? {
            let bounds = words
                .GetAt(word_index)
                .map_err(ocr_error)?
                .BoundingRect()
                .map_err(ocr_error)?;
            left = left.min(bounds.X);
            top = top.min(bounds.Y);
            right = right.max(bounds.X + bounds.Width);
            bottom = bottom.max(bounds.Y + bounds.Height);
        }
        let text = line.Text().map_err(ocr_error)?.to_string();
        if !text.trim().is_empty() {
            blocks.push(TocRawBlock {
                text,
                page_index,
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
                font_size: None,
                confidence: None,
            });
        }
    }
    Ok(blocks)
}

#[cfg(target_os = "windows")]
fn ocr_error(error: windows::core::Error) -> String {
    format!("Windows OCR 失败：{error}")
}

#[cfg(not(target_os = "windows"))]
pub fn recognize_png(_: &str, _: u32) -> Result<Vec<TocRawBlock>, String> {
    Err("系统 OCR 当前仅支持 Windows".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_ocr_images() {
        let oversized = "x".repeat(MAX_PNG_BASE64 + 1);
        assert!(recognize_png(&oversized, 0)
            .unwrap_err()
            .contains("图像过大"));
    }
}
