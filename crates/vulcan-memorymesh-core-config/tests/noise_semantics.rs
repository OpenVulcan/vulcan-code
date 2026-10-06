//! Checks source-derived VMM Noise semantics through immutable Core snapshots.
//! 通过不可变 Core 快照检查从 VMM 源码提取的 Noise 语义。

use std::sync::Arc;
use vulcan_memorymesh_core_config::{
    ConfigBundle, ConfigRegistry, ConfigSnapshot, NoisePrototypeRow, ResourceLayer,
};
use vulcan_memorymesh_core_contracts::Role;

/// Builds an isolated rule bundle; categories are JSON resource content, not runtime data.
/// 构建隔离规则束；类别文本是 JSON 资源内容而非运行时数据。
fn snapshot(categories: &str) -> Arc<ConfigSnapshot> {
    let mut layer = ResourceLayer::default();
    for (language, entries) in [("common", categories), ("zh-CN", "[]")] {
        layer
            .insert(
                format!("noise_rules/{language}.json"),
                format!(r#"{{"language":"{language}","version":"1.0.0","categories":{entries}}}"#)
                    .into_bytes(),
            )
            .expect("valid rule resource");
    }
    ConfigRegistry::new()
        .expect("registry")
        .register(ConfigBundle {
            user_layer: layer,
            ..ConfigBundle::default()
        })
        .expect("valid source-derived rule bundle")
}

/// Checks gates/defaults are canonical snapshot facts and cannot drift during recovery.
/// 检查开关与默认值是规范快照事实，且不能在恢复期间漂移。
#[test]
fn noise_policy_is_validated_canonical_and_digest_bound() {
    use vulcan_memorymesh_core_config::{ConfigImpact, NoisePolicy};
    let mut registry = ConfigRegistry::new().expect("registry");
    let defaults = registry
        .register(ConfigBundle::default())
        .expect("defaults");
    let zero = registry
        .register(ConfigBundle {
            noise_policy: NoisePolicy {
                default_semantic_threshold: 0.0,
                ..NoisePolicy::default()
            },
            ..ConfigBundle::default()
        })
        .expect("zero default");
    assert_eq!(defaults.id, zero.id);
    for policy in [
        NoisePolicy {
            enabled: false,
            ..NoisePolicy::default()
        },
        NoisePolicy {
            semantic_enabled: false,
            ..NoisePolicy::default()
        },
        NoisePolicy {
            default_semantic_threshold: 0.73,
            ..NoisePolicy::default()
        },
    ] {
        let changed = registry
            .register(ConfigBundle {
                noise_policy: policy,
                ..ConfigBundle::default()
            })
            .expect("policy");
        assert_ne!(changed.id, defaults.id);
        assert!(
            changed
                .impacts_compared_to(&defaults)
                .contains(&ConfigImpact::DataSemantics)
        );
        if !policy.enabled {
            assert!(
                !changed
                    .noise_detector
                    .detect(Role::User, "你还记得我上次说的代号吗？")
                    .is_noise
            );
        }
        if !policy.enabled || !policy.semantic_enabled {
            assert!(
                changed
                    .noise_detector
                    .semantic_phrase_targets(Role::User)
                    .is_empty()
            );
        }
    }
    for threshold in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(
            registry
                .register(ConfigBundle {
                    noise_policy: NoisePolicy {
                        default_semantic_threshold: threshold,
                        ..NoisePolicy::default()
                    },
                    ..ConfigBundle::default()
                })
                .is_err()
        );
    }
}

/// Phrases are model prototypes and must not become substring rejection rules.
/// 短语是模型原型，不得成为包含即拒绝的规则。
#[test]
fn phrases_are_not_deterministic_noise() {
    let snapshot = snapshot(
        r#"[{"name":"meta","targets":["user"],"threshold":0.8,"patterns":[],"phrases":["Remember Me"]}]"#,
    );
    assert!(
        !snapshot
            .noise_detector
            .detect(Role::User, "Remember Me")
            .is_noise
    );
    assert!(
        !snapshot
            .noise_detector
            .detect(Role::User, "The project is named Remember Me")
            .is_noise
    );
    assert_eq!(
        snapshot.noise_detector.semantic_phrase_targets(Role::User)[0].phrase,
        "Remember Me"
    );
}

/// Common source rules use ASCII word and whitespace classes even in multilingual messages.
/// 通用来源规则即便在多语言消息中也使用 ASCII 词和空白字符类。
#[test]
fn noise_regex_uses_ascii_perl_classes() {
    let snapshot = ConfigRegistry::new()
        .expect("registry")
        .register(ConfigBundle::default())
        .expect("defaults");
    assert!(
        snapshot
            .noise_detector
            .detect(Role::User, "调试query -> none结束")
            .is_noise
    );
    assert!(
        !snapshot
            .noise_detector
            .detect(Role::User, "query\u{a0}-> none")
            .is_noise
    );
}

/// Zero means the default threshold, and cached rows cannot lower snapshot policy.
/// 零表示默认阈值，缓存行不得降低快照策略。
#[test]
fn semantic_threshold_is_owned_by_the_snapshot() {
    let snapshot = snapshot(
        r#"[{"name":"meta","targets":["user"],"threshold":0,"patterns":[],"phrases":["example"]}]"#,
    );
    let prototype = NoisePrototypeRow {
        category: "meta".to_owned(),
        phrase: "example".to_owned(),
        vector: vec![1.0, 0.0],
        threshold: 0.01,
    };
    assert_eq!(
        snapshot.noise_detector.semantic_phrase_targets(Role::User)[0]
            .threshold
            .to_bits(),
        0.88_f64.to_bits()
    );
    assert!(
        !snapshot
            .noise_detector
            .semantic_decision(Role::User, &[0.5, 0.866_025_4], &[prototype])
            .is_noise
    );
}

/// Missing optional fields follow VMM defaults; empty patterns must not match all text.
/// 缺失可选字段遵循 VMM 默认值；空模式不得匹配所有文本。
#[test]
fn optional_fields_and_phrase_whitespace_follow_vmm() {
    let snapshot = snapshot(
        r#"[{"name":"meta","patterns":["  "],"phrases":["  First\n\tSecond  ",""," ","First Second"]}]"#,
    );
    assert!(
        !snapshot
            .noise_detector
            .detect(Role::User, "ordinary message")
            .is_noise
    );
    for role in [Role::User, Role::Assistant] {
        let targets = snapshot.noise_detector.semantic_phrase_targets(role);
        assert_eq!(
            targets.len(),
            1,
            "identical prototypes have one durable cache identity"
        );
        assert_eq!(targets[0].phrase, "First Second");
        assert_eq!(targets[0].threshold.to_bits(), 0.88_f64.to_bits());
    }
}

/// Source order chooses the first matching category, not alphabetical or maximum score order.
/// 来源顺序决定首个命中类别，而非字母顺序或最大分值顺序。
#[test]
fn decisions_preserve_rule_order() {
    let snapshot = snapshot(
        r#"[{"name":"z_first","targets":["user"],"threshold":0.8,"patterns":["match"],"phrases":["first"]},{"name":"a_second","targets":["user"],"threshold":0.8,"patterns":["match"],"phrases":["second"]}]"#,
    );
    assert_eq!(
        snapshot
            .noise_detector
            .detect(Role::User, "match")
            .category
            .as_deref(),
        Some("z_first")
    );
    // Deliberately reverse storage order to prove the resource remains authoritative.
    // 故意颠倒存储顺序，以证明资源仍是权威。
    let rows = vec![
        NoisePrototypeRow {
            category: "a_second".to_owned(),
            phrase: "second".to_owned(),
            vector: vec![1.0, 0.0],
            threshold: 0.8,
        },
        NoisePrototypeRow {
            category: "z_first".to_owned(),
            phrase: "first".to_owned(),
            vector: vec![0.9, 0.435_889_9],
            threshold: 0.8,
        },
    ];
    assert_eq!(
        snapshot
            .noise_detector
            .semantic_decision(Role::User, &[1.0, 0.0], &rows)
            .category
            .as_deref(),
        Some("z_first")
    );
}

/// Detection consumes normalized observations without imposing a second cosine normalization.
/// 检测消费已经归一化的观察，不施加第二次余弦归一化。
#[test]
fn semantic_similarity_preserves_normalized_source_products() {
    let snapshot = snapshot(
        r#"[{"name":"meta","targets":["user"],"threshold":0.8,"patterns":[],"phrases":["example"]}]"#,
    );
    for magnitude in [0.0_f32, 0.5, 1.0e-30] {
        let rows = [NoisePrototypeRow {
            category: "meta".to_owned(),
            phrase: "example".to_owned(),
            vector: vec![magnitude, magnitude],
            threshold: 0.8,
        }];
        let decision =
            snapshot
                .noise_detector
                .semantic_decision(Role::User, &[magnitude, magnitude], &rows);
        assert!(!decision.is_noise, "magnitude={magnitude}");
        assert_eq!(
            decision.confidence.to_bits(),
            (2.0 * f64::from(magnitude * magnitude)).to_bits()
        );
    }
}
