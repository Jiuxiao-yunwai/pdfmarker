use std::sync::LazyLock;

use regex::Regex;

use crate::models::{BookmarkItem, TocRawBlock};

static ENTRY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?P<title>.+?)(?:\s*[\.．…⋯‥·•_—–-]+\s*|\s{2,}|[：:]\s*)(?:第\s*)?(?:[ps]\.?\s*)?(?P<page>\d+|[ivxlcdm]+|[一二三四五六七八九十百〇零]+)\s*(?:页)?\s*$",
    )
    .expect("valid TOC entry regex")
});
static PAGE_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:第\s*)?(?:[ps]\.?\s*)?(?P<page>\d+|[ivxlcdm]+|[一二三四五六七八九十百〇零]+)\s*(?:页)?$")
        .expect("valid page-only regex")
});
static PART_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(第.+篇|part\s+[ivxlcdm\d]+)").unwrap());
static CHAPTER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(第.+章|chapter\s+\w+)").unwrap());
static SECTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(第.+节|section\s+\w+)").unwrap());
static DECIMAL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+(?:\.\d+)+)").unwrap());
static CHINESE_ORDER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[一二三四五六七八九十]+、").unwrap());

struct PendingLine {
    text: String,
    page_index: u32,
    source_index: usize,
    y: f32,
    height: f32,
}

pub fn parse_blocks(blocks: &[TocRawBlock]) -> Vec<BookmarkItem> {
    let mut items = Vec::new();
    let mut pending: Option<PendingLine> = None;
    let mut has_part = false;
    let mut has_chapter = false;
    let mut previous_level = 0;

    for (index, block) in blocks.iter().enumerate() {
        let line = normalize_line(&block.text);
        if line.is_empty() || is_toc_heading(&line) {
            continue;
        }

        if let Some(captures) = PAGE_ONLY_RE.captures(&line) {
            if pending
                .as_ref()
                .is_some_and(|previous| adjacent(previous, block))
            {
                let previous = pending.take().expect("checked pending line");
                append_item(
                    &mut items,
                    previous.text,
                    Some(captures["page"].to_owned()),
                    previous.page_index,
                    previous.source_index,
                    &mut has_part,
                    &mut has_chapter,
                    &mut previous_level,
                );
            }
            continue;
        }

        if let Some(captures) = ENTRY_RE.captures(&line) {
            let mut title = clean_title(&captures["title"]);
            if !plausible_title(&title) {
                continue;
            }
            if let Some(previous) = pending.take() {
                if adjacent(&previous, block)
                    && !is_heading_like(&previous.text)
                    && plausible_title(&format!("{} {title}", previous.text))
                {
                    title = format!("{} {title}", previous.text);
                } else if is_heading_like(&previous.text) {
                    append_item(
                        &mut items,
                        previous.text,
                        None,
                        previous.page_index,
                        previous.source_index,
                        &mut has_part,
                        &mut has_chapter,
                        &mut previous_level,
                    );
                }
            }
            append_item(
                &mut items,
                title,
                Some(captures["page"].to_owned()),
                block.page_index,
                index,
                &mut has_part,
                &mut has_chapter,
                &mut previous_level,
            );
            continue;
        }

        let current = PendingLine {
            text: line,
            page_index: block.page_index,
            source_index: index,
            y: block.y,
            height: block.height,
        };
        pending = match pending.take() {
            Some(previous)
                if adjacent(&previous, block)
                    && !is_heading_like(&current.text)
                    && plausible_title(&format!("{} {}", previous.text, current.text)) =>
            {
                Some(PendingLine {
                    text: format!("{} {}", previous.text, current.text),
                    ..previous
                })
            }
            Some(previous) => {
                if is_heading_like(&previous.text) {
                    append_item(
                        &mut items,
                        previous.text,
                        None,
                        previous.page_index,
                        previous.source_index,
                        &mut has_part,
                        &mut has_chapter,
                        &mut previous_level,
                    );
                }
                Some(current)
            }
            None => Some(current),
        };
    }
    if let Some(previous) = pending.filter(|line| is_heading_like(&line.text)) {
        append_item(
            &mut items,
            previous.text,
            None,
            previous.page_index,
            previous.source_index,
            &mut has_part,
            &mut has_chapter,
            &mut previous_level,
        );
    }
    items
}

pub fn validate_candidate(items: &[BookmarkItem]) -> Result<(), String> {
    let numbered = items
        .iter()
        .filter(|item| item.printed_page.is_some())
        .count();
    if numbered == 0 || numbered * 2 < items.len() {
        return Err("所选页面不像目录：有效页码条目太少，请重新选择目录页范围".to_owned());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_item(
    items: &mut Vec<BookmarkItem>,
    title: String,
    printed_page: Option<String>,
    page_index: u32,
    source_index: usize,
    has_part: &mut bool,
    has_chapter: &mut bool,
    previous_level: &mut u32,
) {
    let title = clean_title(&title);
    let level = infer_level(&title, *has_part, *has_chapter).min(*previous_level + 1);
    *has_part |= PART_RE.is_match(&title);
    *has_chapter |= CHAPTER_RE.is_match(&title);
    *previous_level = level;
    items.push(BookmarkItem {
        id: format!("toc-{page_index}-{source_index}"),
        title,
        level,
        confidence: if printed_page.is_some() { 0.92 } else { 0.55 },
        printed_page,
        pdf_page: None,
        source_page_index: page_index,
        children: Vec::new(),
    });
}

fn normalize_line(text: &str) -> String {
    text.trim()
        .chars()
        .map(|character| match character {
            '０'..='９' => char::from_u32(character as u32 - '０' as u32 + '0' as u32).unwrap(),
            '\u{00a0}' | '\u{3000}' => ' ',
            other => other,
        })
        .collect()
}

fn clean_title(title: &str) -> String {
    title
        .trim()
        .trim_end_matches(['.', '．', '…', '·', '•', '_', '—', '–', '-', ':', '：'])
        .trim()
        .to_owned()
}

fn adjacent(previous: &PendingLine, current: &TocRawBlock) -> bool {
    previous.page_index == current.page_index
        && (previous.y - current.y).abs() <= previous.height.max(current.height) * 2.2 + 4.0
}

fn plausible_title(title: &str) -> bool {
    let length = title.chars().count();
    (1..=200).contains(&length) && title.matches('=').count() <= 1
}

fn is_heading_like(text: &str) -> bool {
    PART_RE.is_match(text)
        || CHAPTER_RE.is_match(text)
        || SECTION_RE.is_match(text)
        || DECIMAL_RE.is_match(text)
        || CHINESE_ORDER_RE.is_match(text)
        || text.starts_with('（')
        || text.starts_with('(')
        || text.starts_with("附录")
        || text.to_ascii_lowercase().starts_with("appendix")
}

fn is_toc_heading(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "目录" | "目次" | "contents" | "table of contents"
    )
}

fn infer_level(title: &str, has_part: bool, has_chapter: bool) -> u32 {
    if PART_RE.is_match(title) {
        return 0;
    }
    if CHAPTER_RE.is_match(title) {
        return u32::from(has_part);
    }
    if SECTION_RE.is_match(title) {
        return if has_chapter {
            u32::from(has_part) + 1
        } else {
            1
        };
    }
    if title.starts_with('（') || title.starts_with('(') {
        return 2;
    }
    if let Some(numbering) = DECIMAL_RE.captures(title).and_then(|c| c.get(1)) {
        return numbering.as_str().matches('.').count() as u32 + u32::from(has_part);
    }
    if CHINESE_ORDER_RE.is_match(title) {
        return 1;
    }
    0
}

pub fn apply_single_anchor(
    items: &mut [BookmarkItem],
    anchor_printed: &str,
    anchor_pdf: u32,
    page_count: u32,
) -> Result<(), String> {
    let anchor = parse_printed_page(anchor_printed)
        .ok_or_else(|| "锚点印刷页码无法识别，请输入阿拉伯数字、罗马数字或中文数字".to_owned())?;
    if anchor_pdf == 0 || anchor_pdf > page_count {
        return Err(format!("锚点 PDF 页必须在 1 到 {page_count} 之间"));
    }

    for item in items {
        item.pdf_page = item
            .printed_page
            .as_deref()
            .and_then(parse_printed_page)
            .and_then(|printed| {
                let target = i64::from(anchor_pdf) + i64::from(printed) - i64::from(anchor);
                (1..=i64::from(page_count))
                    .contains(&target)
                    .then_some(target as u32)
            });
    }
    Ok(())
}

fn parse_printed_page(value: &str) -> Option<u32> {
    let value = value.trim();
    if let Ok(page) = value.parse::<u32>() {
        return Some(page);
    }
    if value.chars().all(|c| "ivxlcdmIVXLCDM".contains(c)) {
        return parse_roman(value);
    }
    parse_chinese(value)
}

fn parse_roman(value: &str) -> Option<u32> {
    let mut total = 0;
    let mut previous = 0;
    for c in value.chars().rev() {
        let current = match c.to_ascii_uppercase() {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            'D' => 500,
            'M' => 1000,
            _ => return None,
        };
        if current < previous {
            total -= current;
        } else {
            total += current;
            previous = current;
        }
    }
    (total > 0).then_some(total)
}

fn parse_chinese(value: &str) -> Option<u32> {
    let digit = |c| match c {
        '〇' | '零' => Some(0),
        '一' => Some(1),
        '二' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        _ => None,
    };
    if value.contains('百') || value.contains('十') {
        let mut total = 0;
        let mut current = 0;
        for c in value.chars() {
            match c {
                '百' => {
                    total += current.max(1) * 100;
                    current = 0;
                }
                '十' => {
                    total += current.max(1) * 10;
                    current = 0;
                }
                _ => current = digit(c)?,
            }
        }
        Some(total + current)
    } else {
        value
            .chars()
            .try_fold(0_u32, |number, c| Some(number * 10 + digit(c)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(text: &str, index: u32) -> TocRawBlock {
        TocRawBlock {
            text: text.to_owned(),
            page_index: 2,
            x: 0.0,
            y: index as f32,
            width: text.len() as f32,
            height: 1.0,
            font_size: None,
            confidence: Some(1.0),
        }
    }

    #[test]
    fn parses_common_toc_and_maps_pages() {
        let blocks = [
            block("目录", 0),
            block("第一篇 基础 ........ 1", 1),
            block("第一章 入门 ........ 3", 2),
            block("1.1 安装 ........ 5", 3),
        ];
        let mut items = parse_blocks(&blocks);
        assert_eq!(items.iter().map(|i| i.level).collect::<Vec<_>>(), [0, 1, 2]);
        apply_single_anchor(&mut items, "1", 13, 200).unwrap();
        assert_eq!(
            items.iter().map(|i| i.pdf_page).collect::<Vec<_>>(),
            [Some(13), Some(15), Some(17)]
        );
    }

    #[test]
    fn understands_roman_and_chinese_pages() {
        assert_eq!(parse_printed_page("xiv"), Some(14));
        assert_eq!(parse_printed_page("一百二十三"), Some(123));
        assert_eq!(parse_printed_page("二〇二"), Some(202));
    }

    #[test]
    fn accepts_split_pages_fullwidth_digits_and_page_suffixes() {
        let blocks = [
            block("第一章 分行页码", 0),
            block("１２", 1),
            block("1.1 单个引导点．１３页", 2),
            block("1.2 冒号分隔： 14", 3),
            block("第二章 暂无页码", 4),
        ];
        let items = parse_blocks(&blocks);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].printed_page.as_deref(), Some("12"));
        assert_eq!(items[1].printed_page.as_deref(), Some("13"));
        assert_eq!(items[2].printed_page.as_deref(), Some("14"));
        assert_eq!(items[3].printed_page, None);
        assert!(items[3].confidence < 0.75);
    }

    #[test]
    fn accepts_more_leaders_and_prefixed_pages() {
        let blocks = [
            block("第一章 版式提取⋯⋯P. 12", 0),
            block("1.1 跨栏目录‥‥13", 1),
            block("1.2 空格页码    14页", 2),
        ];
        let items = parse_blocks(&blocks);
        assert_eq!(items.len(), 3);
        assert_eq!(
            items
                .iter()
                .map(|item| item.printed_page.as_deref())
                .collect::<Vec<_>>(),
            [Some("12"), Some("13"), Some("14")]
        );
    }

    #[test]
    fn rejects_body_text_mistaken_for_a_toc() {
        let blocks = [
            block("Chapter 2 Convex sets", 0),
            block(
                "2.1 Let C be a convex set and prove the following result",
                1,
            ),
            block(
                "Solution. This is body text rather than a contents entry.",
                2,
            ),
        ];
        let items = parse_blocks(&blocks);
        assert!(validate_candidate(&items).is_err());
    }
}
