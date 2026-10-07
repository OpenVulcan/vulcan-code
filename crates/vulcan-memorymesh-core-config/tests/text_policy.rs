//! Text-policy execution and immutable snapshot regression coverage.
//! 文本策略执行与不可变快照回归覆盖。

use vulcan_memorymesh_core_config::{
    ConfigBundle, ConfigRegistry, PostActionTextPolicy, estimate_memory_tokens,
    sanitize_postaction_text, sanitize_postaction_text_with_policy,
};

/// Checks that an explicit larger budget retains prose while a smaller target reduces it safely.
/// 检查显式较大预算保留自然语言，而较小目标安全缩减内容。
#[test]
fn text_budget_changes_content_without_disabling_mandatory_cleanup() {
    // The natural-language fixture is neither a machine block nor a single long machine token.
    // 自然语言夹具既非机器文本块，也非单个超长机器词元。
    let prose = "我喜欢西瓜和葡萄，希望记录我的饮食偏好。\n".repeat(200);
    let input = format!("<think>must-not-survive</think>\n{prose}");
    let large = PostActionTextPolicy {
        fold_machine_text: false,
        max_fragment_tokens: 20_000,
    };
    let small = PostActionTextPolicy {
        max_fragment_tokens: 256,
        ..large
    };
    let retained = sanitize_postaction_text_with_policy(&input, &large).expect("large budget");
    assert!(!retained.contains("must-not-survive"));
    assert_eq!(retained, prose.trim());
    let clipped = sanitize_postaction_text_with_policy(&input, &small).expect("small budget");
    assert!(clipped.contains("[truncated by token budget]"));
    assert!(estimate_memory_tokens(&clipped) < estimate_memory_tokens(&retained));
    assert!(clipped.is_char_boundary(clipped.len()));
    assert_eq!(
        sanitize_postaction_text(&input).expect("default"),
        sanitize_postaction_text_with_policy(&input, &PostActionTextPolicy::default())
            .expect("explicit default")
    );
    for invalid in [0, 255, 131_073, u32::MAX] {
        assert!(
            sanitize_postaction_text_with_policy(
                "短文本",
                &PostActionTextPolicy {
                    max_fragment_tokens: invalid,
                    ..large.clone()
                }
            )
            .is_err()
        );
    }
}

/// Checks that disabling folding preserves the exact structured body but does not preserve hidden thoughts.
/// 检查禁用折叠保留准确结构化正文，但不保留隐藏思考。
#[test]
fn folding_switch_only_controls_machine_reduction() {
    let logs = (0..20)
        .map(|index| {
            format!(
                "{{\"level\":\"info\",\"msg\":\"sync-{index}\",\"trace\":\"{}\"}}",
                "x".repeat(32)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let enabled = PostActionTextPolicy {
        fold_machine_text: true,
        max_fragment_tokens: 20_000,
    };
    let disabled = PostActionTextPolicy {
        fold_machine_text: false,
        ..enabled
    };
    let input = format!("<analysis>hidden</analysis>\n{logs}");
    assert!(
        sanitize_postaction_text_with_policy(&input, &enabled)
            .expect("folded")
            .contains("folded by policy")
    );
    assert_eq!(
        sanitize_postaction_text_with_policy(&input, &disabled).expect("retained"),
        logs
    );
}

/// Verifies both controls participate in snapshot identity and invalid settings cannot be registered.
/// 验证两个控制项都参与快照身份，且非法设置无法注册。
#[test]
fn text_controls_are_frozen_and_hashed_independently() {
    let mut registry = ConfigRegistry::new().expect("registry");
    let original = registry.register(ConfigBundle::default()).expect("default");
    for text in [
        PostActionTextPolicy {
            fold_machine_text: false,
            ..PostActionTextPolicy::default()
        },
        PostActionTextPolicy {
            max_fragment_tokens: 256,
            ..PostActionTextPolicy::default()
        },
    ] {
        let mut bundle = ConfigBundle::default();
        bundle.memory_policy.postaction_text = text.clone();
        let changed = registry.register(bundle).expect("changed");
        assert_ne!(changed.memory_policy_digest, original.memory_policy_digest);
        assert_ne!(changed.id, original.id);
        assert_eq!(changed.memory_policy.postaction_text, text);
        assert_eq!(
            original.memory_policy.postaction_text,
            PostActionTextPolicy::default()
        );
    }
    let mut invalid = ConfigBundle::default();
    invalid.memory_policy.postaction_text.max_fragment_tokens = 255;
    assert!(registry.register(invalid).is_err());
}
