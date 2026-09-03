//! Literal VMM score-producer contract fixtures.
//! VMM 评分生产者字面量契约 Fixture。

//! Literal examples validate contracts, not live databases: three producers project scores and two PG lexical producers do not.
//! 字面量示例验证契约而非实库：三类生产者提供投影分，两类 PG 词法生产者不提供。
//! The legacy upstream pin remains separate from the newer Host bindings; static source markers do not prove runtime equivalence.
//! 历史上游基线与较新的宿主绑定分别记录；静态源码标记不能证明运行时等价。
//! Legacy `SQLite` raw-score ownership is internal/platform/ffi/sqliteffi/sqliteffi.go; scoped projection belongs to the adapter below.
//! 历史 `SQLite` 原分归属 internal/platform/ffi/sqliteffi/sqliteffi.go；范围投影属于下列适配器。

#![allow(clippy::expect_used)]

use vulcan_memorymesh_core_contracts::{
    PhysicalCandidate, PhysicalCandidateScore, RawScoreKind, ScoreContractDescriptor,
    ScoreContractRef, ScoreDirection, validate_candidate_channel,
    validate_candidate_channel_contract,
};

/// Freezes the exact VMM commit from which producer semantics were derived.
/// 冻结派生生产者语义时使用的精确 VMM 提交。
const FROZEN_VMM_HEAD: &str = "82f05fa5e3c7657e770fefd84e220fba5654ce07";

/// Records the verified VMM Host-binding source checkpoint, separate from the legacy baseline.
/// 记录已核实的 VMM 宿主绑定源码检查点，与历史基线分开。
const HOST_BINDING_VMM_HEAD: &str = "6f9c38870a3241dfb1ea812ae4621a2cebd8cca7";

/// Carries one literal Host-output row in physical rank order.
/// 携带一条按物理排名排列的宿主输出字面量行。
#[derive(Clone, Copy)]
struct FixtureRow {
    /// Carries the unprojected backend value retained for diagnostics.
    /// 携带为诊断保留的未投影后端值。
    raw_value: f64,
    /// Carries the Host-projected decision value consumed by Core.
    /// 携带由 Core 使用的宿主投影决策值。
    decision_score: Option<f64>,
}

/// Carries one source-grounded physical producer contract.
/// 携带一个有源码依据的物理生产者契约。
struct ProducerFixture {
    /// Names the producer and stable fixture prefix.
    /// 命名生产者与稳定 Fixture 前缀。
    name: &'static str,
    /// Records the authoritative VMM source path used to freeze the facts.
    /// 记录用于冻结事实的权威 VMM 源码路径。
    source_path: &'static str,
    /// Declares the exact identity, raw kind, and direction expected from the Host.
    /// 声明宿主应返回的精确身份、原始类别与方向。
    descriptor: ScoreContractDescriptor,
    /// Carries literal Host outputs without reimplementing the producer formula in Core.
    /// 携带宿主输出字面量且不在 Core 内重写生产者公式。
    rows: [FixtureRow; 2],
}

/// Builds one ordered candidate channel directly from literal Host outputs.
/// 直接从宿主输出字面量构建一个有序候选通道。
fn channel(producer: &ProducerFixture) -> Vec<PhysicalCandidate> {
    producer
        .rows
        .iter()
        .enumerate()
        .map(|(rank, row)| PhysicalCandidate {
            memory_id: format!("{}-memory-{rank}", producer.name),
            score: PhysicalCandidateScore {
                raw_value: row.raw_value,
                raw_score_kind: producer.descriptor.raw_score_kind,
                direction: producer.descriptor.direction,
                decision_score: row.decision_score,
                contract: producer.descriptor.contract.clone(),
            },
            physical_rank: u32::try_from(rank).expect("bounded fixture rank"),
        })
        .collect()
}

/// Freezes the five split-channel producers used by a Core-backed VMM Host adapter.
/// 冻结由 Core 驱动的 VMM 宿主适配器使用的五类分离通道生产者。
#[allow(clippy::too_many_lines)]
fn producers() -> Vec<ProducerFixture> {
    vec![
        ProducerFixture {
            name: "sqlite-fts",
            source_path: "internal/adapters/outbound/vldb_sqlite/core_lexical_candidates.go",
            descriptor: ScoreContractDescriptor {
                decision_score_mode:
                    vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected,
                contract: ScoreContractRef {
                    id: "vmm-sqlite-fts-scoped-rank".to_owned(),
                    version: 1,
                },
                raw_score_kind: RawScoreKind::Bm25,
                direction: ScoreDirection::LowerIsBetter,
            },
            // VMM freezes rank projection after scoped filtering; native negated BM25 is not the decision score.
            // VMM 在范围过滤后冻结排名投影；原生 BM25 取负值不是决策分。
            rows: [
                FixtureRow {
                    raw_value: -2.5,
                    decision_score: Some(1.0),
                },
                FixtureRow {
                    raw_value: -1.25,
                    decision_score: Some(0.75),
                },
            ],
        },
        ProducerFixture {
            name: "lancedb",
            source_path: "internal/adapters/outbound/vldb_lancedb/store.go",
            descriptor: ScoreContractDescriptor {
                decision_score_mode:
                    vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected,
                contract: ScoreContractRef {
                    id: "vmm-lancedb-l2-distance".to_owned(),
                    version: 1,
                },
                // The pinned vldb-lancedb v0.1.5 leaves the metric unset; its locked LanceDB 0.27.2 defaults to L2.
                // 固定的 vldb-lancedb v0.1.5 未设置距离类型；锁定的 LanceDB 0.27.2 默认为 L2。
                raw_score_kind: RawScoreKind::L2Distance,
                direction: ScoreDirection::LowerIsBetter,
            },
            rows: [
                FixtureRow {
                    raw_value: 0.25,
                    decision_score: Some(0.80),
                },
                FixtureRow {
                    raw_value: 1.00,
                    decision_score: Some(0.50),
                },
            ],
        },
        ProducerFixture {
            name: "pgvector",
            source_path: "internal/adapters/outbound/vldb_postgres/vector.go",
            descriptor: ScoreContractDescriptor {
                decision_score_mode:
                    vulcan_memorymesh_core_contracts::DecisionScoreMode::HostProjected,
                contract: ScoreContractRef {
                    id: "vmm-pgvector-cosine-distance".to_owned(),
                    version: 1,
                },
                raw_score_kind: RawScoreKind::CosineDistance,
                direction: ScoreDirection::LowerIsBetter,
            },
            rows: [
                FixtureRow {
                    raw_value: 0.25,
                    decision_score: Some(0.80),
                },
                FixtureRow {
                    raw_value: 1.00,
                    decision_score: Some(0.50),
                },
            ],
        },
        ProducerFixture {
            name: "paradedb",
            source_path: "internal/adapters/outbound/vldb_postgres/dialect_paradedb.go",
            descriptor: ScoreContractDescriptor {
                decision_score_mode: vulcan_memorymesh_core_contracts::DecisionScoreMode::RankOnly,
                contract: ScoreContractRef {
                    id: "vmm-paradedb-bm25-rank".to_owned(),
                    version: 1,
                },
                raw_score_kind: RawScoreKind::Bm25,
                direction: ScoreDirection::HigherIsBetter,
            },
            rows: [
                FixtureRow {
                    raw_value: 12.0,
                    decision_score: None,
                },
                FixtureRow {
                    raw_value: 4.0,
                    decision_score: None,
                },
            ],
        },
        ProducerFixture {
            name: "postgres-trgm",
            source_path: "internal/adapters/outbound/vldb_postgres/dialect_standard.go",
            descriptor: ScoreContractDescriptor {
                decision_score_mode: vulcan_memorymesh_core_contracts::DecisionScoreMode::RankOnly,
                contract: ScoreContractRef {
                    id: "vmm-pg-trigram-rank".to_owned(),
                    version: 1,
                },
                raw_score_kind: RawScoreKind::TrigramSimilarity,
                direction: ScoreDirection::HigherIsBetter,
            },
            rows: [
                FixtureRow {
                    raw_value: 0.80,
                    decision_score: None,
                },
                FixtureRow {
                    raw_value: 0.40,
                    decision_score: None,
                },
            ],
        },
    ]
}

/// Verifies every split-channel producer is bound to unique and truthful raw semantics.
/// 验证每个分离通道生产者均绑定唯一且真实的原始评分语义。
#[test]
fn every_producer_channel_passes_its_exact_descriptor() {
    assert_eq!(HOST_BINDING_VMM_HEAD.len(), 40);
    assert_eq!(
        FROZEN_VMM_HEAD.len(),
        40,
        "baseline must be a full Git SHA-1"
    );
    let producers = producers();
    let mut identities = std::collections::BTreeSet::new();
    for producer in &producers {
        assert!(
            identities.insert((
                producer.descriptor.contract.id.clone(),
                producer.descriptor.contract.version,
            )),
            "producer identities must be unique"
        );
        assert!(
            producer.source_path.starts_with("internal/"),
            "fixture must name its VMM source"
        );
        let channel = channel(producer);
        validate_candidate_channel(&channel).expect("literal producer channel must be ordered");
        validate_candidate_channel_contract(&channel, &producer.descriptor)
            .expect("literal producer channel must match its configured semantics");
    }
}

/// Verifies the two legacy `PostgreSQL` combined paths are comparison facts, not Core projections.
/// 验证两条旧 `PostgreSQL` 组合路径是对比事实而不是 Core 投影公式。
#[test]
fn legacy_combined_sql_outputs_are_literal_shadow_comparison_facts() {
    // Both PostgreSQL dialects compute SQL-side RRF and VMM then projects the final rank to a
    // reviewer score. A Core Host must call the split channels instead; these tuples are kept
    // only for shadow comparison and never registered as vector or lexical contracts.
    // 两种 PostgreSQL 方言都在 SQL 内计算 RRF，随后 VMM 将最终排名投影为审核分。Core 宿主
    // 必须调用分离通道；这些元组仅用于影子对比，绝不注册为向量或词法契约。
    let paradedb = [(0.032_786_883_f64, 1.00_f64), (0.016_129_032, 0.75)];
    let standard = [(0.032_786_883_f64, 1.00_f64), (0.016_129_032, 0.75)];
    assert_eq!(paradedb, standard);
    assert!(paradedb[0].0 > paradedb[1].0);
    assert!(paradedb[0].1 > paradedb[1].1);
}

/// Verifies a contract identity cannot be reused with different raw semantics.
/// 验证契约身份不能在不同原始评分语义下复用。
#[test]
fn descriptor_rejects_same_identity_with_wrong_raw_semantics() {
    let producer = &producers()[1];
    let mut candidates = channel(producer);
    for candidate in &mut candidates {
        candidate.score.raw_score_kind = RawScoreKind::CosineSimilarity;
    }
    validate_candidate_channel(&candidates).expect("the internally consistent lie is ordered");
    let error = validate_candidate_channel_contract(&candidates, &producer.descriptor)
        .expect_err("configured descriptor must reject the false raw semantics");
    assert!(error.diagnostic.contains("differ from the requested"));
}
