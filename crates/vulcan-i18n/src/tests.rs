// Format and behavior checks for the shared catalogs.
// 共享目录的格式与行为检查。

use std::collections::BTreeSet;

use serde_json::Value;

use super::*;

/// Every surface declared by `locales.json`; a new surface must be listed here to be checked.
/// `locales.json` 声明的全部分面；新增分面必须登记在此才会被检查。
const SURFACES: &[&Surface] = &[&BOOT, &DESKTOP, &OAUTH, &SETUP, &TRAY, &WEBUI, &TUI];

/// Placeholder names used by a message, as a sorted set.
/// 消息使用的占位符名称，以有序集合表示。
fn placeholders(message: &str) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    let mut rest = message;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('}') {
            let name = &rest[..close];
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                names.insert(name);
            }
        }
    }
    names
}

fn shared_vectors() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/catalogs/locale-matching.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn every_declared_surface_is_checked() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/catalogs/locales.json");
    let registry: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let declared: BTreeSet<&str> = registry["surfaces"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let checked: BTreeSet<&str> = SURFACES.iter().map(|surface| surface.name()).collect();
    assert_eq!(declared, checked, "register new surfaces in SURFACES");
}

#[test]
fn translations_follow_the_default_catalog() {
    let mut problems = Vec::new();
    for surface in SURFACES {
        let default = surface.messages(DEFAULT_LOCALE);
        for code in surface.locales().filter(|code| *code != DEFAULT_LOCALE) {
            let messages = surface.messages(code);
            for (key, text) in &messages {
                match default.get(key) {
                    None => problems.push(format!(
                        "{}/{code}: `{key}` is not in {DEFAULT_LOCALE}",
                        surface.name()
                    )),
                    Some(source) => {
                        if placeholders(text) != placeholders(source) {
                            problems.push(format!(
                                "{}/{code}: `{key}` placeholders differ",
                                surface.name()
                            ));
                        }
                        let affix = key.ends_with("Prefix") || key.ends_with("Suffix");
                        if !source.trim().is_empty() && text.trim().is_empty() && !affix {
                            problems.push(format!("{}/{code}: `{key}` is empty", surface.name()));
                        }
                    }
                }
            }
            if surface.is_complete() {
                // A catalog that mostly copies English passes the key check while still showing English.
                // 大部分照抄英文的目录能通过键检查却仍显示英文。
                let copied = default
                    .iter()
                    .filter(|(key, text)| messages.get(*key) == Some(text))
                    .count();
                if copied * 10 >= default.len() {
                    problems.push(format!(
                        "{}/{code}: {copied} of {} messages are copied from {DEFAULT_LOCALE}",
                        surface.name(),
                        default.len()
                    ));
                }
                problems.extend(
                    default
                        .keys()
                        .filter(|key| !messages.contains_key(*key))
                        .map(|key| format!("{}/{code}: `{key}` is missing", surface.name())),
                );
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn locale_matching_follows_shared_vectors() {
    let vectors = shared_vectors();
    for case in vectors["tags"].as_array().unwrap() {
        let tag = case["tag"].as_str().unwrap();
        assert_eq!(match_tag(tag), case["locale"].as_str(), "tag {tag:?}");
    }
    for case in vectors["preferences"].as_array().unwrap() {
        let tags: Vec<&str> = case["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tag| tag.as_str().unwrap())
            .collect();
        assert_eq!(
            detect(&tags),
            case["locale"].as_str().unwrap(),
            "tags {tags:?}"
        );
    }
    for case in vectors["savedSelections"].as_array().unwrap() {
        let value = case["value"].as_str().unwrap();
        assert_eq!(normalize(value), case["locale"].as_str(), "value {value:?}");
    }
}

#[test]
fn translator_falls_back_per_key_and_rejects_unknown_locales() {
    let mut translator = Translator::new(&[&WEBUI, &TUI]);
    let tui_only = TUI
        .messages(DEFAULT_LOCALE)
        .into_keys()
        .find(|key| !TUI.messages("zh-TW").contains_key(key));
    assert!(translator.set_locale("zh-TW"));
    assert!(!translator.set_locale("zh"));
    assert_eq!(translator.locale(), "zh-TW");
    if let Some(key) = tui_only {
        assert_eq!(
            translator.text(&key),
            TUI.messages(DEFAULT_LOCALE).get(&key).map(String::as_str)
        );
    }
    assert_eq!(translator.text("no.such.key"), None);
}

#[test]
fn format_replaces_known_placeholders_and_keeps_the_rest() {
    assert_eq!(
        format_template(
            "{count} of {total} {missing} {} {a b}",
            &[("count", "1"), ("total", "2")]
        ),
        "1 of 2 {missing} {} {a b}"
    );
}
