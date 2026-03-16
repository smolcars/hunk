#[derive(Clone, Copy)]
enum ReviewUrlAction {
    Open,
    Copy,
}

fn with_review_title_prefill(url: String, title: &str) -> String {
    let normalized_title = normalized_review_title_subject(title);
    let Some(title) = normalized_title else {
        return url;
    };

    if url.contains("/-/merge_requests/new") {
        return append_query_param(url, "merge_request[title]", title.as_str());
    }

    if url.contains("/compare/") {
        let with_quick_pull = append_query_param(url, "quick_pull", "1");
        return append_query_param(with_quick_pull, "title", title.as_str());
    }

    url
}

fn append_query_param(url: String, key: &str, value: &str) -> String {
    let mut out = url;
    let separator = if out.contains('?') {
        if out.ends_with('?') || out.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    out.push_str(separator);
    out.push_str(percent_encode_url_component(key).as_str());
    out.push('=');
    out.push_str(percent_encode_url_component(value).as_str());
    out
}

fn normalized_review_title_subject(raw: &str) -> Option<String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return None;
    }
    if normalized.starts_with('(') && normalized.contains("no description") {
        return None;
    }
    Some(normalized.to_string())
}

fn percent_encode_url_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        let is_unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(format!("%{byte:02X}").as_str());
        }
    }
    encoded
}
