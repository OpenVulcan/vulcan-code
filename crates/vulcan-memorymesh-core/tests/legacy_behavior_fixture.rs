//! Source-derived algorithm fixtures and separately labelled packaged-resource regressions.
//! 源码推导算法 Fixture 与单独标注的内置资源回归测试。

#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use serde::Deserialize;
use vulcan_memorymesh_core::{
    CandidateChannelResult, CommandResult, CommandStep, ConfigBundle, ContextEvidenceGroup,
    CoreCommand, EffectResult, HostEffect, HostEffectResult, MemoryCategory, MemoryId, MemoryLevel,
    MemoryMeshCore, MemoryRecord, MemoryScopeClass, MemorySearchCommand, MemorySourceKind,
    MemoryStability, MemoryState, PhysicalCandidate, PhysicalCandidateScore, RawScoreKind, Role,
    ScopeAccess, ScopeId, ScoreDirection, SessionSourceId, StorageQuery, StorageQueryResult,
    SubjectId, TurnId, VectorSpaceId, WeibullDecayPolicy, normalize_text,
};
use vulcan_memorymesh_core_engine::weibull_multiplier;

/// Carries the versioned legacy behavior fixture envelope.
/// 携带版本化旧行为 Fixture 封套。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyFixture {
    /// Identifies the fixture schema.
    /// 标识 Fixture Schema。
    schema_version: u32,
    /// Limits what passing this fixture proves, independently of other gates.
    /// 独立于其他门禁，限定该 Fixture 通过所能证明的范围。
    evidence_scope: String,
    /// Identifies the read-only source repository.
    /// 标识只读来源仓库。
    source_repository: String,
    /// Identifies the exact source commit.
    /// 标识精确来源提交。
    source_commit: String,
    /// Carries normalization golden cases.
    /// 携带规范化 Golden 用例。
    normalization_cases: Vec<NormalizationCase>,
    /// Carries the actual upstream Weibull inputs and ordering assertion.
    /// 携带真实上游 Weibull 输入与排序断言。
    weibull_case: WeibullCase,
    /// Carries PII behavior cases.
    /// 携带 PII 行为用例。
    pii_cases: Vec<PiiCase>,
    /// Carries Noise behavior cases.
    /// 携带 Noise 行为用例。
    noise_cases: Vec<NoiseCase>,
}

/// Defines one exact normalization input and expected output.
/// 定义一个精确规范化输入与预期输出。
#[derive(Deserialize)]
struct NormalizationCase {
    /// Names the legacy source test.
    /// 命名旧来源测试。
    source_test: String,
    /// Carries exact input text.
    /// 携带精确输入文本。
    input: String,
    /// Carries exact expected text.
    /// 携带精确预期文本。
    expected: String,
}

/// Defines one source-derived Weibull case, preserving the upstream assertion strength.
/// 定义一个源码推导的 Weibull 用例，保持与上游相同的断言强度。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeibullCase {
    /// Carries the exact upstream search query.
    /// 携带精确上游查询。
    query: String,
    /// Names the legacy source test.
    /// 命名旧来源测试。
    source_test: String,
    /// Explains mechanical clock and domain translations, not invented expected scores.
    /// 解释机械时钟及领域映射，不虚构预期分数。
    adaptation: String,
    /// Pins the clock replacing upstream time.Now while preserving relative ages.
    /// 固定时钟以替代上游 time.Now，同时保留相对年龄。
    now_ms: i64,
    /// Carries exact `ConfigureDecay` values and its source-defined scale factors.
    /// 携带精确 `ConfigureDecay` 值及源码定义的尺度因子。
    policy: WeibullDecayPolicy,
    /// Carries the two actual upstream rows.
    /// 携带两条真实上游记录。
    memories: Vec<WeibullMemory>,
    /// Carries the exact ordering asserted upstream.
    /// 携带上游断言的精确顺序。
    expected_order: Vec<String>,
}

/// Carries score-relevant facts from one upstream memory row.
/// 携带一条上游记忆记录中与评分有关的事实。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WeibullMemory {
    /// Identifies the exact source turn from the upstream row.
    /// 标识上游记录中的精确来源轮次。
    source_turn_id: String,
    /// Retains the source row ID as an opaque Core identity.
    /// 将来源记录 ID 保留为不透明 Core 身份。
    id: String,
    /// Carries the exact source abstract and details without normalization.
    /// 携带未经归一化的精确来源摘要和详情。
    summary: String,
    /// Carries the exact detailed text.
    /// 携带精确详情文本。
    details: String,
    /// Carries the normalized initial score.
    /// 携带规范化初始分数。
    base_score: f64,
    /// Maps P2/P1 to the documented Core priority bands.
    /// 将 P2/P1 映射为文档规定的 Core 优先级档。
    priority: u32,
    /// Retains the phase or stable source level.
    /// 保留来源的阶段或稳定级别。
    memory_level: MemoryLevel,
    /// Measures creation age relative to the frozen clock.
    /// 相对冻结时钟衡量创建年龄。
    created_days_ago: i64,
    /// Measures reinforcement age relative to the frozen clock.
    /// 相对冻结时钟衡量强化年龄。
    reinforced_days_ago: i64,
    /// Counts source reinforcements.
    /// 统计来源强化次数。
    reinforcement_count: u32,
    /// Counts source cross-session adoptions.
    /// 统计来源跨会话采纳次数。
    cross_session_adopted_count: u32,
}

/// Defines one PII input and forbidden cleartext contract.
/// 定义一个 PII 输入与禁止明文契约。
#[derive(Deserialize)]
struct PiiCase {
    /// Names the legacy source test.
    /// 命名旧来源测试。
    source_test: String,
    /// Carries raw text.
    /// 携带原始文本。
    input: String,
    /// Lists cleartext values forbidden after scrubbing.
    /// 列出清洗后禁止存在的明文值。
    forbidden_substrings: Vec<String>,
    /// Requires at least this many applied rules.
    /// 要求至少应用此数量的规则。
    minimum_redactions: usize,
}

/// Defines one role-sensitive Noise decision case.
/// 定义一个角色敏感的 Noise 决策用例。
#[derive(Deserialize)]
struct NoiseCase {
    /// Names the legacy source test.
    /// 命名旧来源测试。
    source_test: String,
    /// Carries the exact role name.
    /// 携带精确角色名称。
    role: String,
    /// Carries exact text.
    /// 携带精确文本。
    input: String,
    /// Carries expected classification.
    /// 携带预期分类。
    expected_is_noise: bool,
    /// Carries expected exact category when classified.
    /// 分类时携带预期精确类别。
    expected_category: Option<String>,
}

/// Loads the one checked-in legacy fixture with deterministic UTF-8 decoding.
/// 使用确定性 UTF-8 解码加载唯一已检入旧 Fixture。
fn load_fixture() -> LegacyFixture {
    serde_json::from_str(include_str!("fixtures/deterministic_behavior.json"))
        .expect("legacy behavior fixture must decode")
}

/// Converts one closed fixture role without accepting guessed aliases.
/// 转换一个封闭 Fixture 角色且不接受猜测别名。
fn fixture_role(value: &str) -> Option<Role> {
    match value {
        "user" => Some(Role::User),
        "assistant" => Some(Role::Assistant),
        _ => None,
    }
}

/// Verifies only the explicitly identified source and resource cases, not adapter equivalence.
/// 仅验证显式标识的源码与资源用例，不声称证明适配器等价。
#[test]
fn legacy_behavior_fixture_matches_core_v1() {
    let fixture = load_fixture();
    assert_eq!(fixture.schema_version, 3);
    assert_eq!(
        fixture.evidence_scope,
        "source-derived-unit-cases-not-adapter-conformance"
    );
    assert_eq!(fixture.source_repository, "VulcanMemoryMesh");
    assert_eq!(
        fixture.source_commit,
        "82f05fa5e3c7657e770fefd84e220fba5654ce07"
    );
    for case in fixture.normalization_cases {
        assert_eq!(
            normalize_text(&case.input),
            case.expected,
            "{}",
            case.source_test
        );
    }
    verify_weibull_case(&fixture.weibull_case);
    let core = MemoryMeshCore::new().expect("Core must initialize");
    core.register_config(ConfigBundle {
        name: "legacy-fixture".to_owned(),
        prompt_bundle: "default_cn".to_owned(),
        pii_language: "zh-CN".to_owned(),
        noise_language: "zh-CN".to_owned(),
        ..ConfigBundle::default()
    })
    .expect("legacy fixture configuration must register");
    core.activate_config("legacy-fixture")
        .expect("legacy fixture configuration must activate");
    for case in fixture.pii_cases {
        let result = core
            .scrub_pii(&case.input)
            .expect("PII scrub must complete");
        assert!(
            case.forbidden_substrings
                .iter()
                .all(|forbidden| !result.text.contains(forbidden)),
            "{}",
            case.source_test
        );
        assert!(
            result.redactions.len() >= case.minimum_redactions,
            "{}",
            case.source_test
        );
    }
    for case in fixture.noise_cases {
        let result = core
            .detect_noise(
                fixture_role(&case.role).expect("fixture role must be supported"),
                &case.input,
            )
            .expect("Noise detection must complete");
        assert_eq!(
            result.is_noise, case.expected_is_noise,
            "{}",
            case.source_test
        );
        assert_eq!(
            result.category, case.expected_category,
            "{}",
            case.source_test
        );
    }
}

/// Runs the production Weibull kernel over translated source facts and its actual ordering assertion.
/// 对映射后的来源事实执行生产 Weibull 内核，并使用上游真实的排序断言。
fn verify_weibull_case(case: &WeibullCase) {
    assert!(!case.adaptation.is_empty());
    assert_eq!(case.memories.len(), 2);
    // The checked-in upstream fixture remains millisecond-authored; convert once at this adapter boundary.
    // 检入的上游夹具仍以毫秒编写；只在此适配边界转换一次。
    let now_ns = case
        .now_ms
        .checked_mul(1_000_000)
        .expect("fixture clock must fit the nanosecond contract");
    let mut scores = case
        .memories
        .iter()
        .map(|row| {
            let created_at_ns = now_ns - row.created_days_ago * 86_400_000_000_000;
            let memory = MemoryRecord {
                id: MemoryId::new(&row.id).expect("fixture identity"),
                access_class: vulcan_memorymesh_core::MemoryAccessClass::SubjectPrivate,
                source_subject_id: Some(SubjectId::new("subject-7").expect("fixture subject")),
                scope_id: ScopeId::new("project-9").expect("fixture scope"),
                scope_class: MemoryScopeClass::Environment,
                category: MemoryCategory::ProjectContext,
                summary: row.summary.clone(),
                content: row.details.clone(),
                source_kind: MemorySourceKind::Turn,
                origin_session_source_id: Some(SessionSourceId::new("12").expect("source session")),
                priority: row.priority,
                stability: MemoryStability::Stable,
                tags: Vec::new(),
                evidence: Vec::new(),
                evidence_source: None,
                state: MemoryState::Active,
                importance: 0.5,
                confidence: Some(1.0),
                created_at_ns,
                updated_at_ns: now_ns - row.reinforced_days_ago * 86_400_000_000_000,
                expires_at_ns: None,
                last_recalled_at_ns: None,
                reinforcement_count: u64::from(row.reinforcement_count),
                memory_level: row.memory_level,
                refresh_weight: 0,
                dedupe_hash: None,
                recalled_count: 0,
                adopted_count: 0,
                cross_session_adopted_count: u64::from(row.cross_session_adopted_count),
                decay_disabled: false,
                status_reason: None,
                last_adopted_at_ns: None,
                last_reinforced_at_ns: Some(now_ns - row.reinforced_days_ago * 86_400_000_000_000),
                source_turn_id: Some(TurnId::new(&row.source_turn_id).expect("source turn")),
                version: 1,
                vector_space_id: Some(
                    VectorSpaceId::new("fixture-vector").expect("fixture vector space"),
                ),
                embedding: Some(vec![0.1, 0.2, 0.3]),
            };
            let score = row.base_score * weibull_multiplier(&memory, &case.policy, now_ns);
            (memory, score)
        })
        .collect::<Vec<_>>();
    scores.sort_by(|left, right| right.1.total_cmp(&left.1));
    assert_eq!(
        scores
            .iter()
            .map(|row| row.0.id.to_string())
            .collect::<Vec<_>>(),
        case.expected_order,
        "{}",
        case.source_test
    );
    assert!(scores[0].1 > scores[1].1);
    verify_weibull_search(
        case,
        now_ns,
        &scores.into_iter().map(|row| row.0).collect::<Vec<_>>(),
    );
}

/// Exercises the same fixture through the public Search state machine with explicit host facts.
/// 使用显式宿主事实，让同一 Fixture 经过公开 Search 状态机。
#[allow(clippy::too_many_lines)]
fn verify_weibull_search(case: &WeibullCase, now_ns: i64, records: &[MemoryRecord]) {
    let core = MemoryMeshCore::new().expect("Core initializes");
    let mut bundle = ConfigBundle {
        name: "weibull-fixture".to_owned(),
        embedding_dimension: 3,
        ..ConfigBundle::default()
    };
    // Match the source case: vector-only, no reranker and no MMR service.
    // 与来源用例一致：仅向量，无重排器，也不启用 MMR。
    let policy = &mut bundle.memory_policy.retrieval_policy;
    policy.hybrid_enabled = false;
    policy.rerank_top_n = 0;
    policy.mmr.enabled = false;
    policy.decay = case.policy.clone();
    core.register_config(bundle).expect("fixture config");
    core.activate_config("weibull-fixture")
        .expect("fixture activates");
    let (handle, mut step) = core
        .start(CoreCommand::MemorySearch(MemorySearchCommand {
            subject_id: SubjectId::new("subject-7").expect("subject"),
            scope_access: ScopeAccess {
                write_scope: ScopeId::new("project-9").expect("scope"),
                visible_scopes: vec![ScopeId::new("project-9").expect("scope")],
            },
            query: case.query.clone(),
            limit: 2,
            vector_space_id: VectorSpaceId::new("fixture-vector").expect("vector space"),
            boundary: vulcan_memorymesh_core_contracts::CandidateQueryBoundary {
                candidate_scope: Some(vulcan_memorymesh_core_contracts::MemoryCandidateScope {
                    scope_ids: vec![ScopeId::new("project-9").expect("scope")],
                    origin_session_source_id: None,
                }),
                session_recall_boundary: None,
            },
            now_ns,
        }))
        .expect("search starts");
    for _ in 0..10 {
        let CommandStep::Effects(mut effects) = step else {
            break;
        };
        assert_eq!(effects.len(), 1);
        let request = effects.pop().expect("single effect");
        let result = match request.effect {
            HostEffect::Embedding(effect) => {
                assert_eq!(effect.inputs, vec![case.query.clone()]);
                HostEffectResult::Embedding(vec![vec![0.1, 0.2, 0.3]].into())
            }
            HostEffect::StorageQuery(StorageQuery::VectorCandidates {
                expected_contract, ..
            }) => HostEffectResult::StorageQuery(StorageQueryResult::VectorCandidates(
                CandidateChannelResult {
                    candidates: case
                        .memories
                        .iter()
                        .enumerate()
                        .map(|(index, row)| PhysicalCandidate {
                            memory_id: row.id.clone(),
                            physical_rank: u32::try_from(index).expect("bounded fixture"),
                            score: PhysicalCandidateScore {
                                raw_value: row.base_score,
                                decision_score: Some(row.base_score),
                                raw_score_kind: RawScoreKind::CosineSimilarity,
                                direction: ScoreDirection::HigherIsBetter,
                                contract: expected_contract.clone(),
                            },
                        })
                        .collect(),
                    memories: Vec::new(),
                },
            )),
            HostEffect::StorageQuery(StorageQuery::MemoryRecordBatch { memory_ids, .. }) => {
                HostEffectResult::StorageQuery(StorageQueryResult::MemoryRecordBatch(
                    memory_ids
                        .iter()
                        .map(|id| records.iter().find(|row| row.id == *id).cloned())
                        .collect(),
                ))
            }
            HostEffect::StorageQuery(StorageQuery::ContextEvidenceBatch { memory_ids, .. }) => {
                HostEffectResult::StorageQuery(StorageQueryResult::ContextEvidenceBatch(
                    memory_ids
                        .into_iter()
                        .map(|id| ContextEvidenceGroup {
                            memory_id: id,
                            evidence: Vec::new(),
                        })
                        .collect(),
                ))
            }
            unexpected => panic!("unexpected source-fixture effect: {unexpected:?}"),
        };
        step = core
            .resume(
                &handle,
                EffectResult {
                    effect_id: request.effect_id,
                    result,
                },
            )
            .expect("source fixture resumes");
    }
    let CommandStep::Complete(result) = step else {
        panic!("bounded search did not finish")
    };
    let CommandResult::MemorySearch(hits) = *result else {
        panic!("search result required")
    };
    assert_eq!(
        hits.iter()
            .map(|hit| hit.memory.id.to_string())
            .collect::<Vec<_>>(),
        case.expected_order
    );
    assert!(hits[0].final_score > hits[1].final_score);
    core.release_command(&handle)
        .expect("release fixture command");
}
