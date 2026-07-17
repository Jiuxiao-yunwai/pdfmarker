use std::time::Duration;

use serde_json::{json, Value};

use crate::models::{BookmarkItem, VisionRequest};

const MAX_IMAGE_BASE64: usize = 24 * 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

pub async fn recognize_page(request: VisionRequest) -> Result<Vec<BookmarkItem>, String> {
    if request.endpoint.len() > 2048 || request.api_key.len() > 8192 || request.model.len() > 200 {
        return Err("API 配置内容过长".to_owned());
    }
    let endpoint = reqwest::Url::parse(request.endpoint.trim())
        .map_err(|_| "API URL 无效，请填写完整的 http:// 或 https:// 地址".to_owned())?;
    if !matches!(endpoint.scheme(), "http" | "https") {
        return Err("API URL 仅支持 http:// 或 https://".to_owned());
    }
    if request.api_key.trim().is_empty() || request.model.trim().is_empty() {
        return Err("请填写 API Key 和模型名".to_owned());
    }
    if request.png_base64.len() > MAX_IMAGE_BASE64 {
        return Err("发送给多模态 API 的页面图像过大".to_owned());
    }

    let prompt = "逐项识别这张电子书目录页。保留标题原文，忽略页眉、页脚和“目录”标题。只返回 JSON 数组，每项格式为 {\"title\":\"完整标题\",\"page\":\"12\",\"level\":0}；没有印刷页码时 page 为 null，level 从 0 开始，子级递增。不要 Markdown，不要解释。";
    let body = json!({
        "model": request.model.trim(),
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": prompt },
                { "type": "image_url", "image_url": {
                    "url": format!("data:image/png;base64,{}", request.png_base64),
                    "detail": "high"
                }}
            ]
        }]
    });
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| format!("无法创建 API 客户端：{error}"))?
        .post(endpoint)
        .bearer_auth(request.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("多模态 API 请求失败：{error}"))?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES)
    {
        return Err("多模态 API 返回内容过大".to_owned());
    }
    let response_text = response
        .text()
        .await
        .map_err(|error| format!("无法读取 API 响应：{error}"))?;
    if response_text.len() > MAX_RESPONSE_BYTES as usize {
        return Err("多模态 API 返回内容过大".to_owned());
    }
    if !status.is_success() {
        let message = serde_json::from_str::<Value>(&response_text)
            .ok()
            .and_then(|value| value.pointer("/error/message")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| response_text.chars().take(300).collect());
        return Err(format!("多模态 API 返回 {status}：{message}"));
    }
    let response_json: Value = serde_json::from_str(&response_text)
        .map_err(|_| "多模态 API 返回的不是兼容 JSON".to_owned())?;
    let content = response_json
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .ok_or_else(|| "API 响应中缺少 choices[0].message.content".to_owned())?;
    parse_entries(content, request.page_index)
}

fn parse_entries(content: &str, page_index: u32) -> Result<Vec<BookmarkItem>, String> {
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
        .ok_or_else(|| "模型返回值应为目录数组或 {\"items\": [...]}".to_owned())?;
    let mut items = Vec::new();
    for (index, entry) in entries.iter().take(500).enumerate() {
        let Some(title) = entry.get("title").and_then(Value::as_str).map(str::trim) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        let printed_page = entry
            .get("page")
            .or_else(|| entry.get("printedPage"))
            .and_then(value_text);
        let level = entry.get("level").and_then(value_u32).unwrap_or(0).min(8);
        items.push(BookmarkItem {
            id: format!("api-{page_index}-{index}"),
            title: title.chars().take(200).collect(),
            level,
            confidence: if printed_page.is_some() { 0.98 } else { 0.75 },
            printed_page,
            pdf_page: None,
            source_page_index: page_index,
            children: Vec::new(),
        });
    }
    if items.is_empty() {
        return Err("模型没有返回可用的目录条目".to_owned());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn calls_user_endpoint_and_parses_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let size = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..size])
                .to_ascii_lowercase()
                .contains("authorization: bearer test-key"));
            let body = r#"{"choices":[{"message":{"content":"```json\n[{\"title\":\"第一章 入门\",\"page\":12,\"level\":1}]\n```"}}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let items = tauri::async_runtime::block_on(recognize_page(VisionRequest {
            endpoint: format!("http://{address}"),
            api_key: "test-key".to_owned(),
            model: "test-model".to_owned(),
            png_base64: "cG5n".to_owned(),
            page_index: 2,
        }))
        .unwrap();
        server.join().unwrap();
        assert_eq!(items[0].title, "第一章 入门");
        assert_eq!(items[0].printed_page.as_deref(), Some("12"));
        assert_eq!(items[0].level, 1);
    }
}
