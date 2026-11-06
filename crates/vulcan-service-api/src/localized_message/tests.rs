use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::WebuiLocalizedMessage;

/// Constructor text that marks one catalog-backed message in Rust sources; its key must follow as a
/// string literal, so a constructor whose key the scan cannot read fails instead of passing unseen.
/// 在 Rust 源码中标记一条目录消息的构造文本；其后必须是字符串字面量键，因此扫描无法读取键的构造会失败，
/// 而不是未经检查地通过。
const CONSTRUCTOR: &str = "WebuiLocalizedMessage::new(";

/// Placeholder names declared by one catalog template.
/// 单个目录模板声明的占位符名称。
fn placeholders(template: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                names.insert(after[..close].to_string());
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    names
}

/// Collects every Rust source file below `directory`.
/// 收集 `directory` 下的全部 Rust 源文件。
fn rust_sources(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// Reads the string literal at the start of `text`; returns it and the remaining text.
/// 读取 `text` 开头的字符串字面量；返回该字面量与剩余文本。
fn leading_literal(text: &str) -> Option<(&str, &str)> {
    let body = text.strip_prefix('"')?;
    let close = body.find('"')?;
    Some((&body[..close], &body[close + 1..]))
}

/// Skips one parenthesized argument list whose opening parenthesis was already consumed.
/// 跳过一个左括号已被消费的括号参数列表。
fn after_arguments(text: &str) -> &str {
    let mut depth = 1usize;
    for (index, character) in text.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &text[index + 1..];
                }
            }
            _ => {}
        }
    }
    ""
}

/// Every catalog-backed message in the workspace names a `webui` key whose placeholders it fills exactly.
/// 工作区中的每条目录消息都引用一个 `webui` 键，并恰好填充该键的全部占位符。
#[test]
fn localized_messages_match_the_webui_catalog() {
    let catalog = vulcan_i18n::WEBUI.messages(vulcan_i18n::DEFAULT_LOCALE);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for group in ["crates", "apps"] {
        let Ok(members) = std::fs::read_dir(root.join(group)) else {
            continue;
        };
        for member in members.flatten() {
            rust_sources(&member.path().join("src"), &mut files);
        }
    }
    let this_file = Path::new(file!())
        .file_name()
        .expect("test file name")
        .to_owned();
    let mut problems = Vec::new();
    let mut found = 0usize;
    for file in files {
        if file.file_name() == Some(this_file.as_os_str())
            && file.parent().and_then(Path::file_name) == Some("localized_message".as_ref())
        {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        let mut rest = source.as_str();
        while let Some(start) = rest.find(CONSTRUCTOR) {
            let arguments = &rest[start + CONSTRUCTOR.len()..];
            let Some((key, tail)) = leading_literal(arguments.trim_start()) else {
                problems.push(format!(
                    "{}: a WebuiLocalizedMessage::new call has no literal key the catalog check can read",
                    file.display()
                ));
                rest = arguments;
                continue;
            };
            found += 1;
            let mut names = BTreeSet::new();
            let mut chain = after_arguments(tail);
            while let Some(arguments) = chain.trim_start().strip_prefix(".with(") {
                let Some((name, tail)) = leading_literal(arguments.trim_start()) else {
                    break;
                };
                names.insert(name.to_string());
                chain = after_arguments(tail);
            }
            match catalog.get(key) {
                None => problems.push(format!(
                    "{}: `{key}` is not in webui/en-US.json",
                    file.display()
                )),
                Some(template) if placeholders(template) != names => problems.push(format!(
                    "{}: `{key}` fills {names:?} but the catalog declares {:?}",
                    file.display(),
                    placeholders(template)
                )),
                Some(_) => {}
            }
            rest = chain;
        }
    }
    assert!(
        found > 0,
        "no WebuiLocalizedMessage constructors were found; the scan root may be wrong"
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

/// Default-language rendering fills placeholders and leaves unknown keys readable.
/// 默认语言渲染会填充占位符，未知键保持可读。
#[test]
fn default_text_fills_placeholders_and_keeps_unknown_keys() {
    let known = WebuiLocalizedMessage::new("serviceError.gitCommit.branchExists")
        .with("branch", "feature/x");
    assert!(known.default_text().contains("feature/x"));
    assert!(!known.default_text().contains("{branch}"));
    let unknown = WebuiLocalizedMessage::new("serviceError.missing.key");
    assert_eq!(unknown.default_text(), "serviceError.missing.key");
}

/// The wire form omits empty parameters and round-trips exactly.
/// 线协议形式省略空参数并可精确往返。
#[test]
fn wire_form_round_trips() {
    let bare = WebuiLocalizedMessage::new("serviceError.gitCommit.detachedHead");
    let json = serde_json::to_value(&bare).expect("serialize");
    assert_eq!(
        json,
        serde_json::json!({ "key": "serviceError.gitCommit.detachedHead" })
    );
    let filled = bare.with("name", "value");
    let decoded: WebuiLocalizedMessage =
        serde_json::from_value(serde_json::to_value(&filled).expect("serialize")).expect("decode");
    assert_eq!(decoded, filled);
}
