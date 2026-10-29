//! T1.11 义项提取基础用例。

use lexigrab_lib::core::entry::Entry;
use lexigrab_lib::parsers::sense::{extract_senses, MAX_SENSES};

fn make_entry(html: &str) -> Entry {
    let mut entry = Entry::new("hello", html);
    entry.source_name = "Mini".to_string();
    entry
}

#[test]
fn extracts_list_items_as_candidates() {
    let entry = make_entry(
        r#"<ol><li><span class="pos">n.</span> 第一个释义</li><li>第二个释义</li></ol>"#,
    );
    let senses = extract_senses(&entry, 2, 20);

    assert_eq!(senses.len(), 2);
    assert_eq!(senses[0].id, "e2-s1");
    assert_eq!(senses[1].id, "e2-s2");
    assert_eq!(senses[0].entry_index, 2);
    assert_eq!(senses[0].source_name, "Mini");
    assert_eq!(senses[0].headword, "hello");
    assert_eq!(senses[0].part_of_speech.as_deref(), Some("n."));
    assert_eq!(senses[0].definition_text, "n. 第一个释义");
    assert!(senses[0].definition_html.contains("第一个释义"));
    assert_eq!(senses[1].part_of_speech, None);
    assert_eq!(senses[1].definition_text, "第二个释义");
}

#[test]
fn extracts_unordered_list_items() {
    let entry = make_entry("<ul><li>甲释义</li><li>乙释义</li></ul>");
    let senses = extract_senses(&entry, 0, 20);

    assert_eq!(senses.len(), 2);
    assert_eq!(senses[0].definition_text, "甲释义");
    assert_eq!(senses[1].definition_text, "乙释义");
}

#[test]
fn keeps_nested_list_inside_parent_candidate() {
    let entry = make_entry("<ol><li>父项<ul><li>子项</li></ul></li></ol>");
    let senses = extract_senses(&entry, 0, 20);

    assert_eq!(senses.len(), 1);
    assert_eq!(senses[0].definition_text, "父项子项");
    assert!(senses[0].definition_html.contains("子项"));
}

#[test]
fn extracts_paragraphs_when_no_list() {
    let entry = make_entry("<p>第一段释义</p><p>第二段释义</p>");
    let senses = extract_senses(&entry, 1, 20);

    assert_eq!(senses.len(), 2);
    assert_eq!(senses[0].id, "e1-s1");
    assert_eq!(senses[0].definition_text, "第一段释义");
    assert_eq!(senses[1].definition_text, "第二段释义");
}

#[test]
fn extracts_paragraphs_inside_wrapper() {
    let entry = make_entry("<div><p>甲段释义</p><p>乙段释义</p></div>");
    let senses = extract_senses(&entry, 0, 20);

    assert_eq!(senses.len(), 2);
    assert_eq!(senses[0].definition_text, "甲段释义");
    assert_eq!(senses[1].definition_text, "乙段释义");
}

#[test]
fn falls_back_to_single_candidate_without_structure() {
    let entry = make_entry("<div><b>整段释义</b></div>");
    let senses = extract_senses(&entry, 0, 20);

    assert_eq!(senses.len(), 1);
    assert_eq!(senses[0].id, "e0-s1");
    assert_eq!(senses[0].definition_text, "整段释义");
    assert_eq!(senses[0].definition_html, "<div><b>整段释义</b></div>");
}

#[test]
fn drops_empty_and_too_short_candidates() {
    let entry = make_entry("<ol><li></li><li>  </li><li>a</li><li>有效释义</li></ol>");
    let senses = extract_senses(&entry, 0, 20);

    assert_eq!(senses.len(), 1);
    assert_eq!(senses[0].definition_text, "有效释义");
    assert_eq!(senses[0].id, "e0-s1");
}

#[test]
fn empty_html_returns_no_candidates() {
    let entry = make_entry("   ");
    assert!(extract_senses(&entry, 0, 20).is_empty());

    let empty_tags = make_entry("<ol><li></li></ol>");
    assert!(extract_senses(&empty_tags, 0, 20).is_empty());
}

#[test]
fn long_candidate_is_preserved() {
    let long_text = "很长".repeat(5_000);
    let entry = make_entry(&format!("<ol><li>{long_text}</li></ol>"));
    let senses = extract_senses(&entry, 0, 20);

    assert_eq!(senses.len(), 1);
    assert_eq!(senses[0].definition_text.chars().count(), 10_000);
    assert!(senses[0].definition_html.contains(&long_text));
}

#[test]
fn caps_candidates_at_max_senses() {
    let items: String = (1..=25)
        .map(|index| format!("<li>释义{index}</li>"))
        .collect();
    let entry = make_entry(&format!("<ol>{items}</ol>"));

    let senses = extract_senses(&entry, 0, 100);
    assert_eq!(senses.len(), MAX_SENSES);
    assert_eq!(senses[MAX_SENSES - 1].id, format!("e0-s{MAX_SENSES}"));

    let limited = extract_senses(&entry, 0, 3);
    assert_eq!(limited.len(), 3);
}

#[test]
fn deduplicates_same_definition_text() {
    let entry = make_entry("<ol><li>同样的释义</li><li>同样的释义</li><li>不同释义</li></ol>");
    let senses = extract_senses(&entry, 0, 20);

    assert_eq!(senses.len(), 2);
    assert_eq!(senses[0].definition_text, "同样的释义");
    assert_eq!(senses[1].definition_text, "不同释义");
    assert_eq!(senses[0].id, "e0-s1");
    assert_eq!(senses[1].id, "e0-s2");
}

#[test]
fn extracts_part_of_speech_from_various_markers() {
    let entry = make_entry(
        r#"<ul>
            <li><span class="pos">n.</span> 名词释义</li>
            <li><i>v.</i> 动词释义</li>
            <li>(adj.) 形容词释义</li>
            <li><i>Hello</i> 非词性斜体</li>
        </ul>"#,
    );
    let senses = extract_senses(&entry, 0, 20);

    assert_eq!(senses.len(), 4);
    assert_eq!(senses[0].part_of_speech.as_deref(), Some("n."));
    assert_eq!(senses[1].part_of_speech.as_deref(), Some("v."));
    assert_eq!(senses[2].part_of_speech.as_deref(), Some("adj."));
    assert_eq!(senses[3].part_of_speech, None);
}
