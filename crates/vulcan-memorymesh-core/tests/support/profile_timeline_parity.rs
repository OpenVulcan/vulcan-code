//! Compares public Core profile queries with freshly generated external business output.
//! 将公共 Core 画像查询与真实生成的外置业务输出对照。

use super::*;
use serde::Deserialize;

/// Describes one exact synthetic active fact supplied to both runtimes.
/// 描述交给两个运行时的同一条准确合成活跃事实。
#[derive(Deserialize)]
struct OracleNode {
    /// Preserves the complete fact text without test-side normalization.
    /// 保留完整事实正文，不在测试端规范化。
    content: String,
    /// Uses an empty string only for the domain's explicit absent effective-date case.
    /// 仅在领域明确缺失生效日期时使用空字符串。
    anchor: String,
    /// Provides the exact creation instant and ordering tie breaker.
    /// 提供准确创建时刻及平局排序依据。
    created: String,
    /// Encodes the external P0/P1/P2 priority.
    /// 编码外置 P0/P1/P2 优先级。
    priority: u32,
    /// Encodes the shared lifetime label L0/L1/L2/L3.
    /// 编码共享生命周期标签 L0/L1/L2/L3。
    level: u32,
    /// Preserves the refresh count from the external input.
    /// 保留外置输入的刷新次数。
    weight: u32,
}

/// Holds one external output together with the exact inputs and historical timezone.
/// 保存外置输出及其准确输入和历史时区。
#[derive(Deserialize)]
struct OracleTimeline {
    /// Identifies the historical calendar used during real external execution.
    /// 标识真实外置执行使用的历史日历。
    timezone: String,
    /// Preserves the deliberately unordered source facts.
    /// 保留刻意未排序的来源事实。
    nodes: Vec<OracleNode>,
    /// Contains the production-generated body, compared byte for byte.
    /// 包含生产生成的正文，用于逐字节比较。
    rendered: String,
}

/// Converts an explicit RFC3339 source instant to exact signed nanoseconds, rejecting invalid fixtures.
/// 将明确的 RFC3339 来源时刻转换为准确有符号纳秒，拒绝无效样本。
fn timestamp(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .expect("valid oracle RFC3339 instant")
        .timestamp_nanos_opt()
        .expect("oracle timestamp fits nanoseconds")
}

/// Verifies the actual public query renderer against all external date, label and ordering bytes.
/// 根据全部外置日期、标签和排序字节验证实际公共查询渲染器。
#[test]
fn public_profile_timeline_matches_external_oracle() {
    // fixtures were produced by the real Go business function, not a Rust reimplementation.
    // fixtures 由真实 Go 业务函数产生，并非 Rust 重实现。
    let fixtures: Vec<OracleTimeline> =
        serde_json::from_str(include_str!("../fixtures/profile_timeline.json"))
            .expect("external profile timeline fixture");
    assert_eq!(fixtures.len(), 6);
    // core uses the public command/effect chain and deterministic fact delivery only.
    // core 使用公共命令和效果链，仅事实交付是确定性的。
    let core = MemoryMeshCore::new().expect("Core");
    // identities define the authorized subject section used by the public query.
    // identities 定义公共查询使用的已授权主体分区。
    let (subject_id, scope_access, _) = identities();
    // host creates a valid domain template through the normal reference storage mutation.
    // host 通过正常参考存储变更创建合法领域模板。
    let mut host = DeterministicHost::new();
    seed_profile_node(&mut host, &subject_id, &scope_access.write_scope, 1);
    // template retains source and scope invariants while the oracle supplies business fields.
    // template 保留来源和作用域不变量，由外置样本提供业务字段。
    let template = &host.storage.dataset().profile_nodes[0];
    for fixture in fixtures {
        // nodes map only the documented protocol representation differences.
        // nodes 仅映射已记录的协议表示差异。
        let mut nodes = fixture
            .nodes
            .iter()
            .enumerate()
            .map(|(index, source)| {
                // node is an authorized fact whose complete business data comes from the oracle input.
                // node 是已授权事实，其完整业务数据来自外置基准输入。
                let mut node = template.clone();
                node.id =
                    ProfileNodeId::new(format!("oracle-profile-{index}")).expect("profile ID");
                node.content = source.content.clone();
                node.priority = match source.priority {
                    0 => 100,
                    1 => 50,
                    2 => 20,
                    _ => panic!("unknown external priority"),
                };
                node.profile_level = match source.level {
                    0 => ProfileLevel::Transient,
                    1 => ProfileLevel::Situational,
                    2 => ProfileLevel::Stable,
                    3 => ProfileLevel::Persistent,
                    _ => panic!("unknown external lifetime"),
                };
                node.refresh_weight = source.weight;
                node.created_at_ns = timestamp(&source.created);
                node.effective_date_ns = if source.anchor.is_empty() {
                    None
                } else {
                    Some(timestamp(&source.anchor))
                };
                node.updated_at_ns = node.effective_date_ns.unwrap_or(node.created_at_ns);
                node
            })
            .collect::<Vec<_>>();
        for mode in [ProfileViewMode::Full, ProfileViewMode::Split] {
            for reversed in [false, true] {
                if reversed {
                    nodes.reverse();
                }
                // bundle is returned by the real command/effect renderer with no text normalization.
                // bundle 由真实命令和效果渲染器返回，不进行文本规范化。
                let bundle = query_profile_facts(
                    &core,
                    subject_id.clone(),
                    scope_access.clone(),
                    ProfileView {
                        mode,
                        sections: vec![subject_profile_section(&scope_access)],
                        include_explanations: false,
                        display_timezone: fixture.timezone.clone(),
                    },
                    nodes.clone(),
                )
                .expect("oracle profile query");
                assert_eq!(
                    bundle.sections[0].rendered, fixture.rendered,
                    "timezone={}, reversed={reversed}, mode={mode:?}",
                    fixture.timezone
                );
                if mode == ProfileViewMode::Split || fixture.nodes.is_empty() {
                    assert!(bundle.rendered.is_empty());
                } else {
                    assert!(bundle.rendered.contains(&fixture.rendered));
                }
                if reversed {
                    nodes.reverse();
                }
            }
        }
    }
}
