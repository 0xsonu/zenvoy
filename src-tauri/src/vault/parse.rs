use once_cell::sync::Lazy;
use regex::Regex;

static FENCED_BLOCK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)(^|\n)```[^\n]*\n[\s\S]*?\n```[ \t]*(?:\n|$)").unwrap());
static INLINE_CODE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`[^`\n]*`").unwrap());
static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\s)#([A-Za-z][\w\-/]*)").unwrap());
static WIKILINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(!?)\[\[([^\]|]+?)(?:\|[^\]]+)?\]\]").unwrap());
static LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(!?)\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)"#).unwrap());
static EMBED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"!\[\[([^\]|]+?)(?:\|[^\]]+)?\]\]").unwrap());
static FRONTMATTER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)\A---\n(.*?)\n---\n?").unwrap());
static HEADING_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^#{1,6}\s+").unwrap());
static IMAGE_MD_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap());
static MD_LINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap());
static MD_EMBED_ALT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"!\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").unwrap());
static MD_WIKI_ALT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").unwrap());
static MARKUP_TRIM_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[*_~>]+").unwrap());
static WS_COLLAPSE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
static SCHEME_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z\d+.\-]*:").unwrap());

static TASK_LINE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(\s*(?:[-*+]|\d+\.)\s+)\[( |x|X)\](.*)$").unwrap());
static INLINE_DUE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)(?:^|\s)due:(\S+)").unwrap());
static INLINE_PRIORITY_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:^|\s)!(high|med|medium|low|h|m|l)\b").unwrap());
static INLINE_WAITING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:^|\s)@waiting\b").unwrap());
static INLINE_TAG_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(?:^|\s)#([a-z0-9][a-z0-9/_\-]*)").unwrap());
static ISO_DATE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());

const ATTACHMENT_EXTS: &[&str] = &[
    ".apng", ".avif", ".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp", ".pdf", ".aac", ".flac",
    ".m4a", ".mp3", ".ogg", ".wav", ".m4v", ".mov", ".mp4", ".ogv", ".webm",
];

fn strip_code_content(body: &str) -> String {
    if !body.contains('`') {
        return body.to_string();
    }
    let out = FENCED_BLOCK_RE.replace_all(body, "$1 ");
    INLINE_CODE_RE.replace_all(&out, " ").to_string()
}

pub fn extract_tags(body: &str) -> Vec<String> {
    if !body.contains('#') {
        return vec![];
    }
    let stripped = strip_code_content(body);
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for cap in TAG_RE.captures_iter(&stripped) {
        let tag = cap[1].to_string();
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
    }
    out
}

pub fn extract_wikilinks(body: &str) -> Vec<String> {
    if !body.contains("[[") {
        return vec![];
    }
    let stripped = strip_code_content(body);
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for cap in WIKILINK_RE.captures_iter(&stripped) {
        let bang = &cap[1];
        let target = cap[2].trim().to_string();
        if target.is_empty() {
            continue;
        }
        if bang == "!" && is_attachment_ext(&target) {
            continue;
        }
        if seen.insert(target.clone()) {
            out.push(target);
        }
    }
    out
}

pub fn body_has_local_asset(body: &str) -> bool {
    if !body.contains("](") && !body.contains("![[") {
        return false;
    }
    let stripped = strip_code_content(body);
    for cap in LINK_RE.captures_iter(&stripped) {
        let href = cap[2].trim();
        if href.is_empty() || href.starts_with('#') || href.starts_with("//") {
            continue;
        }
        if SCHEME_RE.is_match(href) {
            continue;
        }
        if is_attachment_ext(href) {
            return true;
        }
    }
    for cap in EMBED_RE.captures_iter(&stripped) {
        if is_attachment_ext(cap[1].trim()) {
            return true;
        }
    }
    false
}

/// Extract all asset embed references (![[...]] and ![...](...)  local paths).
pub fn extract_asset_embeds(body: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let stripped = strip_code_content(body);
    for cap in EMBED_RE.captures_iter(&stripped) {
        let t = cap[1].trim();
        if !t.is_empty() {
            seen.insert(t.to_string());
        }
    }
    for cap in LINK_RE.captures_iter(&stripped) {
        if &cap[0][..1] != "!" {
            continue;
        }
        let href = cap[2].trim();
        if href.is_empty() || href.starts_with('#') || href.starts_with("//") {
            continue;
        }
        if SCHEME_RE.is_match(href) {
            continue;
        }
        seen.insert(href.to_string());
    }
    seen.into_iter().collect()
}

pub fn build_excerpt(body: &str) -> String {
    let without_front = if body.starts_with("---\n") {
        FRONTMATTER_RE.replace(body, "").to_string()
    } else {
        body.to_string()
    };
    let mut text = strip_code_content(&without_front);
    if text.contains("](") {
        text = IMAGE_MD_RE.replace_all(&text, " ").to_string();
        text = MD_LINK_RE.replace_all(&text, "$1").to_string();
    }
    if text.contains("![[") {
        text = MD_EMBED_ALT_RE
            .replace_all(&text, |caps: &regex::Captures| {
                caps.get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| {
                        caps.get(1)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default()
                    })
            })
            .to_string();
    }
    if text.contains("[[") {
        text = MD_WIKI_ALT_RE
            .replace_all(&text, |caps: &regex::Captures| {
                caps.get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| {
                        caps.get(1)
                            .map(|m| m.as_str().to_string())
                            .unwrap_or_default()
                    })
            })
            .to_string();
    }
    if text.contains('#') {
        text = HEADING_RE.replace_all(&text, "").to_string();
    }
    if text.contains('*') || text.contains('_') || text.contains('~') || text.contains('>') {
        text = MARKUP_TRIM_RE.replace_all(&text, "").to_string();
    }
    text = WS_COLLAPSE_RE.replace_all(&text, " ").trim().to_string();
    if text.len() > 220 {
        text.truncate(220);
    }
    text
}

fn is_attachment_ext(target: &str) -> bool {
    let clean = target.split(&['#', '?'][..]).next().unwrap_or(target);
    if let Some(dot_pos) = clean.rfind('.') {
        let ext = clean[dot_pos..].to_lowercase();
        ATTACHMENT_EXTS.contains(&ext.as_str())
    } else {
        false
    }
}

use super::types::{NoteFolder, VaultTask};

pub fn parse_tasks(path: &str, title: &str, folder: &NoteFolder, body: &str) -> Vec<VaultTask> {
    let normalized = body.replace("\r\n", "\n");
    let defaults = parse_note_defaults(&normalized);
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut out = vec![];
    let mut task_index = 0;
    let mut in_fence = false;
    let fence_start_re = Regex::new(r"^[ \t]*(`{3,}|~{3,})").unwrap();

    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = fence_start_re.captures(line) {
            let marker = caps[1].to_string();
            if !in_fence {
                in_fence = true;
            } else if line.contains(&marker) {
                in_fence = false;
            }
            continue;
        }
        if in_fence {
            continue;
        }
        let Some(caps) = TASK_LINE_RE.captures(line) else {
            continue;
        };
        let checked_char = &caps[2];
        let tail = caps[3].to_string();
        let checked = checked_char == "x" || checked_char == "X";

        let mut due = String::new();
        let mut priority = String::new();
        let mut waiting = false;
        let mut tags = vec![];
        let mut stripped = tail.clone();

        if let Some(dm) = INLINE_DUE_RE.captures(&stripped) {
            let d = dm[1].to_string();
            if ISO_DATE_RE.is_match(&d) {
                due = d;
            }
            stripped = INLINE_DUE_RE.replace(&stripped, " ").to_string();
        }
        if let Some(pm) = INLINE_PRIORITY_RE.captures(&stripped) {
            priority = normalize_priority(&pm[1]);
            stripped = INLINE_PRIORITY_RE.replace(&stripped, " ").to_string();
        }
        if INLINE_WAITING_RE.is_match(&stripped) {
            waiting = true;
            stripped = INLINE_WAITING_RE.replace(&stripped, " ").to_string();
        }
        for tm in INLINE_TAG_RE.captures_iter(&tail) {
            let tag = tm[1].to_lowercase();
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        stripped = WS_COLLAPSE_RE.replace_all(stripped.trim(), " ").to_string();
        let content = if stripped.is_empty() {
            tail.trim().to_string()
        } else {
            stripped
        };

        let final_due = if due.is_empty() {
            defaults.due.clone()
        } else {
            due
        };
        let final_priority = if priority.is_empty() {
            defaults.priority.clone()
        } else {
            priority
        };

        out.push(VaultTask {
            id: format!("{}#{}", path, task_index),
            source_path: path.to_string(),
            note_title: title.to_string(),
            note_folder: folder.clone(),
            line_number: i as i32,
            task_index,
            raw_text: line.to_string(),
            content,
            checked,
            due: final_due,
            priority: final_priority,
            waiting,
            tags,
        });
        task_index += 1;
    }
    out
}

struct NoteDefaults {
    due: String,
    priority: String,
}

fn parse_note_defaults(body: &str) -> NoteDefaults {
    let mut defaults = NoteDefaults {
        due: String::new(),
        priority: String::new(),
    };
    if let Some(caps) = FRONTMATTER_RE.captures(body) {
        for line in caps[1].lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(colon) = trimmed.find(':') {
                let key = trimmed[..colon].trim().to_lowercase();
                let val = unquote(trimmed[colon + 1..].trim());
                match key.as_str() {
                    "due" if ISO_DATE_RE.is_match(&val) => {
                        defaults.due = val;
                    }
                    "priority" => {
                        let p = normalize_priority(&val);
                        if !p.is_empty() {
                            defaults.priority = p;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    defaults
}

fn normalize_priority(raw: &str) -> String {
    match raw.to_lowercase().trim() {
        "high" | "h" => "high".to_string(),
        "med" | "medium" | "m" => "med".to_string(),
        "low" | "l" => "low".to_string(),
        _ => String::new(),
    }
}

fn unquote(v: &str) -> String {
    let t = v.trim();
    if t.len() >= 2 {
        let first = t.as_bytes()[0];
        let last = t.as_bytes()[t.len() - 1];
        if (first == b'"' || first == b'\'') && first == last {
            return t[1..t.len() - 1].to_string();
        }
    }
    t.to_string()
}

/// Extract the title from a note body — first H1/H2 heading, or filename stem.
pub fn extract_title(body: &str, filename_stem: &str) -> String {
    for line in body.lines().take(20) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let title = rest.trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    filename_stem.to_string()
}
