use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Url;
use serde_json::{json, Value};

use crate::{
    models::{BookmarkItem, VisionImagesRequest, VisionRequest, VisionResult, VisionUsage},
    pdf,
};

const MAX_PDF_BYTES: usize = 45 * 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 3;
const MAX_ITEMS: usize = 500;
const MAX_IMAGE_PAGES: usize = 20;
const MAX_IMAGE_BASE64: usize = 16 * 1024 * 1024;
const MAX_IMAGES_BASE64: usize = 64 * 1024 * 1024;

pub async fn recognize_toc(request: VisionRequest) -> Result<VisionResult, String> {
    let started = Instant::now();
    validate_config(&request.endpoint, &request.api_key, &request.model)?;
    let endpoint = responses_endpoint(&request.endpoint)?;
    let path = request.input_path.clone();
    let start_page = request.start_page;
    let end_page = request.end_page;
    let pdf_bytes = tauri::async_runtime::spawn_blocking(move || {
        pdf::selected_page_range(&path, start_page, end_page)
    })
    .await
    .map_err(|error| format!("目录页整理任务异常终止：{error}"))??;
    if pdf_bytes.len() > MAX_PDF_BYTES {
        return Err(format!(
            "所选目录页生成的 PDF 超过 45 MB，请缩小页码范围后重试（当前约 {:.1} MB）",
            pdf_bytes.len() as f64 / 1024.0 / 1024.0
        ));
    }

    let body = request_body(&request, &pdf_bytes);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| format!("无法创建 API 客户端：{error}"))?;
    let mut last_error = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        let response = match client
            .post(endpoint.clone())
            .bearer_auth(request.api_key.trim())
            .json(&body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("多模态 API 请求失败：{error}");
                retry_delay(attempt).await;
                continue;
            }
        };
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES)
        {
            return Err("多模态 API 返回内容过大".to_owned());
        }
        let response_text = match response.text().await {
            Ok(text) => text,
            Err(error) => {
                last_error = format!("无法读取 API 响应：{error}");
                retry_delay(attempt).await;
                continue;
            }
        };
        if response_text.len() > MAX_RESPONSE_BYTES as usize {
            return Err("多模态 API 返回内容过大".to_owned());
        }
        if !status.is_success() {
            let message = api_error_message(&response_text);
            if pdf_input_unsupported(status, &message) {
                return Err(format!(
                    "PDF_INPUT_UNSUPPORTED：模型或服务不支持 PDF 文件输入，准备改用高清截图：{message}"
                ));
            }
            last_error = format!("多模态 API 返回 {status}：{message}");
            let retryable = status.is_server_error()
                || status == reqwest::StatusCode::REQUEST_TIMEOUT
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
            if !retryable {
                return Err(last_error);
            }
        } else {
            match parse_response(&response_text, request.start_page, request.end_page) {
                Ok((items, false)) => {
                    return Ok(VisionResult {
                        items,
                        usage: parse_usage(&response_text, false),
                        elapsed_ms: elapsed_ms(started),
                        transport: "pdf".to_owned(),
                    })
                }
                Ok((_, true)) => last_error = "模型输出未完整结束".to_owned(),
                Err(error) => last_error = error,
            }
        }
        retry_delay(attempt).await;
    }
    Err(format!("{last_error}（已自动尝试 {MAX_ATTEMPTS} 次）"))
}

pub async fn recognize_toc_images(request: VisionImagesRequest) -> Result<VisionResult, String> {
    let started = Instant::now();
    validate_config(&request.endpoint, &request.api_key, &request.model)?;
    if request.start_page == 0 || request.start_page > request.end_page {
        return Err("高清截图的目录页范围无效".to_owned());
    }
    let expected = (request.end_page - request.start_page + 1) as usize;
    if request.images.len() != expected {
        return Err("高清截图数量与目录页范围不一致".to_owned());
    }
    if request.images.is_empty() || request.images.len() > MAX_IMAGE_PAGES {
        return Err(format!(
            "高清截图降级模式一次最多处理 {MAX_IMAGE_PAGES} 页，请缩小目录页范围"
        ));
    }
    if request
        .images
        .iter()
        .any(|image| image.len() > MAX_IMAGE_BASE64)
        || request.images.iter().map(String::len).sum::<usize>() > MAX_IMAGES_BASE64
    {
        return Err("高清目录截图数据过大，请缩小目录页范围".to_owned());
    }

    let endpoint = chat_completions_endpoint(&request.endpoint)?;
    let body = image_request_body(&request);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| format!("无法创建 API 客户端：{error}"))?;
    let mut last_error = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        let response = match client
            .post(endpoint.clone())
            .bearer_auth(request.api_key.trim())
            .json(&body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                last_error = format!("高清截图 API 请求失败：{error}");
                retry_delay(attempt).await;
                continue;
            }
        };
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|size| size > MAX_RESPONSE_BYTES)
        {
            return Err("多模态 API 返回内容过大".to_owned());
        }
        let response_text = match response.text().await {
            Ok(text) => text,
            Err(error) => {
                last_error = format!("无法读取高清截图 API 响应：{error}");
                retry_delay(attempt).await;
                continue;
            }
        };
        if response_text.len() > MAX_RESPONSE_BYTES as usize {
            return Err("多模态 API 返回内容过大".to_owned());
        }
        if !status.is_success() {
            let message = api_error_message(&response_text);
            last_error = format!("高清截图 API 返回 {status}：{message}");
            let retryable = status.is_server_error()
                || status == reqwest::StatusCode::REQUEST_TIMEOUT
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
            if !retryable {
                return Err(last_error);
            }
        } else {
            match parse_chat_response(&response_text, request.start_page, request.end_page) {
                Ok((items, false)) => {
                    return Ok(VisionResult {
                        items,
                        usage: parse_usage(&response_text, true),
                        elapsed_ms: elapsed_ms(started),
                        transport: "images".to_owned(),
                    })
                }
                Ok((_, true)) => last_error = "模型输出达到长度上限".to_owned(),
                Err(error) => last_error = error,
            }
        }
        retry_delay(attempt).await;
    }
    Err(format!("{last_error}（已自动尝试 {MAX_ATTEMPTS} 次）"))
}

fn validate_config(endpoint: &str, api_key: &str, model: &str) -> Result<(), String> {
    if endpoint.len() > 2048 || api_key.len() > 8192 || model.len() > 200 {
        return Err("API 配置内容过长".to_owned());
    }
    if api_key.trim().is_empty() || model.trim().is_empty() {
        return Err("请填写 API Key 和模型名".to_owned());
    }
    Ok(())
}

fn responses_endpoint(value: &str) -> Result<Url, String> {
    let mut endpoint = Url::parse(value.trim())
        .map_err(|_| "API URL 无效，请填写完整的 http:// 或 https:// 地址".to_owned())?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err("API URL 仅支持 http:// 或 https://".to_owned());
    }
    let path = endpoint.path().trim_end_matches('/').to_owned();
    if path.ends_with("/v1") {
        endpoint.set_path(&format!("{path}/responses"));
    } else if path.ends_with("/chat/completions") {
        endpoint.set_path(&format!(
            "{}/responses",
            path.trim_end_matches("/chat/completions")
        ));
    } else if !path.ends_with("/responses") {
        return Err("API URL 请填写到 /v1，或填写完整的 /responses 地址".to_owned());
    }
    Ok(endpoint)
}

fn chat_completions_endpoint(value: &str) -> Result<Url, String> {
    let mut endpoint = Url::parse(value.trim())
        .map_err(|_| "API URL 无效，请填写完整的 http:// 或 https:// 地址".to_owned())?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err("API URL 仅支持 http:// 或 https://".to_owned());
    }
    let path = endpoint.path().trim_end_matches('/').to_owned();
    if path.ends_with("/v1") {
        endpoint.set_path(&format!("{path}/chat/completions"));
    } else if path.ends_with("/responses") {
        endpoint.set_path(&format!(
            "{}/chat/completions",
            path.trim_end_matches("/responses")
        ));
    } else if !path.ends_with("/chat/completions") {
        return Err("API URL 请填写到 /v1，或填写完整的 API 地址".to_owned());
    }
    Ok(endpoint)
}

fn request_body(request: &VisionRequest, pdf_bytes: &[u8]) -> Value {
    let page_count = request.end_page - request.start_page + 1;
    let prompt = format!(
        "你是 PDF 目录结构识别器。附件仅包含原 PDF 第 {}–{} 页，附件第 1 页对应原 PDF 第 {} 页。\n\
逐项识别能形成有效 PDF 书签的目录内容，严格保持视觉阅读顺序和标题原文；正确处理双栏、跨行标题、点线引导符、罗马数字和中文数字。优先保留带有明确目标页码的条目。没有页码的页眉、页脚、装饰文字和孤立的“目录/Contents”标题不要返回；仅当无页码标题明确统领后续有页码条目、且缺少它会破坏目录层级时，才将它作为父级返回。\n\
printedPage 只填写目录中印刷出来的目标页码，没有则为 null；不要把附件页码或原 PDF 页码当作 printedPage。level 从 0 开始，父子层级每次最多增加 1。sourcePage 是条目出现在附件中的页码，范围 1–{}。不要计算最终 PDF 跳转页。",
        request.start_page, request.end_page, request.start_page, page_count
    );
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["items"],
        "properties": {
            "items": {
                "type": "array",
                "maxItems": MAX_ITEMS,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["title", "printedPage", "level", "sourcePage"],
                    "properties": {
                        "title": { "type": "string", "minLength": 1, "maxLength": 200 },
                        "printedPage": { "type": ["string", "null"], "maxLength": 24 },
                        "level": { "type": "integer", "minimum": 0, "maximum": 8 },
                        "sourcePage": { "type": "integer", "minimum": 1, "maximum": page_count }
                    }
                }
            }
        }
    });
    json!({
        "model": request.model.trim(),
        "store": false,
        "max_output_tokens": 12000,
        "input": [{
            "role": "user",
            "content": [
                {
                    "type": "input_file",
                    "filename": format!("toc-pages-{}-{}.pdf", request.start_page, request.end_page),
                    "file_data": format!("data:application/pdf;base64,{}", STANDARD.encode(pdf_bytes)),
                    "detail": "high"
                },
                { "type": "input_text", "text": prompt }
            ]
        }],
        "text": {
            "format": {
                "type": "json_schema",
                "name": "pdf_table_of_contents",
                "strict": true,
                "schema": schema
            }
        }
    })
}

fn image_request_body(request: &VisionImagesRequest) -> Value {
    let page_count = request.end_page - request.start_page + 1;
    let prompt = format!(
        "你是 PDF 目录结构识别器。下面按顺序提供原 PDF 第 {}–{} 页的高清截图，第 1 张图对应原 PDF 第 {} 页。\n\
逐项识别能形成有效 PDF 书签的目录内容，严格保持视觉阅读顺序和标题原文；正确处理双栏、跨行标题、点线引导符、罗马数字和中文数字。优先保留带有明确目标页码的条目。没有页码的页眉、页脚、装饰文字和孤立的“目录/Contents”标题不要返回；仅当无页码标题明确统领后续有页码条目、且缺少它会破坏目录层级时，才将它作为父级返回。\n\
只返回 JSON 对象，不要 Markdown 或解释，格式为 {{\"items\":[{{\"title\":\"完整标题\",\"printedPage\":\"12\",\"level\":0,\"sourcePage\":1}}]}}。printedPage 没有时为 null，不要把截图页码当作 printedPage。level 从 0 开始且每次最多增加 1；sourcePage 是截图序号，范围 1–{}。不要计算最终 PDF 跳转页。",
        request.start_page, request.end_page, request.start_page, page_count
    );
    let mut content = vec![json!({ "type": "text", "text": prompt })];
    content.extend(request.images.iter().map(|image| {
        json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:image/png;base64,{image}"),
                "detail": "high"
            }
        })
    }));
    json!({
        "model": request.model.trim(),
        "messages": [{ "role": "user", "content": content }]
    })
}

fn parse_response(
    response_text: &str,
    start_page: u32,
    end_page: u32,
) -> Result<(Vec<BookmarkItem>, bool), String> {
    let response: Value = serde_json::from_str(response_text)
        .map_err(|_| "多模态 API 返回的不是兼容 JSON".to_owned())?;
    let incomplete = response.get("status").and_then(Value::as_str) == Some("incomplete")
        || response
            .get("incomplete_details")
            .is_some_and(|value| !value.is_null());
    if let Some(refusal) = response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|item| {
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find(|content| content.get("type").and_then(Value::as_str) == Some("refusal"))
        .and_then(|content| content.get("refusal").and_then(Value::as_str))
    {
        return Err(format!("模型拒绝解析该 PDF：{refusal}"));
    }
    let content = response
        .get("output_text")
        .and_then(Value::as_str)
        .or_else(|| {
            response
                .get("output")?
                .as_array()?
                .iter()
                .flat_map(|item| {
                    item.get("content")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                })
                .find(|content| content.get("type").and_then(Value::as_str) == Some("output_text"))?
                .get("text")?
                .as_str()
        })
        .ok_or_else(|| "API 响应中缺少 output_text".to_owned())?;
    Ok((parse_entries(content, start_page, end_page)?, incomplete))
}

fn parse_chat_response(
    response_text: &str,
    start_page: u32,
    end_page: u32,
) -> Result<(Vec<BookmarkItem>, bool), String> {
    let response: Value = serde_json::from_str(response_text)
        .map_err(|_| "高清截图 API 返回的不是兼容 JSON".to_owned())?;
    let incomplete = response
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        == Some("length");
    let content = response
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| "高清截图 API 响应中缺少 choices[0].message.content".to_owned())?;
    Ok((parse_entries(content, start_page, end_page)?, incomplete))
}

fn parse_entries(
    content: &str,
    start_page: u32,
    end_page: u32,
) -> Result<Vec<BookmarkItem>, String> {
    let trimmed = content.trim();
    let start = trimmed
        .find(['[', '{'])
        .ok_or_else(|| "模型没有返回 JSON".to_owned())?;
    let end = trimmed
        .rfind([']', '}'])
        .ok_or_else(|| "模型返回的 JSON 不完整".to_owned())?;
    let value: Value = serde_json::from_str(&trimmed[start..=end])
        .map_err(|error| format!("无法解析模型返回的目录 JSON：{error}"))?;
    let entries = value
        .as_array()
        .or_else(|| value.get("items").and_then(Value::as_array))
        .ok_or_else(|| "模型返回值应为 {\"items\": [...]}".to_owned())?;
    let excerpt_pages = end_page - start_page + 1;
    let base_level = entries
        .iter()
        .filter_map(|entry| entry.get("level").and_then(value_u32))
        .min()
        .unwrap_or(0);
    let mut items = Vec::new();
    let mut previous_level = 0;
    for (index, entry) in entries.iter().take(MAX_ITEMS).enumerate() {
        let Some(title) = entry.get("title").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        let printed_page = entry
            .get("printedPage")
            .or_else(|| entry.get("page"))
            .and_then(value_text)
            .map(|page| page.chars().take(24).collect());
        let raw_level = entry
            .get("level")
            .and_then(value_u32)
            .unwrap_or(0)
            .min(8)
            .saturating_sub(base_level);
        let level = if items.is_empty() {
            0
        } else {
            raw_level.min(previous_level + 1)
        };
        previous_level = level;
        let source_page = entry
            .get("sourcePage")
            .and_then(value_u32)
            .filter(|page| (1..=excerpt_pages).contains(page))
            .unwrap_or(1);
        items.push(BookmarkItem {
            id: format!("api-{start_page}-{index}"),
            title: title.chars().take(200).collect(),
            level,
            confidence: if printed_page.is_some() { 0.98 } else { 0.75 },
            printed_page,
            pdf_page: None,
            source_page_index: start_page + source_page - 2,
            children: Vec::new(),
        });
    }
    if items.is_empty() {
        return Err("模型没有返回可用的目录条目".to_owned());
    }
    let numbered = items
        .iter()
        .filter(|item| item.printed_page.is_some())
        .count();
    if numbered == 0 || numbered * 2 < items.len() {
        return Err("AI 返回的有效目录页码太少，请检查所选目录页范围".to_owned());
    }
    Ok(items)
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text)
            if !text.trim().is_empty() && !text.trim().eq_ignore_ascii_case("null") =>
        {
            Some(text.trim().to_owned())
        }
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn value_u32(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .or_else(|| value.as_str()?.trim().parse().ok())
}

fn api_error_message(response_text: &str) -> String {
    serde_json::from_str::<Value>(response_text)
        .ok()
        .and_then(|value| value.pointer("/error/message")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| response_text.chars().take(300).collect())
}

fn parse_usage(response_text: &str, chat_completions: bool) -> VisionUsage {
    let Ok(response) = serde_json::from_str::<Value>(response_text) else {
        return VisionUsage::default();
    };
    let usage = response.get("usage").unwrap_or(&Value::Null);
    let input_tokens = if chat_completions {
        usage
            .get("prompt_tokens")
            .or_else(|| usage.get("input_tokens"))
    } else {
        usage.get("input_tokens")
    }
    .and_then(Value::as_u64);
    let output_tokens = if chat_completions {
        usage
            .get("completion_tokens")
            .or_else(|| usage.get("output_tokens"))
    } else {
        usage.get("output_tokens")
    }
    .and_then(Value::as_u64);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_u64)
        .or_else(|| Some(input_tokens? + output_tokens?));
    VisionUsage {
        input_tokens,
        output_tokens,
        total_tokens,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn pdf_input_unsupported(status: reqwest::StatusCode, message: &str) -> bool {
    if matches!(
        status,
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
    ) {
        return true;
    }
    if !matches!(
        status,
        reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::UNPROCESSABLE_ENTITY
    ) {
        return false;
    }
    let message = message.to_ascii_lowercase();
    [
        "input_file",
        "file_data",
        "pdf",
        "json_schema",
        "text.format",
        "unsupported",
        "unexpected item type",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

async fn retry_delay(attempt: usize) {
    if attempt + 1 < MAX_ATTEMPTS {
        tokio::time::sleep(Duration::from_millis(300 * (attempt as u64 + 1))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_responses_pdf_request() {
        let request = VisionRequest {
            endpoint: "https://example.com/v1/".to_owned(),
            api_key: "test-key".to_owned(),
            model: "vision-model".to_owned(),
            input_path: "book.pdf".to_owned(),
            start_page: 9,
            end_page: 15,
        };
        assert_eq!(
            responses_endpoint(&request.endpoint).unwrap().as_str(),
            "https://example.com/v1/responses"
        );
        let body = request_body(&request, b"%PDF-test");
        assert_eq!(
            body.pointer("/input/0/content/0/type")
                .and_then(Value::as_str),
            Some("input_file")
        );
        assert!(body
            .pointer("/input/0/content/0/file_data")
            .and_then(Value::as_str)
            .unwrap()
            .starts_with("data:application/pdf;base64,"));
        assert_eq!(
            body.pointer("/text/format/type").and_then(Value::as_str),
            Some("json_schema")
        );
    }

    #[test]
    fn parses_responses_output_and_repairs_levels() {
        let response = json!({
            "status": "completed",
            "usage": { "input_tokens": 1200, "output_tokens": 80, "total_tokens": 1280 },
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": r#"{"items":[{"title":"第一章","printedPage":"1","level":2,"sourcePage":1},{"title":"1.1 入门","printedPage":"3","level":5,"sourcePage":2}]}"#
                }]
            }]
        });
        let (items, incomplete) = parse_response(&response.to_string(), 9, 15).unwrap();
        assert!(!incomplete);
        let usage = parse_usage(&response.to_string(), false);
        assert_eq!(usage.input_tokens, Some(1200));
        assert_eq!(usage.output_tokens, Some(80));
        assert_eq!(usage.total_tokens, Some(1280));
        assert_eq!(
            items.iter().map(|item| item.level).collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(
            items
                .iter()
                .map(|item| item.source_page_index)
                .collect::<Vec<_>>(),
            [8, 9]
        );
    }

    #[test]
    fn builds_chat_completions_image_fallback() {
        let request = VisionImagesRequest {
            endpoint: "https://example.com/v1/responses".to_owned(),
            api_key: "test-key".to_owned(),
            model: "vision-model".to_owned(),
            images: vec!["cG5nMQ==".to_owned(), "cG5nMg==".to_owned()],
            start_page: 9,
            end_page: 10,
        };
        assert_eq!(
            chat_completions_endpoint(&request.endpoint)
                .unwrap()
                .as_str(),
            "https://example.com/v1/chat/completions"
        );
        let body = image_request_body(&request);
        assert_eq!(
            body.pointer("/messages/0/content/1/type")
                .and_then(Value::as_str),
            Some("image_url")
        );
        assert_eq!(
            body.pointer("/messages/0/content/2/image_url/detail")
                .and_then(Value::as_str),
            Some("high")
        );
    }

    #[test]
    fn parses_chat_image_response_and_marks_pdf_fallback_errors() {
        let response = json!({
            "usage": { "prompt_tokens": 2200, "completion_tokens": 100, "total_tokens": 2300 },
            "choices": [{
                "finish_reason": "stop",
                "message": {
                    "content": "```json\n{\"items\":[{\"title\":\"第一章\",\"printedPage\":\"1\",\"level\":0,\"sourcePage\":1}]}\n```"
                }
            }]
        });
        let (items, incomplete) = parse_chat_response(&response.to_string(), 9, 15).unwrap();
        assert!(!incomplete);
        let usage = parse_usage(&response.to_string(), true);
        assert_eq!(usage.input_tokens, Some(2200));
        assert_eq!(usage.output_tokens, Some(100));
        assert_eq!(usage.total_tokens, Some(2300));
        assert_eq!(items[0].source_page_index, 8);
        assert!(pdf_input_unsupported(
            reqwest::StatusCode::BAD_REQUEST,
            "input_file is unsupported"
        ));
        assert!(pdf_input_unsupported(
            reqwest::StatusCode::NOT_FOUND,
            "route not found"
        ));
    }
}
