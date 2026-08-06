use crate::models::BookmarkItem;

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
        let Some(printed) = item.printed_page.as_deref().and_then(parse_printed_page) else {
            // Existing PDF bookmarks have a valid target page but no printed page.
            // A manual mapping must never erase that explicit destination.
            continue;
        };
        let target = i64::from(anchor_pdf) + i64::from(printed) - i64::from(anchor);
        item.pdf_page = (1..=i64::from(page_count))
            .contains(&target)
            .then_some(target as u32);
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

    fn item(page: &str) -> BookmarkItem {
        BookmarkItem {
            id: page.to_owned(),
            title: format!("目录 {page}"),
            level: 0,
            printed_page: Some(page.to_owned()),
            pdf_page: None,
            confidence: 1.0,
            source_page_index: 0,
            children: vec![],
        }
    }

    #[test]
    fn maps_arabic_pages_from_one_anchor() {
        let mut items = [item("1"), item("3"), item("5")];
        apply_single_anchor(&mut items, "1", 13, 200).unwrap();
        assert_eq!(
            items.iter().map(|item| item.pdf_page).collect::<Vec<_>>(),
            [Some(13), Some(15), Some(17)]
        );
    }

    #[test]
    fn preserves_existing_pdf_destinations_without_printed_pages() {
        let mut items = vec![BookmarkItem {
            id: "existing".into(),
            title: "已有书签".into(),
            level: 0,
            printed_page: None,
            pdf_page: Some(27),
            confidence: 1.0,
            source_page_index: 0,
            children: vec![],
        }];

        apply_single_anchor(&mut items, "1", 9, 100).unwrap();

        assert_eq!(items[0].pdf_page, Some(27));
    }

    #[test]
    fn understands_roman_and_chinese_pages() {
        assert_eq!(parse_printed_page("xiv"), Some(14));
        assert_eq!(parse_printed_page("一百二十三"), Some(123));
        assert_eq!(parse_printed_page("二〇二"), Some(202));
    }
}
