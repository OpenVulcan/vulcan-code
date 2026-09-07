//! Resource and immutable configuration conformance tests.
//! 资源与不可变配置一致性测试。

#![allow(clippy::expect_used)]

use vulcan_memorymesh_core_config::{
    ConfigBundle, ConfigImpact, ConfigRegistry, MAX_CONFIG_IDENTIFIER_BYTES,
    MAX_RESOURCE_FILE_BYTES, MAX_RESOURCE_LAYER_BYTES, MAX_RESOURCE_LAYER_FILES, MemoryPolicy,
    REQUIRED_PROMPT_SCENES, ResourceLayer, embedded_resource_identity,
};
use vulcan_memorymesh_core_contracts::{
    ErrorCode, RawScoreKind, Role, ScoreContractDescriptor, ScoreContractRef, ScoreDirection,
};

/// Binds physical consistency to configuration identity without coupling it to either scoring mode.
/// 将物理一致性绑定到配置身份，且不耦合任一评分方式；验证默认值及四种合法组合。
#[test]
fn observation_mode_changes_snapshot_identity() {
    use vulcan_memorymesh_core_contracts::{CandidateReadMode, RrfScoreMode};
    let mut registry = ConfigRegistry::new().expect("registry");
    let mut policy = MemoryPolicy::default();
    assert_eq!(
        policy.retrieval_policy.candidate_read_mode,
        CandidateReadMode::SeparateChannels
    );
    let mut identities = std::collections::BTreeSet::new();
    for score in [RrfScoreMode::RankNormalized, RrfScoreMode::FusedRankOnly] {
        for read in [
            CandidateReadMode::SeparateChannels,
            CandidateReadMode::SingleObservation,
        ] {
            policy.retrieval_policy.rrf_score_mode = score;
            policy.retrieval_policy.candidate_read_mode = read;
            let snapshot = registry
                .register(ConfigBundle {
                    memory_policy: policy.clone(),
                    ..ConfigBundle::default()
                })
                .expect("independent modes");
            assert!(
                identities.insert(snapshot.id.clone()),
                "read mode omitted from digest"
            );
        }
    }
}

/// Verifies the exact frozen authoritative resource inventory.
/// 验证精确冻结的权威资源清单。
#[test]
fn authoritative_resource_manifest_is_exact() {
    // Load and validate every embedded file against the manifest.
    // 加载并针对清单校验每个嵌入文件。
    let identity = embedded_resource_identity().expect("resource identity must validate");
    assert_eq!(identity.file_count, 49);
    assert_eq!(identity.total_bytes, 89_818);
    assert_eq!(identity.manifest_version, 1);
}

/// Verifies default prompt, PII, and Noise behavior in one immutable snapshot.
/// 验证一个不可变快照中的默认 Prompt、PII 与 Noise 行为。
#[test]
fn default_snapshot_compiles_all_resource_types() {
    // Create a validated empty registry.
    // 创建经过校验的空注册表。
    let mut registry = ConfigRegistry::new().expect("registry must validate");
    // Register the documented defaults.
    // 注册文档规定的默认值。
    let snapshot = registry
        .register(ConfigBundle::default())
        .expect("default registration must compile");
    assert_eq!(snapshot.prompts.len(), 5);
    assert!(snapshot.prompt("precheck_l1_main.md").is_ok());
    // Scrub a Chinese mobile number using common plus zh-CN rules.
    // 使用 common 与 zh-CN 规则清洗中国手机号码。
    let scrubbed = snapshot
        .pii_scrubber
        .scrub("联系电话 13800138000")
        .expect("bounded scrub");
    assert_eq!(scrubbed.text, "联系电话 [MOBILE_MASKED]");
    assert!(snapshot.pii_scrubber.is_excluded_field("session_id"));
    // Detect a locale-overridden Chinese meta question.
    // 检测区域覆盖后的中文元问题。
    let noise = snapshot
        .noise_detector
        .detect(Role::User, "你还记得我上次说的代号吗？");
    assert!(noise.is_noise);
    assert_eq!(noise.category.as_deref(), Some("meta_question"));
}

/// Verifies documented aliases and rejects incomplete whole-bundle overrides.
/// 验证已记录别名并拒绝不完整的整体 Bundle 覆盖。
#[test]
fn prompt_alias_and_whole_bundle_override_are_enforced() {
    // Create a validated registry for alias compilation.
    // 创建用于别名编译的已校验注册表。
    let mut registry = ConfigRegistry::new().expect("registry must validate");
    let defaults = registry
        .register(ConfigBundle::default())
        .expect("default bundle");
    let empty_alias = registry
        .register(ConfigBundle {
            prompt_bundle: String::new(),
            ..ConfigBundle::default()
        })
        .expect("manifest-declared empty alias");
    assert_eq!(empty_alias.prompt_bundle, "default_en");
    assert_eq!(empty_alias.id, defaults.id);
    // Select the Chinese default through a documented alias.
    // 通过已记录别名选择中文默认 Bundle。
    let chinese = ConfigBundle {
        name: "chinese".to_owned(),
        prompt_bundle: "zh-cn".to_owned(),
        ..ConfigBundle::default()
    };
    // Compile the alias into the exact canonical bundle name.
    // 将别名编译为精确规范 Bundle 名称。
    let snapshot = registry
        .register(chinese)
        .expect("alias registration must compile");
    assert_eq!(snapshot.prompt_bundle, "default_cn");
    // Create a partial high-priority bundle that must not silently fall back per scene.
    // 创建一个不允许按场景静默兜底的部分高优先级 Bundle。
    let mut partial_layer = ResourceLayer::default();
    partial_layer
        .insert(
            "prompts/default_en/precheck_l1_main.md",
            b"override".to_vec(),
        )
        .expect("valid override path must insert");
    // Register the partial bundle and capture the explicit failure.
    // 注册部分 Bundle 并捕获显式失败。
    let error = registry
        .register(ConfigBundle {
            name: "partial".to_owned(),
            user_layer: partial_layer,
            ..ConfigBundle::default()
        })
        .expect_err("partial prompt bundle must fail");
    assert_eq!(error.code, ErrorCode::MissingResource);
}

/// Verifies complete user prompt replacement and snapshot immutability.
/// 验证完整用户 Prompt 替换与快照不可变性。
#[test]
fn complete_prompt_override_produces_a_new_retained_snapshot() {
    // Create a validated registry.
    // 创建经过校验的注册表。
    let mut registry = ConfigRegistry::new().expect("registry must validate");
    // Register and activate the authoritative default snapshot.
    // 注册并激活权威默认快照。
    let original = registry
        .register(ConfigBundle::default())
        .expect("default registration must compile");
    registry
        .activate("default")
        .expect("default registration must activate");
    // Build one complete higher-priority bundle.
    // 构建一个完整的更高优先级 Bundle。
    let mut user_layer = ResourceLayer::default();
    for scene in REQUIRED_PROMPT_SCENES {
        user_layer
            .insert(
                format!("prompts/default_en/{scene}"),
                format!("override:{scene}").into_bytes(),
            )
            .expect("complete override scene must insert");
    }
    // Re-register under the same name without implicitly changing activation.
    // 使用相同名称重新注册，但不隐式改变激活状态。
    let replacement = registry
        .register(ConfigBundle {
            user_layer,
            ..ConfigBundle::default()
        })
        .expect("complete override must compile");
    assert_ne!(original.id, replacement.id);
    assert_eq!(
        registry
            .active_snapshot()
            .expect("active snapshot must remain available")
            .id,
        original.id
    );
    assert!(registry.snapshot(&original.id).is_some());
    assert!(registry.snapshot(&replacement.id).is_some());
}

/// Verifies that invalid resource paths never enter a host layer.
/// 验证无效资源路径绝不进入宿主层。
#[test]
fn resource_layer_rejects_path_traversal() {
    // Create an empty host resource layer.
    // 创建空宿主资源层。
    let mut layer = ResourceLayer::default();
    // Attempt to insert a path that escapes the closed resource namespace.
    // 尝试插入一条逃逸封闭资源命名空间的路径。
    let error = layer
        .insert("../secret", b"secret".to_vec())
        .expect_err("path traversal must fail");
    assert_eq!(error.code, ErrorCode::InvalidConfiguration);
}

/// Verifies direct public-map construction cannot bypass authoritative compile-time validation.
/// 验证直接构造公开映射无法绕过编译时权威校验。
#[test]
fn snapshot_compilation_revalidates_public_resource_maps() {
    for (name, path, bytes) in [
        ("bypassed-path", "../secret", b"secret".to_vec()),
        ("bypassed-empty", "pii_rules/empty.json", Vec::new()),
    ] {
        // Construct the public map directly to model a Rust caller bypassing `insert`.
        // 直接构造公开映射，模拟 Rust 调用方绕过 `insert` 的情况。
        let mut layer = ResourceLayer::default();
        layer.files.insert(path.to_owned(), bytes);
        let mut registry = ConfigRegistry::new().expect("registry must validate");
        let error = registry
            .register(ConfigBundle {
                name: name.to_owned(),
                user_layer: layer,
                ..ConfigBundle::default()
            })
            .expect_err("bypassed invalid resource entry must fail compilation");
        assert_eq!(error.code, ErrorCode::InvalidConfiguration, "{name}");
    }
}

/// Verifies one override layer has deterministic duplicate, item, file, and aggregate bounds.
/// 验证单个覆盖层具有确定性的重复、数量、单文件和聚合边界。
#[test]
fn resource_layer_enforces_all_capacity_bounds() {
    let mut duplicate = ResourceLayer::default();
    duplicate
        .insert("pii_rules/duplicate.json", b"{}".to_vec())
        .expect("first unique resource must insert");
    let duplicate_error = duplicate
        .insert("pii_rules/duplicate.json", b"[]".to_vec())
        .expect_err("duplicate resource path must fail");
    assert_eq!(duplicate_error.code, ErrorCode::InvalidConfiguration);

    let oversized_error = ResourceLayer::default()
        .insert(
            "noise_rules/oversized.json",
            vec![b'x'; MAX_RESOURCE_FILE_BYTES + 1],
        )
        .expect_err("oversized resource file must fail");
    assert_eq!(oversized_error.code, ErrorCode::InvalidConfiguration);

    let mut item_bounded = ResourceLayer::default();
    for index in 0..MAX_RESOURCE_LAYER_FILES {
        item_bounded
            .insert(format!("pii_rules/item-{index}.json"), b"{}".to_vec())
            .expect("resource within item bound must insert");
    }
    let item_error = item_bounded
        .insert("pii_rules/item-overflow.json", b"{}".to_vec())
        .expect_err("resource item overflow must fail");
    assert_eq!(item_error.code, ErrorCode::InvalidConfiguration);

    let mut byte_bounded = ResourceLayer::default();
    for index in 0..(MAX_RESOURCE_LAYER_BYTES / MAX_RESOURCE_FILE_BYTES) {
        byte_bounded
            .insert(
                format!("noise_rules/chunk-{index}.json"),
                vec![b'x'; MAX_RESOURCE_FILE_BYTES],
            )
            .expect("resource within aggregate byte bound must insert");
    }
    let aggregate_error = byte_bounded
        .insert("noise_rules/aggregate-overflow.json", vec![b'x'])
        .expect_err("resource aggregate overflow must fail");
    assert_eq!(aggregate_error.code, ErrorCode::InvalidConfiguration);
}

/// Verifies configuration names and selectors are bounded before normalization or hashing.
/// 验证配置名称与选择器在规范化或哈希前受到边界约束。
#[test]
fn configuration_identifiers_are_closed_and_bounded() {
    let invalid_bundles = [
        ConfigBundle {
            name: " registration-with-padding ".to_owned(),
            ..ConfigBundle::default()
        },
        ConfigBundle {
            prompt_bundle: "x".repeat(MAX_CONFIG_IDENTIFIER_BYTES + 1),
            ..ConfigBundle::default()
        },
        ConfigBundle {
            pii_language: "zh-CN\0hidden".to_owned(),
            ..ConfigBundle::default()
        },
        ConfigBundle {
            vector_space_fingerprint: " ".to_owned(),
            ..ConfigBundle::default()
        },
    ];
    for (index, bundle) in invalid_bundles.into_iter().enumerate() {
        let mut registry = ConfigRegistry::new().expect("registry must validate");
        let error = registry
            .register(bundle)
            .expect_err("invalid configuration identifier must fail");
        assert_eq!(
            error.code,
            ErrorCode::InvalidConfiguration,
            "invalid identifier fixture {index}"
        );
    }
}

/// Verifies typed policy validation, snapshot hashing, and impact classification.
/// 验证强类型策略校验、快照哈希与影响分类。
#[test]
fn memory_policy_and_snapshot_impacts_are_explicit() {
    let mut registry = ConfigRegistry::new().expect("registry must validate");
    let original = registry
        .register(ConfigBundle::default())
        .expect("default policy must compile");
    assert_eq!(original.memory_policy.schema_version, 10);
    assert!(!original.memory_policy_digest.is_empty());
    assert!(!original.prompt_digest.is_empty());
    assert!(!original.pii_digest.is_empty());
    assert!(!original.noise_digest.is_empty());

    let changed = registry
        .register(ConfigBundle {
            name: "changed-policy".to_owned(),
            memory_policy: MemoryPolicy {
                retrieval_policy: vulcan_memorymesh_core_contracts::RetrievalPolicy {
                    lexical_candidate_floor: 9,
                    ..vulcan_memorymesh_core_contracts::RetrievalPolicy::v1()
                },
                ..MemoryPolicy::default()
            },
            vector_space_fingerprint: "host-vector-space-v2".to_owned(),
            ..ConfigBundle::default()
        })
        .expect("changed policy must compile");
    assert_ne!(original.id, changed.id);
    assert_eq!(
        changed.impacts_compared_to(&original),
        vec![ConfigImpact::RuntimePolicy, ConfigImpact::VectorSpace]
    );

    // Changing only one producer contract or its raw semantics must change both the policy
    // digest and immutable snapshot identity.
    // 仅变更一个生产者契约或其原始评分语义时，策略摘要与不可变快照身份都必须变化。
    let pgvector_contract = ScoreContractRef {
        id: "vmm-pgvector-cosine-distance".to_owned(),
        version: 1,
    };
    let mut score_policy = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    score_policy.vector_contract = ScoreContractDescriptor {
        decision_score_mode: vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected,
        contract: pgvector_contract.clone(),
        raw_score_kind: RawScoreKind::CosineDistance,
        direction: ScoreDirection::LowerIsBetter,
    };
    for binding in score_policy
        .replace_min_similarity
        .iter_mut()
        .chain(&mut score_policy.hard_dedupe)
    {
        binding.contract = pgvector_contract.clone();
    }
    let score_changed = registry
        .register(ConfigBundle {
            name: "changed-score-producer".to_owned(),
            memory_policy: MemoryPolicy {
                retrieval_policy: score_policy,
                ..MemoryPolicy::default()
            },
            ..ConfigBundle::default()
        })
        .expect("score producer change must compile");
    assert_ne!(
        original.memory_policy_digest,
        score_changed.memory_policy_digest
    );
    assert_ne!(original.id, score_changed.id);

    let mut data_changed = (*original).clone();
    data_changed.pii_digest = "different-pii-digest".to_owned();
    assert_eq!(
        data_changed.impacts_compared_to(&original),
        vec![ConfigImpact::DataSemantics]
    );

    let error = registry
        .register(ConfigBundle {
            name: "invalid-policy".to_owned(),
            memory_policy: MemoryPolicy {
                retrieval_policy: vulcan_memorymesh_core_contracts::RetrievalPolicy {
                    lexical_candidate_floor: 0,
                    ..vulcan_memorymesh_core_contracts::RetrievalPolicy::v1()
                },
                ..MemoryPolicy::default()
            },
            ..ConfigBundle::default()
        })
        .expect_err("zero lexical candidate floor must fail");
    assert_eq!(error.code, ErrorCode::InvalidConfiguration);
}

/// Binds each new window independently and rejects missing or oversized closed-policy fields.
/// 独立绑定每个新增窗口，并拒绝封闭策略字段缺失或越界，不回退到旧策略。
#[test]
fn candidate_windows_are_digest_bound_and_validated() {
    let mut registry = ConfigRegistry::new().expect("registry");
    let original = registry.register(ConfigBundle::default()).expect("default");
    for (lexical, replacement, valid) in [
        (9, 5, true),
        (8, 6, true),
        (32, 32, true),
        (0, 5, false),
        (33, 5, false),
        (8, 0, false),
        (8, 33, false),
    ] {
        let mut bundle = ConfigBundle::default();
        bundle
            .memory_policy
            .retrieval_policy
            .lexical_candidate_floor = lexical;
        bundle.memory_policy.retrieval_policy.replacement_top_k = replacement;
        let result = registry.register(bundle);
        if valid {
            let snapshot = result.expect("valid window");
            assert_ne!(snapshot.id, original.id);
            assert_ne!(snapshot.memory_policy_digest, original.memory_policy_digest);
        } else {
            assert_eq!(
                result.expect_err("invalid window").code,
                ErrorCode::InvalidConfiguration
            );
        }
    }
    for retired_memory in [true, false] {
        let mut bundle = ConfigBundle::default();
        if retired_memory {
            bundle.memory_policy.schema_version = 1;
        } else {
            bundle.memory_policy.retrieval_policy.schema_version = 1;
        }
        assert_eq!(
            registry.register(bundle).expect_err("retired schema").code,
            ErrorCode::InvalidConfiguration
        );
    }
}

/// Verifies scalar narrowing is not hidden by registration and every new floating policy field rejects invalid numbers.
/// 验证注册过程不隐藏数值缩减，且所有新浮点策略字段都拒绝非法数值。
#[test]
fn binary64_policy_values_and_arithmetic_identity_are_explicit() {
    let mut registry = ConfigRegistry::new().expect("registry");
    let mut bundle = ConfigBundle::default();
    bundle.memory_policy.retrieval_policy.decay.shape = 1.350_000_123;
    bundle.memory_policy.retrieval_policy.decay.scale_hours = 0.250_000_123;
    bundle.memory_policy.retrieval_policy.replace_min_similarity[0].threshold = 0.800_009;
    let first = registry
        .register(bundle.clone())
        .expect("fractional policy");
    assert_eq!(
        first
            .memory_policy
            .retrieval_policy
            .decay
            .scale_hours
            .to_bits(),
        0.250_000_123_f64.to_bits()
    );
    bundle.memory_policy.retrieval_policy.decay.math_profile =
        vulcan_memorymesh_core_contracts::WeibullMathProfile::GoAmd64Sse2V1;
    let other = registry
        .register(bundle.clone())
        .expect("explicit source arithmetic");
    assert_ne!(first.id, other.id);
    assert_ne!(first.memory_policy_digest, other.memory_policy_digest);

    // Each selector names one distinct numeric policy input; no array index comes from an external caller.
    // 每个选择号标识一个独立数值策略输入，数组下标不来自外部调用方。
    for field in 0..17 {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            let mut invalid_bundle = bundle.clone();
            let policy = &mut invalid_bundle.memory_policy.retrieval_policy;
            match field {
                0 => policy.dual_channel_bonus = invalid,
                1 => policy.replace_min_similarity[0].threshold = invalid,
                2 => policy.hard_dedupe[0].threshold = invalid,
                3 => policy.decay.shape = invalid,
                4 => policy.decay.scale_hours = invalid,
                5 => policy.decay.min_multiplier = invalid,
                6 => policy.decay.reinforce_weight = invalid,
                7 => policy.decay.cross_session_boost = invalid,
                8 => policy.decay.refresh_weight_factor = invalid,
                9 => policy.decay.level_scales[0] = invalid,
                10 => policy.decay.priority_scales[0] = invalid,
                11 => policy.decay.scope_scales[0] = invalid,
                12 => policy.context.support_step = invalid,
                13 => policy.context.support_cap = invalid,
                14 => policy.context.rebuttal_step = invalid,
                15 => policy.context.rebuttal_cap = invalid,
                _ => policy.mmr.lambda = invalid,
            }
            assert_eq!(
                registry
                    .register(invalid_bundle)
                    .expect_err("invalid scalar")
                    .code,
                ErrorCode::InvalidConfiguration,
                "field={field}"
            );
        }
    }
}

/// Verifies every retrieval state that could panic or become ambiguous is rejected at registration.
/// 验证所有可能触发 panic 或形成歧义的检索状态都会在注册阶段被拒绝。
#[test]
fn retrieval_policy_rejects_unsafe_or_ambiguous_states() {
    let mut invalid_policies = Vec::new();

    // A decay floor above one makes the runtime floating-point clamp panic.
    // 大于一的衰减下限会令运行时浮点 clamp 发生 panic。
    let mut invalid_floor = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    invalid_floor.decay.min_multiplier = 1.0001;
    invalid_policies.push(("invalid-decay-floor", invalid_floor));

    // Weibull shape and all three priority bands are closed policy inputs.
    // Weibull 形状与三个优先级档位均为封闭策略输入。
    let mut invalid_shape = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    invalid_shape.decay.shape = 0.0;
    invalid_policies.push(("invalid-decay-shape", invalid_shape));
    let mut incomplete_priority = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    incomplete_priority.decay.priority_scales.pop();
    invalid_policies.push(("incomplete-priority-bands", incomplete_priority));

    // Public and candidate windows must retain the authoritative VMM upper bound.
    // 公共结果与候选窗口必须保留权威 VMM 上限。
    let mut oversized_window = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    oversized_window.top_k_max = 33;
    oversized_window.candidate_pool_max = 33;
    invalid_policies.push(("oversized-retrieval-window", oversized_window));

    // Score identities and per-contract thresholds must remain unambiguous.
    // 评分身份与逐契约阈值必须保持无歧义。
    let mut colliding_contracts = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    colliding_contracts.lexical_contract = colliding_contracts.vector_contract.clone();
    invalid_policies.push(("colliding-score-contracts", colliding_contracts));
    let mut duplicate_threshold = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    duplicate_threshold
        .hard_dedupe
        .push(duplicate_threshold.hard_dedupe[0].clone());
    invalid_policies.push(("duplicate-score-threshold", duplicate_threshold));
    let mut oversized_bonus = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    oversized_bonus.dual_channel_bonus = 1.0001;
    invalid_policies.push(("oversized-dual-channel-bonus", oversized_bonus));

    for (name, retrieval_policy) in invalid_policies {
        let mut registry = ConfigRegistry::new().expect("registry must validate");
        let error = registry
            .register(ConfigBundle {
                name: name.to_owned(),
                memory_policy: MemoryPolicy {
                    retrieval_policy,
                    ..MemoryPolicy::default()
                },
                ..ConfigBundle::default()
            })
            .expect_err("unsafe retrieval policy must fail registration");
        assert_eq!(error.code, ErrorCode::InvalidConfiguration, "{name}");
    }
}

/// Verifies `PostAction` history and token budgets preserve the authoritative positive VMM contract.
/// 验证 `PostAction` 历史窗口与 Token 预算保持 VMM 权威实现要求的正数契约。
#[test]
fn postaction_policy_rejects_unbounded_zero_limits() {
    for (name, memory_policy) in [
        (
            "zero-postaction-history",
            MemoryPolicy {
                postaction_history_turn_limit: 0,
                ..MemoryPolicy::default()
            },
        ),
        (
            "zero-postaction-token-budget",
            MemoryPolicy {
                postaction_max_input_tokens: 0,
                ..MemoryPolicy::default()
            },
        ),
    ] {
        let mut registry = ConfigRegistry::new().expect("registry must validate");
        let error = registry
            .register(ConfigBundle {
                name: name.to_owned(),
                memory_policy,
                ..ConfigBundle::default()
            })
            .expect_err("zero PostAction bounds must fail registration");
        assert_eq!(error.code, ErrorCode::InvalidConfiguration, "{name}");
    }
}

/// Verifies independent PreCheck budgets are bounded and both affect immutable snapshot identity.
/// 验证独立 PreCheck 预算受边界约束，且两个字段都影响不可变快照身份。
#[test]
fn precheck_history_policy_is_independent_and_digest_bound() {
    let mut registry = ConfigRegistry::new().expect("registry");
    let original = registry
        .register(ConfigBundle::default())
        .expect("defaults");
    for (history, tokens) in [(2, 6000), (3, 4096)] {
        let changed = registry
            .register(ConfigBundle {
                memory_policy: MemoryPolicy {
                    precheck_history_turn_limit: history,
                    precheck_max_input_tokens: tokens,
                    ..MemoryPolicy::default()
                },
                ..ConfigBundle::default()
            })
            .expect("valid independent setting");
        assert_ne!(original.id, changed.id);
        assert_eq!(changed.memory_policy.postaction_history_turn_limit, 3);
        assert_eq!(changed.memory_policy.postaction_max_input_tokens, 6000);
    }
    for (history, tokens) in [(0, 6000), (65, 6000), (3, 0), (3, 131073)] {
        assert!(
            registry
                .register(ConfigBundle {
                    memory_policy: MemoryPolicy {
                        precheck_history_turn_limit: history,
                        precheck_max_input_tokens: tokens,
                        ..MemoryPolicy::default()
                    },
                    ..ConfigBundle::default()
                })
                .is_err()
        );
    }
}

/// Verifies moving one threshold between semantic sets always changes snapshot identity.
/// 验证在语义集合之间移动一项阈值必然改变快照身份。
#[test]
fn retrieval_threshold_set_boundaries_are_digest_bound() {
    let mut registry = ConfigRegistry::new().expect("registry must validate");
    let original = registry
        .register(ConfigBundle::default())
        .expect("default snapshot must compile");
    let mut moved_policy = vulcan_memorymesh_core_contracts::RetrievalPolicy::v1();
    moved_policy.min_similarity = std::mem::take(&mut moved_policy.replace_min_similarity);
    let moved = registry
        .register(ConfigBundle {
            name: "threshold-set-moved".to_owned(),
            memory_policy: MemoryPolicy {
                retrieval_policy: moved_policy,
                ..MemoryPolicy::default()
            },
            ..ConfigBundle::default()
        })
        .expect("moved threshold set must remain structurally valid");
    assert_ne!(original.memory_policy_digest, moved.memory_policy_digest);
    assert_ne!(original.id, moved.id);
}

/// Binds the fusion algorithm selection into snapshot identity, even when other parameters are unchanged.
/// 即使其他参数未变，也将融合算法选择绑定到快照身份，防止恢复时静默切换。
#[test]
fn rrf_score_modes_are_digest_bound() {
    let mut registry = ConfigRegistry::new().expect("registry must validate");
    let original = registry
        .register(ConfigBundle::default())
        .expect("default snapshot");
    let mut bundle = ConfigBundle {
        name: "fused-rank-only".to_owned(),
        ..ConfigBundle::default()
    };
    bundle.memory_policy.retrieval_policy.rrf_score_mode =
        vulcan_memorymesh_core_contracts::RrfScoreMode::FusedRankOnly;
    let alternate = registry.register(bundle).expect("explicit alternate mode");
    assert_ne!(
        original.memory_policy_digest,
        alternate.memory_policy_digest
    );
    assert_ne!(original.id, alternate.id);
}

/// Binds independent query/reviewer windows and exact score thresholds, rejecting obsolete or unbounded policies.
/// 绑定独立查询及送审窗口和精确评分阈值，拒绝过时或无界策略。
#[test]
fn precheck_policy_is_digest_bound_and_validated() {
    let mut registry = ConfigRegistry::new().expect("registry");
    let original = registry.register(ConfigBundle::default()).expect("default");
    for (recall, review, score, valid) in [
        (6, 12, 0.75, true),
        (5, 13, 0.75, true),
        (5, 12, f64::from_bits(0.75_f64.to_bits() + 1), true),
        (32, 256, 1.0, true),
        (1, 1, 0.0, true),
        (0, 12, 0.75, false),
        (33, 12, 0.75, false),
        (5, 0, 0.75, false),
        (5, 257, 0.75, false),
        (5, 12, -0.01, false),
        (5, 12, 1.01, false),
        (5, 12, f64::NAN, false),
        (5, 12, f64::INFINITY, false),
    ] {
        let mut bundle = ConfigBundle::default();
        bundle.memory_policy.precheck_policy = vulcan_memorymesh_core_contracts::PreCheckPolicy {
            recall_top_k: recall,
            review_candidate_limit: review,
            min_review_score: score,
        };
        let result = registry.register(bundle);
        if valid {
            let snapshot = result.expect("valid PreCheck policy");
            assert_ne!(snapshot.id, original.id);
            assert_ne!(snapshot.memory_policy_digest, original.memory_policy_digest);
        } else {
            assert_eq!(
                result.expect_err("invalid policy").code,
                ErrorCode::InvalidConfiguration
            );
        }
    }
    let mut obsolete = ConfigBundle::default();
    obsolete.memory_policy.schema_version = 4;
    assert_eq!(
        registry
            .register(obsolete)
            .expect_err("obsolete policy")
            .code,
        ErrorCode::InvalidConfiguration
    );
}
