// SPDX-License-Identifier: AGPL-3.0-only
//! 义项提取基础。契约见 docs/BACKEND_STRUCTURE.md 2.6 与 7.4。
//!
//! 只做结构化提取：解析 `Entry.definition_html`，按顶层列表项/段落拆成候选，
//! 不调用选择器与 LLM、不做网络请求；无法拆分时退化为整条 Entry 一个候选。

use scraper::{ElementRef, Html};
use serde::{Deserialize, Serialize};

use crate::core::entry::Entry;

/// 每个 Entry/查询内最多候选数。
pub const MAX_SENSES: usize = 20;

/// 常见词性缩写（小写、不含句点），用于避免过度猜测。
const POS_ABBREVIATIONS: &[&str] = &[
    "n",
    "v",
    "vt",
    "vi",
    "adj",
    "adv",
    "prep",
    "conj",
    "pron",
    "num",
    "art",
    "int",
    "aux",
    "abbr",
    "pl",
    "sing",
    "phr",
    "phrase",
    "noun",
    "verb",
    "adjective",
    "adverb",
    "preposition",
    "conjunction",
    "pronoun",
    "numeral",
    "article",
    "interjection",
    "determiner",
    "det",
    "modal",
    "participle",
    "gerund",
];

/// 从 `Entry.definition_html` 提取的候选义项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SenseCandidate {
    pub id: String,
    pub entry_index: usize,
    pub source_name: String,
    pub headword: String,
    pub part_of_speech: Option<String>,
    pub definition_text: String,
    pub definition_html: String,
}

/// 单个候选块解析出的字段。
struct SenseParts {
    definition_text: String,
    definition_html: String,
    part_of_speech: Option<String>,
}

/// 提取候选义项；无法拆分时整条 Entry 一个候选。
pub fn extract_senses(entry: &Entry, entry_index: usize, max_senses: usize) -> Vec<SenseCandidate> {
    let limit = max_senses.clamp(1, MAX_SENSES);
    let raw_html = entry.definition_html.trim();
    if raw_html.is_empty() {
        return Vec::new();
    }

    let document = Html::parse_fragment(raw_html);
    let mut candidates = Vec::new();
    let mut seen_texts: Vec<String> = Vec::new();

    for block in candidate_blocks(&document) {
        if candidates.len() >= limit {
            break;
        }
        let Some(parts) = parts_from_element(block) else {
            continue;
        };
        if seen_texts.contains(&parts.definition_text) {
            continue;
        }
        seen_texts.push(parts.definition_text.clone());
        candidates.push(SenseCandidate {
            id: format!("e{entry_index}-s{}", candidates.len() + 1),
            entry_index,
            source_name: entry.source_name.clone(),
            headword: entry.headword.clone(),
            part_of_speech: parts.part_of_speech,
            definition_text: parts.definition_text,
            definition_html: parts.definition_html,
        });
    }

    // 列表/段落都拆不出可用候选时，整条 Entry 退化成一个候选。
    if candidates.is_empty() {
        if let Some(candidate) = fallback_candidate(&document, entry, entry_index) {
            candidates.push(candidate);
        }
    }

    candidates.truncate(limit);
    candidates
}

/// 按优先级收集候选块：顶层 li 优先，其次顶层 p，最后为空（走整条退化）。
fn candidate_blocks(document: &Html) -> Vec<ElementRef<'_>> {
    let mut lists = Vec::new();
    for root_child in document.tree.root().children().filter_map(ElementRef::wrap) {
        collect_lists(root_child, &mut lists);
    }

    let mut blocks: Vec<ElementRef<'_>> = Vec::new();
    for list in lists {
        blocks.extend(
            list.child_elements()
                .filter(|item| item.value().name() == "li"),
        );
    }
    if !blocks.is_empty() {
        return blocks;
    }

    // 没有列表时收集段落；列表内部的段落不参与拆分。
    let mut paragraphs = Vec::new();
    for root_child in document.tree.root().children().filter_map(ElementRef::wrap) {
        collect_paragraphs(root_child, &mut paragraphs);
    }
    paragraphs
}

/// 收集未嵌套在列表内的 ol/ul，遇到列表即停止下探（嵌套列表内容留在父 li 里）。
fn collect_lists<'a>(element: ElementRef<'a>, lists: &mut Vec<ElementRef<'a>>) {
    for child in element.child_elements() {
        match child.value().name() {
            "ol" | "ul" => lists.push(child),
            _ => collect_lists(child, lists),
        }
    }
}

/// 收集未嵌套在列表内的 p。
fn collect_paragraphs<'a>(element: ElementRef<'a>, paragraphs: &mut Vec<ElementRef<'a>>) {
    for child in element.child_elements() {
        match child.value().name() {
            "p" => paragraphs.push(child),
            "ol" | "ul" => {}
            _ => collect_paragraphs(child, paragraphs),
        }
    }
}

/// 解析一个候选块；文本过短返回 `None`。
fn parts_from_element(element: ElementRef<'_>) -> Option<SenseParts> {
    let definition_text = collapse_whitespace(&element.text().collect::<String>());
    if definition_text.chars().count() < 2 {
        return None;
    }
    Some(SenseParts {
        part_of_speech: extract_part_of_speech(element, &definition_text),
        definition_text,
        definition_html: element.inner_html(),
    })
}

/// 整条 Entry 退化候选。
fn fallback_candidate(
    document: &Html,
    entry: &Entry,
    entry_index: usize,
) -> Option<SenseCandidate> {
    let definition_text = collapse_whitespace(&document_text(document));
    if definition_text.chars().count() < 2 {
        return None;
    }
    Some(SenseCandidate {
        id: format!("e{entry_index}-s1"),
        entry_index,
        source_name: entry.source_name.clone(),
        headword: entry.headword.clone(),
        part_of_speech: part_of_speech_from_text(&definition_text),
        definition_text,
        definition_html: entry.definition_html.clone(),
    })
}

/// 整篇文档的文本（含嵌套标签文本）。
fn document_text(document: &Html) -> String {
    document
        .tree
        .root()
        .descendants()
        .filter_map(|node| node.value().as_text())
        .map(|text| text.to_string())
        .collect::<String>()
}

/// 压缩空白：连续空白折叠为一个空格，去掉首尾空白。
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 尽力提取词性：优先 class 含 pos 的标签，其次 `<i>` 前缀，最后定义文本开头的缩写。
fn extract_part_of_speech(element: ElementRef<'_>, definition_text: &str) -> Option<String> {
    for descendant in element.descendent_elements() {
        if descendant == element {
            continue;
        }
        let Some(class) = descendant.attr("class") else {
            continue;
        };
        let has_pos_class = class
            .split_whitespace()
            .any(|token| token.to_ascii_lowercase().contains("pos"));
        if !has_pos_class {
            continue;
        }
        let value = collapse_whitespace(&descendant.text().collect::<String>());
        if !value.is_empty() {
            return Some(value);
        }
    }

    if let Some(first) = element.child_elements().next() {
        if first.value().name() == "i" {
            let value = collapse_whitespace(&first.text().collect::<String>());
            if let Some(pos) = normalize_pos_text(&value) {
                return Some(pos);
            }
        }
    }

    part_of_speech_from_text(definition_text)
}

/// 从定义文本开头提取词性缩写，如 `(n.)`、`n.`、`vt./vi.`。
fn part_of_speech_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed.strip_prefix('(') {
        if let Some(end) = rest.find(')') {
            if let Some(pos) = normalize_pos_text(&rest[..end]) {
                return Some(pos);
            }
        }
    }

    let token = trimmed.split_whitespace().next()?;
    if token.ends_with('.') || token.contains('/') {
        return normalize_pos_text(token);
    }
    None
}

/// 规范化词性文本：去掉一层括号与句点后必须命中已知缩写集合，否则不猜。
fn normalize_pos_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let inner = match trimmed.strip_prefix('(') {
        Some(rest) => match rest.strip_suffix(')') {
            Some(inner) => inner.trim(),
            None => trimmed,
        },
        None => trimmed,
    };
    let tokens: Vec<&str> = inner
        .split('/')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    if tokens.iter().all(|token| is_pos_abbreviation(token)) {
        Some(inner.to_string())
    } else {
        None
    }
}

/// 判断单个 token 是否为已知词性缩写（忽略大小写与句点）。
fn is_pos_abbreviation(token: &str) -> bool {
    let normalized = token
        .trim()
        .trim_end_matches(['.', '。', '·'])
        .trim()
        .to_ascii_lowercase();
    !normalized.is_empty() && POS_ABBREVIATIONS.contains(&normalized.as_str())
}
