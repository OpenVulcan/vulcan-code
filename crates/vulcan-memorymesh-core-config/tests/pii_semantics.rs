//! Verifies VMM rule execution semantics through immutable resource snapshots.
//! 通过不可变资源快照验证 VMM 规则执行语义。

use serde::Deserialize;
use std::sync::Arc;
use vulcan_memorymesh_core_config::{ConfigBundle, ConfigRegistry, ConfigSnapshot, ResourceLayer};
use vulcan_memorymesh_core_contracts::CoreError;

/// Selects authoritative resource paths from the checked manifest, not a hand-maintained language list.
/// 从受检清单选择权威资源路径，而非手工维护的语言列表。
#[derive(Deserialize)]
struct ManifestPaths {
    /// Contains every manifest row; non-path metadata is validated by the production registry.
    /// 包含每个清单行；非路径元数据由生产注册表验证。
    files: Vec<ResourcePath>,
}

/// Holds one manifest-owned resource path for language enumeration.
/// 保存一个清单拥有的资源路径，用于枚举语言。
#[derive(Deserialize)]
struct ResourcePath {
    /// Supplies the exact embedded relative path.
    /// 提供精确的嵌入相对路径。
    path: String,
}

/// Compiles every manifest-owned PII language against common rules through production configuration.
/// 通过生产配置将每个清单拥有的 PII 语言与通用规则一起编译。
#[test]
fn every_authoritative_pii_language_compiles() {
    let manifest: ManifestPaths = toml::from_str(include_str!(
        "../../vulcan-runtime-assets/resources/memory/resource-manifest.toml"
    ))
    .expect("manifest");
    let mut count = 0;
    let mut registry = ConfigRegistry::new().expect("registry");
    for file in manifest.files {
        let Some(language) = file
            .path
            .strip_prefix("pii_rules/")
            .and_then(|path| path.strip_suffix(".json"))
        else {
            continue;
        };
        let snapshot = registry
            .register(ConfigBundle {
                pii_language: language.to_owned(),
                ..ConfigBundle::default()
            })
            .expect(language);
        assert!(
            snapshot.pii_scrubber.rule_count() > 0,
            "language={language}"
        );
        count += 1;
    }
    assert_eq!(count, 37);
}

/// Compiles explicit common and locale rules without modifying bundled resources.
/// 编译显式通用与区域规则，不修改内置资源。
fn snapshot(common: &str, locale: &str) -> Result<Arc<ConfigSnapshot>, CoreError> {
    let mut layer = ResourceLayer::default();
    for (language, rules) in [("common", common), ("zh-CN", locale)] {
        layer.insert(
            format!("pii_rules/{language}.json"),
            format!(r#"{{"language":"{language}","version":"2.0.0","rules":{rules}}}"#)
                .into_bytes(),
        )?;
    }
    ConfigRegistry::new()?.register(ConfigBundle {
        user_layer: layer,
        ..ConfigBundle::default()
    })
}

/// VMM len consumes UTF-8 byte counts, including non-ASCII captures.
/// VMM 的 len 使用 UTF-8 字节数量，包括非 ASCII 捕获。
#[test]
fn length_conditions_use_utf8_bytes() {
    for (condition, input) in [
        ("len($0) == 18", "甲乙丙丁戊己"),
        ("len($0) > 25", "甲乙丙丁戊己庚辛壬"),
    ] {
        let rules = format!(
            r#"[{{"name":"length","pattern":".+","replacement":"[MASK]","condition":"{condition}"}}]"#
        );
        let snapshot = snapshot(&rules, "[]").expect("length rules");
        assert_eq!(
            snapshot.pii_scrubber.scrub(input).expect("scrub").text,
            "[MASK]"
        );
    }
}

/// Only numeric dollar captures expand; suffixes and all other dollar forms remain literal.
/// 仅展开数字美元捕获；后缀及所有其他美元形式保持字面值。
#[test]
fn replacement_templates_follow_vmm_numeric_grammar() {
    let snapshot = snapshot(
        r#"[{"name":"template","pattern":"(secret)","replacement":"$1_tag $$ ${1} $name $0"}]"#,
        "[]",
    )
    .expect("template");
    assert_eq!(
        snapshot.pii_scrubber.scrub("secret").expect("scrub").text,
        "secret_tag $$ ${1} $name secret"
    );
}

/// Locale replacement keeps the common rule's position and trims stable rule names.
/// 区域替换保持通用规则的位置，并去除稳定规则名称的首尾空白。
#[test]
fn locale_rules_override_common_in_place() {
    let snapshot = snapshot(
        r#"[{"name":"first","pattern":"secret","replacement":"OLD"},{"name":"second","pattern":"LOCAL","replacement":"FINAL"}]"#,
        r#"[{"name":" first ","pattern":"secret","replacement":"LOCAL"}]"#,
    ).expect("locale override");
    let result = snapshot.pii_scrubber.scrub("secret").expect("scrub");
    assert_eq!(result.text, "FINAL");
    assert_eq!(
        result
            .redactions
            .iter()
            .map(|row| row.rule_name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
}

/// Invalid capture references and ambiguous names fail compilation, not silent redaction.
/// 非法捕获引用和歧义名称在编译时失败，而非静默脱敏。
#[test]
fn invalid_rule_identity_and_replacement_are_rejected() {
    for rules in [
        r#"[{"name":"","pattern":"secret","replacement":"MASK"}]"#,
        r#"[{"name":"a","pattern":"x","replacement":"MASK"},{"name":" a ","pattern":"y","replacement":"MASK"}]"#,
        r#"[{"name":"bad-capture","pattern":"(secret)","replacement":"$2"}]"#,
        r#"[{"name":"overflow-capture","pattern":"(secret)","replacement":"$99999999999999999999999999999999999"}]"#,
    ] {
        assert!(
            snapshot(rules, "[]").is_err(),
            "invalid rule accepted: {rules}"
        );
    }
}

/// An absent, empty, or whitespace-only condition is unconditional in the source engine.
/// 来源引擎中缺失、空或仅空白的条件均为无条件执行。
#[test]
fn blank_conditions_are_unconditional() {
    for condition in ["", "   ", "  len($0) == 18  "] {
        let rules = format!(
            r#"[{{"name":"condition","pattern":".+","replacement":"MASK","condition":"{condition}"}}]"#
        );
        let snapshot = snapshot(&rules, "[]").expect("trimmed condition");
        assert_eq!(
            snapshot
                .pii_scrubber
                .scrub("甲乙丙丁戊己")
                .expect("scrub")
                .text,
            "MASK"
        );
    }
}

/// Existing Chinese phone rules must recognize ASCII word boundaries next to Han characters.
/// 现有中国手机号规则必须识别与汉字相邻的 ASCII 词边界。
#[test]
fn bundled_pii_word_boundaries_follow_go_regexp() {
    let snapshot = ConfigRegistry::new()
        .expect("registry")
        .register(ConfigBundle::default())
        .expect("defaults");
    assert_eq!(
        snapshot
            .pii_scrubber
            .scrub("电话13800138000结束")
            .expect("scrub")
            .text,
        "电话[MOBILE_MASKED]结束"
    );
}

/// ASCII Perl classes do not remove support for Han properties, escaped literals, or UTF-8 text.
/// ASCII Perl 字符类不会移除汉字属性、转义字面量或 UTF-8 文本支持。
#[test]
fn regex_classes_preserve_source_semantics() {
    for (pattern, input, expected) in [
        (r"^\d+$", "１２３", "１２３"),
        (r"^\w+$", "汉字", "汉字"),
        (r"^x\sy$", "x\u{a0}y", "x\u{a0}y"),
        (r"^\D+$", "１２３", "MASK"),
        (r"^\W+$", "汉字", "MASK"),
        (r"^\S+$", "x\u{a0}y", "MASK"),
        (r"^\p{Han}+$", "汉字", "MASK"),
        (r"^\Q\d\E$", r"\d", "MASK"),
        (r"^[\d]+$", "１２３", "１２３"),
        (r"^\\d$", r"\d", "MASK"),
    ] {
        let rules = serde_json::json!([{"name":"class", "pattern":pattern, "replacement":"MASK"}])
            .to_string();
        let snapshot = snapshot(&rules, "[]").expect("source regex");
        assert_eq!(
            snapshot.pii_scrubber.scrub(input).expect("scrub").text,
            expected,
            "pattern={pattern}"
        );
    }
}

/// Guards Go token semantics independently of the bundled default patterns.
/// 独立于内置默认模式保护 Go 标记语义。
#[test]
fn custom_regex_tokens_preserve_source_semantics() {
    for (pattern, input, expected) in [
        (r"\141", "a", "MASK"),
        (r"\007", "\u{7}", "MASK"),
        (r"[a&&b]", "a&b", "MASKMASKMASK"),
        (r"[a~~b]", "a~b", "MASKMASKMASK"),
        (r"[[a]", "[a", "MASKMASK"),
        (r"[a[b]c]", "ac]", "MASK"),
        (r"a{x}", "a{x}", "MASK"),
        (r"a{}", "a{}", "MASK"),
        (r"a{1", "a{1", "MASK"),
        (r"a{01}", "a{01}", "MASK"),
        (r"(?P<x>a)(?P<x>b)", "ab", "MASK"),
        (r"(?P<1>a)", "a", "MASK"),
        (r"(?ii)a", "A", "MASK"),
        (r"(?i-i)a", "A", "A"),
    ] {
        let rules = serde_json::json!([{"name":"source", "pattern":pattern, "replacement":"MASK"}])
            .to_string();
        assert_eq!(
            snapshot(&rules, "[]")
                .expect("source pattern")
                .pii_scrubber
                .scrub(input)
                .expect("scrub")
                .text,
            expected,
            "pattern={pattern}"
        );
    }
    let numeric_capture = snapshot(
        r#"[{"name":"numbered","pattern":"(?P<x>a)(?P<x>b)","replacement":"$2$1"}]"#,
        "[]",
    )
    .expect("numbered captures survive lowering");
    assert_eq!(
        numeric_capture
            .pii_scrubber
            .scrub("ab")
            .expect("scrub")
            .text,
        "ba"
    );
}

/// Rejects invalid source syntax rather than activating Rust-only interpretation modes.
/// 拒绝非法来源语法，而非激活 Rust 独有解释模式。
#[test]
fn source_regex_rejects_invalid_extensions() {
    for pattern in [
        r"\1",
        r"\u0061",
        r"[\b]",
        r"[\Qx\E]",
        r"(?x)a",
        r"(?-u)a",
        r"(?P<>a)",
        r"(?i-)a",
        r"a{1001}",
        r"a{2,1}",
        r"a{999999999999999999999999999999}",
    ] {
        let rules =
            serde_json::json!([{"name":"invalid", "pattern":pattern, "replacement":"MASK"}])
                .to_string();
        assert!(snapshot(&rules, "[]").is_err(), "pattern={pattern}");
    }
}

/// Exercises arbitrary supported expressions, source short-circuiting, and runtime-error masking.
/// 验证任意受支持表达式、来源短路及运行时错误脱敏。
#[test]
fn condition_language_preserves_vmm_decisions() {
    for (condition, pattern, input, masks) in [
        ("len($0) == 9", ".+", "8888-9999", true),
        ("len($0) == 9", ".+", "123", false),
        ("len($0) >= 3 && len($0) < 5", ".+", "1234", true),
        ("len($0) != 4 || len($0) <= 2", ".+", "1234", false),
        ("1 == 2 || 3 == 3 && 7 % 3 == 1", ".+", "123", true),
        ("(1 == 2 || 3 == 3) && 7 % 3 != 1", ".+", "123", false),
        ("$0 == $1", "(123)", "123", true),
        ("is_base64($0) || is_luhn($0)", ".+", "SGVsbG8=", true),
        ("is_base64($0)", ".+", "Zg==", true),
        ("is_base64($0)", ".+", "Zh==", true),
        ("is_base64($0)", ".+", "Z==g", false),
        ("is_base64($0)", ".+", "Zg=", false),
        ("is_base64($0)", ".+", "====", false),
        ("is_luhn($0)", ".+", " 4111111111111111 ", true),
        ("weight_sum($1, [1,2,3]) == 14", "(.+)", "123", true),
        ("weight_sum($1, [1,2,3,]) == 14", "(.+)", "123", true),
        ("weight_sum($1, [1,2,3]) == 6", "(.+)", "12A", true),
        ("cn_check($0) == 2", ".+", "x", true),
        ("cn_check($0) == 2", ".+", "1", false),
        ("cn_check($0) == 2", ".+", "bad", true),
        ("1 == 2 && cn_check($0) == 1", ".+", "bad", false),
        ("1 == 1 || cn_check($0) == 1", ".+", "bad", true),
        ("len($1) == 0", "^(a)?b$", "b", true),
        ("1 == 2 && len($1) == 0", "^(a)?b$", "b", false),
        ("1 % 0 == 0", ".+", "anything", true),
        ("len(1) == 0", ".+", "anything", true),
        ("[1] == [1]", ".+", "anything", true),
        ("len($0)", ".+", "anything", true),
    ] {
        let rules = serde_json::json!([{"name":"expression", "pattern":pattern, "replacement":"MASK", "condition":condition}]).to_string();
        let snapshot = snapshot(&rules, "[]").expect(condition);
        assert_eq!(
            snapshot.pii_scrubber.scrub(input).expect("scrub").text,
            if masks { "MASK" } else { input },
            "condition={condition}, input={input}"
        );
    }
}

/// Bounds the compiler before matching and validates even unreachable capture references.
/// 在匹配前约束编译器，并验证不可达捕获引用。
#[test]
fn condition_language_rejects_invalid_or_excessive_resources() {
    let mut conditions = vec![
        "unknown($0)".to_owned(),
        "len($1) == 0".to_owned(),
        "1 == 1 || len($99) == 0".to_owned(),
        "len()".to_owned(),
        "len($0,$0)".to_owned(),
        "weight_sum($0,[]) == 0".to_owned(),
        "len($0) == -1".to_owned(),
        "len($0) == 9223372036854775808".to_owned(),
        "len($0".to_owned(),
        "1 2".to_owned(),
        "len($0) &&".to_owned(),
    ];
    conditions.push(vec!["is_base64($0)"; 40].join(" || "));
    conditions.push(format!("{}len($0)==3{}", "(".repeat(65), ")".repeat(65)));
    for condition in conditions {
        let rules = serde_json::json!([{"name":"invalid", "pattern":".+", "replacement":"MASK", "condition":condition}]).to_string();
        assert!(
            snapshot(&rules, "[]").is_err(),
            "invalid condition accepted: {condition}"
        );
    }
}

/// Source OR lowering allows sixteen two-op predicates (62 steps) but rejects seventeen (66).
/// 来源或降级允许十六个双操作谓词（62 步），但拒绝十七个（66 步）。
#[test]
fn condition_operation_boundary_matches_source_lowering() {
    for count in [16, 17] {
        let condition = vec!["is_base64($0)"; count].join(" || ");
        let rules = serde_json::json!([{"name":"steps", "pattern":".+", "replacement":"MASK", "condition":condition}]).to_string();
        assert_eq!(snapshot(&rules, "[]").is_ok(), count == 16);
    }
}

/// A small legal template must not expand accepted text past the existing domain text limit.
/// 小型合法模板不得把已接受文本展开到既有领域文本上限之外。
#[test]
fn redaction_expansion_is_bounded() {
    let rules =
        serde_json::json!([{"name":"expand", "pattern":"x", "replacement":"y".repeat(4096)}])
            .to_string();
    let snapshot = snapshot(&rules, "[]").expect("expansion rule");
    let boundary = snapshot
        .pii_scrubber
        .scrub(&"x".repeat(1024))
        .expect("exact boundary");
    assert_eq!(
        boundary.text.len(),
        vulcan_memorymesh_core_contracts::MAX_DOMAIN_TEXT_BYTES
    );
    let error = snapshot
        .pii_scrubber
        .scrub(&"x".repeat(1025))
        .expect_err("expansion must fail before exceeding capacity");
    assert_eq!(
        error.code,
        vulcan_memorymesh_core_contracts::ErrorCode::InvalidConfiguration
    );
    assert!(!error.side_effect_possible);
    let input_error = snapshot
        .pii_scrubber
        .scrub(&"a".repeat(vulcan_memorymesh_core_contracts::MAX_DOMAIN_TEXT_BYTES + 1))
        .expect_err("input capacity");
    assert_eq!(
        input_error.code,
        vulcan_memorymesh_core_contracts::ErrorCode::InvalidArgument
    );
}

/// Empty input remains empty, while zero-width matches on nonempty UTF-8 retain source ordering.
/// 空输入仍为空，而非空 UTF-8 上的零宽匹配保留来源顺序。
#[test]
fn empty_and_zero_width_redaction_follow_source() {
    let snapshot = snapshot(
        r#"[{"name":"boundary","pattern":"^|$","replacement":"_"}]"#,
        "[]",
    )
    .expect("boundary rule");
    let empty = snapshot.pii_scrubber.scrub("").expect("empty");
    assert!(empty.text.is_empty() && empty.redactions.is_empty());
    assert_eq!(
        snapshot
            .pii_scrubber
            .scrub("é")
            .expect("UTF-8 boundary")
            .text,
        "_é_"
    );
}
