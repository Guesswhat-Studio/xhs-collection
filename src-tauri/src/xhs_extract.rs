use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

pub(crate) fn xhs_note_id(value: &Value) -> Option<String> {
    first_json_path_str(
        value,
        &[
            &["noteId"],
            &["note_id"],
            &["id"],
            &["note", "noteId"],
            &["note", "note_id"],
            &["note", "id"],
            &["noteCard", "noteId"],
            &["noteCard", "note_id"],
            &["noteCard", "id"],
            &["note_card", "noteId"],
            &["note_card", "note_id"],
            &["note_card", "id"],
        ],
    )
}

pub(crate) fn xhs_note_url(value: &Value, source_note_id: &str) -> String {
    let base = format!("https://www.xiaohongshu.com/explore/{source_note_id}");
    if let Some(url) = first_json_path_str(
        value,
        &[
            &["sourceUrl"],
            &["source_url"],
            &["noteUrl"],
            &["note_url"],
            &["href"],
        ],
    )
    .and_then(|url| normalize_xhs_note_url(&url, source_note_id))
    {
        return url;
    }

    let Some(token) = first_json_path_str(value, &[&["xsecToken"], &["xsec_token"]]) else {
        return base;
    };
    let source = first_json_path_str(value, &[&["xsecSource"], &["xsec_source"]])
        .unwrap_or_else(|| "pc_collect".to_string());
    format!(
        "{base}?xsec_token={}&xsec_source={}",
        encode_url_query_value(&token),
        encode_url_query_value(&source)
    )
}

pub(crate) fn normalize_xhs_note_url(raw_url: &str, source_note_id: &str) -> Option<String> {
    let value = raw_url.trim();
    if value.is_empty() || !value.contains(source_note_id) {
        return None;
    }
    let absolute = if value.starts_with("https://www.xiaohongshu.com/") {
        value.to_string()
    } else if value.starts_with('/') {
        format!("https://www.xiaohongshu.com{value}")
    } else {
        return None;
    };

    let query = absolute
        .find('?')
        .map(|index| &absolute[index..])
        .unwrap_or("");
    Some(format!(
        "https://www.xiaohongshu.com/explore/{source_note_id}{query}"
    ))
}

pub(crate) fn encode_url_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub(crate) fn xhs_note_type(value: &Value) -> String {
    let raw_type = first_json_path_str(
        value,
        &[
            &["noteType"],
            &["note_type"],
            &["type"],
            &["note", "type"],
            &["note", "noteType"],
        ],
    )
    .unwrap_or_default()
    .to_lowercase();

    if raw_type.contains("video") || raw_type == "1" {
        return "video".to_string();
    }
    if raw_type.contains("image")
        || raw_type.contains("normal")
        || value.get("imageList").and_then(Value::as_array).is_some()
        || value.get("images").and_then(Value::as_array).is_some()
    {
        return "image".to_string();
    }
    "unknown".to_string()
}

pub(crate) fn xhs_published_at(value: &Value) -> Option<String> {
    xhs_time_at(
        value,
        &[
            &["publishTime"],
            &["publish_time"],
            &["publishedAt"],
            &["published_at"],
            &["createTime"],
            &["create_time"],
            &["timestamp"],
            &["time"],
            &["note", "publishTime"],
            &["note", "createTime"],
            &["noteCard", "publishTime"],
            &["noteCard", "time"],
            &["note_card", "publish_time"],
            &["note_card", "time"],
        ],
    )
}

pub(crate) fn xhs_collected_at(value: &Value) -> Option<String> {
    xhs_time_at(
        value,
        &[
            &["collectTime"],
            &["collect_time"],
            &["collectedAt"],
            &["collected_at"],
            &["favoriteTime"],
            &["favorite_time"],
            &["favTime"],
            &["fav_time"],
            &["userInteract", "collectTime"],
            &["user_interact", "collect_time"],
        ],
    )
}

pub(crate) fn xhs_time_at(value: &Value, paths: &[&[&str]]) -> Option<String> {
    first_json_path_str(value, paths).and_then(|value| normalize_xhs_time(&value))
}

fn normalize_xhs_time(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return Some(datetime.with_timezone(&Utc).to_rfc3339());
    }

    if value.chars().all(|ch| ch.is_ascii_digit()) {
        if let Ok(number) = value.parse::<i64>() {
            let millis = if number > 10_000_000_000 {
                number
            } else {
                number * 1000
            };
            if let Some(datetime) = Utc.timestamp_millis_opt(millis).single() {
                return Some(datetime.to_rfc3339());
            }
        }
    }

    Some(value.to_string())
}

pub(crate) fn xhs_cover_url(value: &Value) -> Option<String> {
    first_json_path_str(
        value,
        &[
            &["cover", "url"],
            &["cover", "urlDefault"],
            &["cover", "url_default"],
            &["cover", "infoList", "0", "url"],
            &["image", "url"],
            &["image", "urlDefault"],
            &["noteCard", "cover", "url"],
            &["noteCard", "cover", "urlDefault"],
            &["noteCard", "cover", "infoList", "0", "url"],
            &["note_card", "cover", "url"],
            &["note_card", "cover", "url_default"],
            &["note_card", "cover", "infoList", "0", "url"],
        ],
    )
    .or_else(|| first_array_image_url(value, "imageList"))
    .or_else(|| first_array_image_url(value, "images"))
}

fn first_array_image_url(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_array).and_then(|items| {
        items.iter().find_map(|item| {
            first_json_path_str(
                item,
                &[
                    &["url"],
                    &["urlDefault"],
                    &["url_default"],
                    &["infoList", "0", "url"],
                ],
            )
        })
    })
}

pub(crate) fn first_json_path_str(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| json_path_str(value, path))
}

fn json_path_str(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for segment in path {
        if let Ok(index) = segment.parse::<usize>() {
            cursor = cursor.as_array()?.get(index)?;
        } else {
            cursor = cursor.get(*segment)?;
        }
    }
    json_scalar_to_string(cursor)
}

fn json_scalar_to_string(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(text) => text.trim().to_string(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => return None,
    };
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
