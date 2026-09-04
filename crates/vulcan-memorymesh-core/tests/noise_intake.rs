//! Verifies single-turn Noise, timeline bypass, and persisted continuations through the public Core.
//! 通过公共 Core 验证单轮 Noise、时间线跳过及持久续体。

#![allow(clippy::expect_used, clippy::panic)]

use vulcan_memorymesh_core::{
    CommandResult, CommandStep, ConfigBundle, CoreCommand, CoreError, EffectRequest, EffectResult,
    HostDriver, HostEffect, HostEffectResult, IdempotencyKey, InMemoryStorage, MemoryMeshCore,
    MutationIdentity, MutationSourceKind, NoiseAudit, NoiseDecisionSource, OperationId,
    PostActionInputKind, PostActionIntakeCommand, ResourceLayer, Role, ScopeAccess, ScopeId,
    SessionSourceId, StoragePort, SubjectId, TimelineItem, VectorSpaceId, request_fingerprint,
};

/// Compiles explicit user and assistant rules with a small deterministic vector space.
/// 编译显式用户与助手规则，并使用小型确定性向量空间。
fn core() -> MemoryMeshCore {
    core_with_policy(vulcan_memorymesh_core::NoisePolicy::default())
}

/// Verifies audit metadata comes from the active immutable snapshot rather than a default identity.
/// 验证审计元数据来自活动不可变快照，而非默认身份。
fn assert_noise_identity(core: &MemoryMeshCore, noise: &NoiseAudit) {
    let snapshot = core.active_snapshot().expect("snapshot");
    assert_eq!(noise.config_snapshot_id, snapshot.id);
    assert_eq!(noise.rules_digest, snapshot.noise_digest);
    assert_eq!(noise.rule_schema_version, "1.0.0");
    assert!(noise.confidence.is_finite() && noise.confidence >= 0.0);
}

/// Creates the same immutable rules with an explicitly selected admission policy.
/// 使用显式选择的准入策略创建相同不可变规则。
fn core_with_policy(noise_policy: vulcan_memorymesh_core::NoisePolicy) -> MemoryMeshCore {
    let core = MemoryMeshCore::new().expect("Core");
    let mut layer = ResourceLayer::default();
    layer.insert("noise_rules/common.json", br#"{"language":"common","version":"1.0.0","categories":[{"name":"user_meta","targets":["user"],"patterns":["^IGNORE$"],"phrases":["User Prototype"]},{"name":"assistant_denial","targets":["assistant"],"patterns":["^DENY$"],"phrases":["Assistant Prototype"]}]}"#.to_vec()).expect("common rules");
    layer
        .insert(
            "noise_rules/zh-CN.json",
            br#"{"language":"zh-CN","version":"1.0.0","categories":[]}"#.to_vec(),
        )
        .expect("locale rules");
    core.register_config(ConfigBundle {
        noise_policy,
        name: "noise-test".to_owned(),
        embedding_dimension: 3,
        user_layer: layer,
        ..ConfigBundle::default()
    })
    .expect("register");
    core.activate_config("noise-test").expect("activate");
    core
}

/// Builds one fingerprint-bound command without guessing its source shape from its messages.
/// 构造一个绑定指纹的命令，不从消息猜测其来源形态。
fn command(
    core: &MemoryMeshCore,
    kind: PostActionInputKind,
    messages: &[(Role, &str)],
    key: &str,
) -> CoreCommand {
    let scope = ScopeId::new("scope-noise").expect("scope");
    let mut command = CoreCommand::PostActionIntake(PostActionIntakeCommand {
        input_kind: kind,
        subject_id: SubjectId::new("subject-noise").expect("subject"),
        scope_access: ScopeAccess {
            write_scope: scope.clone(),
            visible_scopes: vec![scope],
        },
        session_source_id: SessionSourceId::new("session-noise").expect("session"),
        vector_space_id: VectorSpaceId::new("vector-noise").expect("vector space"),
        timeline: messages
            .iter()
            .map(|(role, text)| TimelineItem {
                role: *role,
                content: (*text).to_owned(),
                occurred_at_ns: 10,
            })
            .collect(),
        completed_at_ns: 10,
        mutation_identity: MutationIdentity {
            operation_id: OperationId::new(key).expect("operation"),
            idempotency_key: IdempotencyKey::new(key).expect("key"),
            request_fingerprint: String::new(),
            source_kind: MutationSourceKind::HostRpc,
            request_source_id: "noise-test".to_owned(),
            observed_at_ns: 10,
        },
    });
    let fingerprint = request_fingerprint(
        &command,
        core.active_snapshot().expect("snapshot").id.as_str(),
    );
    if let CoreCommand::PostActionIntake(payload) = &mut command {
        payload.mutation_identity.request_fingerprint = fingerprint;
    }
    command
}

/// Supplies deterministic vectors and the reference storage while recording exact embedding inputs.
/// 提供确定性向量与参考存储，同时记录精确嵌入输入。
#[derive(Default)]
struct NoiseHost {
    /// Retains committed turns, prototype bundles, and receipts.
    /// 保留已提交轮次、原型束与收据。
    storage: InMemoryStorage,
    /// Records embedding calls to distinguish prototypes from runtime queries.
    /// 记录嵌入调用，以区分原型与运行时查询。
    embeddings: Vec<Vec<String>>,
}

impl HostDriver for NoiseHost {
    /// Returns storage outcomes and unit-scale vectors; extreme arithmetic is tested against VMM separately.
    /// 返回存储结果与单位尺度向量；极端数值单独与VMM对照测试。
    fn execute_effect(&mut self, request: EffectRequest) -> Result<EffectResult, CoreError> {
        let result = match request.effect {
            HostEffect::StorageQuery(query) => {
                HostEffectResult::StorageQuery(self.storage.query(&query)?)
            }
            HostEffect::StorageMutation(batch) => {
                HostEffectResult::StorageMutation(self.storage.mutate(&batch)?)
            }
            HostEffect::Embedding(effect) => {
                assert_eq!(effect.expected_dimension, 3);
                self.embeddings.push(effect.inputs.clone());
                let vectors: Vec<Vec<f32>> = effect
                    .inputs
                    .iter()
                    .map(|text| match text.as_str() {
                        "User Prototype" => vec![1.0, 0.0, 0.0],
                        "Assistant Prototype" | "semantic denial" => vec![0.0, 1.0, 0.0],
                        "borderline user" => vec![0.5, 0.0, 0.866_025_4],
                        _ => vec![0.0, 0.0, 1.0],
                    })
                    .collect();
                HostEffectResult::Embedding(vectors.into())
            }
            other => panic!("Intake must not emit LLM or unrelated effects: {other:?}"),
        };
        Ok(EffectResult {
            effect_id: request.effect_id,
            result,
        })
    }
}

/// Checks that assistant regexes never gate native intake and source timelines still bypass Noise.
/// 检查助手正则绝不门控原生登记，且来源时间线仍跳过 Noise。
#[test]
fn assistant_regex_is_ignored_and_timeline_bypass_is_preserved() {
    let core = core();
    for kind in [
        PostActionInputKind::SingleTurn,
        PostActionInputKind::Timeline,
    ] {
        let messages = [
            (Role::User, "useful information"),
            (Role::Assistant, "DENY"),
        ];
        let mut host = NoiseHost::default();
        let result = core
            .run_to_completion(command(&core, kind, &messages, "intake-kind"), &mut host)
            .expect("intake");
        let CommandResult::PostActionAccepted(result) = result else {
            panic!("intake result")
        };
        assert!(!result.ignored_as_noise);
        assert!(result.turn_id.is_some());
        assert_noise_identity(&core, &result.noise);
        assert!(!result.noise.degraded);
        assert!(result.noise.unavailable_categories.is_empty());
        if kind == PostActionInputKind::Timeline {
            assert_eq!(result.noise.source, NoiseDecisionSource::TimelineBypass);
            assert_eq!(result.noise.role, None);
            assert_eq!(result.noise.category, None);
            assert_eq!(result.noise.confidence.to_bits(), 0.0_f64.to_bits());
            assert!(host.embeddings.is_empty());
        } else {
            assert_eq!(result.noise.source, NoiseDecisionSource::Semantic);
            assert_eq!(result.noise.role, Some(Role::User));
            assert_eq!(result.noise.category, None);
            assert_eq!(result.noise.confidence.to_bits(), 0.0_f64.to_bits());
            assert_eq!(
                host.embeddings,
                vec![vec!["User Prototype"], vec!["useful information"]]
            );
        }
        assert_eq!(host.storage.dataset().turns.len(), 1);
    }
}

/// Restores every semantic stage into a fresh Core and verifies user-only queries and cache reuse.
/// 将每个语义阶段恢复到新 Core，并验证仅查询用户提示词及缓存复用。
#[test]
fn user_only_semantics_and_shared_cache_survive_every_checkpoint() {
    let mut host = NoiseHost::default();
    for (key, messages) in [
        ("first", vec![(Role::User, "useful information")]),
        (
            "second",
            vec![
                (Role::User, "useful information"),
                (Role::Assistant, "semantic denial"),
            ],
        ),
    ] {
        let mut current = core();
        let (mut handle, mut step) = current
            .start(command(
                &current,
                PostActionInputKind::SingleTurn,
                &messages,
                key,
            ))
            .expect("start");
        for _ in 0..12 {
            let CommandStep::Effects(_) = &step else {
                break;
            };
            let checkpoint = current.export_checkpoint(&handle).expect("export");
            let fresh = core();
            let (fresh_handle, fresh_step) =
                fresh.restore_checkpoint(&checkpoint).expect("restore");
            assert_eq!(step, fresh_step);
            current = fresh;
            handle = fresh_handle;
            let CommandStep::Effects(effects) = fresh_step else {
                panic!("waiting")
            };
            assert_eq!(effects.len(), 1);
            let result = host
                .execute_effect(effects.into_iter().next().expect("effect"))
                .expect("Host");
            step = current.resume(&handle, result).expect("resume");
        }
        let CommandStep::Complete(result) = step else {
            panic!("bounded intake completion")
        };
        let CommandResult::PostActionAccepted(result) = *result else {
            panic!("intake result")
        };
        assert!(!result.ignored_as_noise);
        assert_noise_identity(&current, &result.noise);
        assert_eq!(result.noise.source, NoiseDecisionSource::Semantic);
        assert_eq!(result.noise.role, Some(Role::User));
        assert_eq!(result.noise.category, None);
        assert_eq!(result.noise.confidence.to_bits(), 0.0_f64.to_bits());
        assert!(!result.noise.degraded);
        current.release_command(&handle).expect("release");
    }
    assert_eq!(host.storage.dataset().turns.len(), 2);
    assert_eq!(
        host.embeddings,
        vec![
            vec!["User Prototype"],
            vec!["useful information"],
            vec!["useful information"],
        ]
    );
}

/// Rejects source-shape lies and proves the same flattened messages have distinct fingerprints.
/// 拒绝错误来源形态，并证明相同展平消息具有不同形态指纹。
#[test]
fn input_shape_is_validated_and_fingerprinted() {
    let core = core();
    let messages = [
        (Role::User, "first"),
        (Role::Assistant, "middle"),
        (Role::User, "follow-up"),
    ];
    let single = command(&core, PostActionInputKind::SingleTurn, &messages, "shape");
    let timeline = command(&core, PostActionInputKind::Timeline, &messages, "shape");
    assert_ne!(
        single
            .mutation_identity()
            .expect("identity")
            .request_fingerprint,
        timeline
            .mutation_identity()
            .expect("identity")
            .request_fingerprint
    );
    assert!(core.start(single).is_err());
    assert!(matches!(
        core.run_to_completion(timeline, &mut NoiseHost::default())
            .expect("timeline"),
        CommandResult::PostActionAccepted(_)
    ));
}

/// Classifies independently injectable failures along the Noise pipeline.
/// 分类 Noise 流水线上可独立注入的故障。
#[derive(Clone, Copy)]
enum NoiseFault {
    /// Fails only the first category, preserving assistant semantic evidence.
    /// 仅使首个类别失败，保留助手语义证据。
    UserPrototype,
    /// Fails every prototype category and therefore permits regex-only admission.
    /// 使全部原型类别失败，因此允许仅正则准入。
    AllPrototypes,
    /// Fails the runtime role embedding batch.
    /// 使运行时角色嵌入批次失败。
    Runtime,
    /// Reports an optional cache query transport failure.
    /// 报告可选缓存查询传输故障。
    CacheQuery,
    /// Supplies a correctly identified but incomplete derived cache.
    /// 提供身份正确但不完整的派生缓存。
    CacheIncomplete,
    /// Definitively rejects the optional cache write without committing it.
    /// 明确拒绝可选缓存写入且不提交。
    CacheRejected,
}

/// Injects a selected failure without hiding real storage and successful provider calls.
/// 注入选定故障，不隐藏真实存储及成功的提供方调用。
struct FaultHost {
    /// Owns ordinary capability behavior and audit observations.
    /// 拥有正常能力行为与审计观测。
    base: NoiseHost,
    /// Selects the explicit failure boundary for this run.
    /// 选择本次运行的显式故障边界。
    fault: NoiseFault,
}

impl HostDriver for FaultHost {
    /// Reports typed optional failures while preserving the exact request identity.
    /// 报告强类型可选故障，并保留精确请求身份。
    fn execute_effect(&mut self, request: EffectRequest) -> Result<EffectResult, CoreError> {
        use vulcan_memorymesh_core::{
            EffectFailure, EffectFailureKind, FailureClass, Mutation, MutationOutcome,
            NoisePrototype, StorageQuery, StorageQueryResult,
        };
        let failure_kind = match &request.effect {
            HostEffect::StorageQuery(StorageQuery::NoisePrototypeBundle { .. })
                if matches!(self.fault, NoiseFault::CacheQuery) =>
            {
                Some(EffectFailureKind::StorageQuery)
            }
            HostEffect::Embedding(effect) => {
                let prototype = effect.inputs.iter().any(|text| text.ends_with("Prototype"));
                let fails = match self.fault {
                    NoiseFault::UserPrototype => effect.inputs == ["User Prototype"],
                    NoiseFault::AllPrototypes => prototype,
                    NoiseFault::Runtime => !prototype,
                    _ => false,
                };
                fails.then_some(EffectFailureKind::Embedding)
            }
            _ => None,
        };
        let result = if let Some(kind) = failure_kind {
            HostEffectResult::Failure(EffectFailure {
                kind,
                class: FailureClass::Transport,
                retryable: true,
                allow_degrade: true,
                diagnostic: "injected optional Noise failure".to_owned(),
            })
        } else if let HostEffect::StorageQuery(StorageQuery::NoisePrototypeBundle { identity }) =
            &request.effect
        {
            if !matches!(self.fault, NoiseFault::CacheIncomplete) {
                return self.base.execute_effect(request);
            }
            HostEffectResult::StorageQuery(StorageQueryResult::NoisePrototypes {
                identity: identity.clone(),
                prototypes: vec![NoisePrototype {
                    category: "user_meta".to_owned(),
                    phrase: "User Prototype".to_owned(),
                    vector: vec![1.0, 0.0, 0.0],
                }],
            })
        } else if matches!(&request.effect, HostEffect::StorageMutation(batch) if batch.mutations.iter().any(|mutation| matches!(mutation, Mutation::PutNoisePrototypeBundle { .. })))
            && matches!(self.fault, NoiseFault::CacheRejected)
        {
            HostEffectResult::StorageMutation(MutationOutcome::Rejected {
                reason: "cache is read-only".to_owned(),
            })
        } else {
            return self.base.execute_effect(request);
        };
        Ok(EffectResult {
            effect_id: request.effect_id,
            result,
        })
    }
}

/// Verifies user-only Noise failures degrade without ever falling back to assistant content.
/// 验证仅面向用户的 Noise 故障会受控降级，且绝不回退到助手内容。
#[test]
fn noise_failures_preserve_user_only_admission() {
    for (fault, degraded, source) in [
        (
            NoiseFault::UserPrototype,
            true,
            NoiseDecisionSource::RegexOnlyFallback,
        ),
        (
            NoiseFault::AllPrototypes,
            true,
            NoiseDecisionSource::RegexOnlyFallback,
        ),
        (
            NoiseFault::Runtime,
            true,
            NoiseDecisionSource::RegexOnlyFallback,
        ),
        (NoiseFault::CacheQuery, true, NoiseDecisionSource::Semantic),
        (
            NoiseFault::CacheIncomplete,
            false,
            NoiseDecisionSource::Semantic,
        ),
        (
            NoiseFault::CacheRejected,
            true,
            NoiseDecisionSource::Semantic,
        ),
    ] {
        let core = core();
        let mut host = FaultHost {
            base: NoiseHost::default(),
            fault,
        };
        let result = core
            .run_to_completion(
                command(
                    &core,
                    PostActionInputKind::SingleTurn,
                    &[
                        (Role::User, "useful information"),
                        (Role::Assistant, "semantic denial"),
                    ],
                    "fault",
                ),
                &mut host,
            )
            .expect("degraded intake");
        let CommandResult::PostActionAccepted(result) = result else {
            panic!("intake result")
        };
        assert!(!result.ignored_as_noise);
        assert_noise_identity(&core, &result.noise);
        assert_eq!(result.noise.degraded, degraded);
        assert_eq!(result.noise.source, source);
        let unavailable: &[&str] = match fault {
            NoiseFault::UserPrototype => &["user_meta"],
            NoiseFault::AllPrototypes => &["user_meta"],
            _ => &[],
        };
        assert_eq!(result.noise.unavailable_categories, unavailable);
        if source == NoiseDecisionSource::Semantic {
            assert_eq!(result.noise.role, Some(Role::User));
        } else {
            assert_eq!(result.noise.role, None);
        }
        assert_eq!(result.noise.category, None);
        assert_eq!(result.noise.confidence.to_bits(), 0.0_f64.to_bits());
        assert_eq!(host.base.storage.dataset().turns.len(), 1);
    }
}

/// Disabled and regex-only policies never request embeddings, while custom thresholds affect decisions.
/// 禁用及仅正则策略不请求嵌入，而自定义阈值实际影响决策。
#[test]
fn intake_uses_snapshot_noise_policy() {
    use vulcan_memorymesh_core::NoisePolicy;
    for (policy, user, assistant, ignored) in [
        (
            NoisePolicy {
                enabled: false,
                ..NoisePolicy::default()
            },
            "IGNORE",
            "DENY",
            false,
        ),
        (
            NoisePolicy {
                semantic_enabled: false,
                ..NoisePolicy::default()
            },
            "IGNORE",
            "DENY",
            true,
        ),
        (
            NoisePolicy {
                semantic_enabled: false,
                ..NoisePolicy::default()
            },
            "useful information",
            "semantic denial",
            false,
        ),
    ] {
        let core = core_with_policy(policy);
        let mut host = NoiseHost::default();
        let result = core
            .run_to_completion(
                command(
                    &core,
                    PostActionInputKind::SingleTurn,
                    &[(Role::User, user), (Role::Assistant, assistant)],
                    "policy",
                ),
                &mut host,
            )
            .expect("intake");
        let CommandResult::PostActionAccepted(result) = result else {
            panic!("result")
        };
        assert_eq!(result.ignored_as_noise, ignored);
        assert_noise_identity(&core, &result.noise);
        assert_eq!(
            result.noise.source,
            if policy.enabled {
                NoiseDecisionSource::Regex
            } else {
                NoiseDecisionSource::Disabled
            }
        );
        assert!(!result.noise.degraded);
        assert!(host.embeddings.is_empty());
    }
    for (threshold, ignored) in [(0.4, true), (0.8, false)] {
        let core = core_with_policy(NoisePolicy {
            default_semantic_threshold: threshold,
            ..NoisePolicy::default()
        });
        let result = core
            .run_to_completion(
                command(
                    &core,
                    PostActionInputKind::SingleTurn,
                    &[
                        (Role::User, "borderline user"),
                        (Role::Assistant, "useful answer"),
                    ],
                    "threshold",
                ),
                &mut NoiseHost::default(),
            )
            .expect("threshold intake");
        let CommandResult::PostActionAccepted(result) = result else {
            panic!("result")
        };
        assert_eq!(result.ignored_as_noise, ignored);
        assert_noise_identity(&core, &result.noise);
        assert_eq!(result.noise.source, NoiseDecisionSource::Semantic);
        assert_eq!(result.noise.category.as_deref(), Some("user_meta"));
        assert!((result.noise.confidence - 0.5).abs() < 0.000_01);
    }
}
