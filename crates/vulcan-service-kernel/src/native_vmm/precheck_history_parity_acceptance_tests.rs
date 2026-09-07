//! Real-host PreCheck history compared with source-generated external mixed-state expectations.
//! 将真实宿主 PreCheck 历史与来源生成的外置混合状态预期对照。

use super::*;
use serde::Deserialize;

/// Carries the exact external positive-budget selection result.
/// 携带准确外置正数预算选择结果。
#[derive(Deserialize)]
struct Budget {
    /// Is the configured historical token ceiling, excluding the current user prompt.
    /// 为配置的历史令牌上限，不包含当前用户提示词。
    max_tokens: u32,
    /// Identifies source ordinals in chronological order.
    /// 按时间顺序标识来源序号。
    selected: Vec<u64>,
}

/// Loads original synthetic inputs and production external context outputs.
/// 加载合成原始输入及生产外置上下文输出。
#[derive(Deserialize)]
pub(super) struct Fixture {
    /// Supplies original inputs before either implementation's cleaner runs.
    /// 提供两个实现各自运行清洗器之前的原始输入。
    pub(super) sources: Vec<String>,
    /// Records the independently generated canonical final answer.
    /// 记录独立生成的规范最终回答。
    assistant: String,
    /// Preserves the complete external turn ID, content type and content objects.
    /// 保留完整外置回合身份、内容类型与正文对象。
    turns: Vec<serde_json::Value>,
    /// Stores actual external results at each integer budget boundary.
    /// 保存每个整数预算边界的实际外置结果。
    budgets: Vec<Budget>,
}

/// Loads immutable generated evidence and rejects invalid or incomplete fixture data.
/// 加载不可变生成证据，拒绝无效或不完整样本数据。
pub(super) fn fixture() -> Fixture {
    // The source corpus is fixed to two completed and two pending turns.
    // 来源样本固定为两个已完成及两个待处理回合。
    let fixture: Fixture = serde_json::from_str(include_str!("fixtures/precheck_history.json"))
        .expect("external precheck fixture");
    assert_eq!(fixture.sources.len(), 4);
    assert_eq!(fixture.turns.len(), 4);
    assert_eq!(fixture.budgets.len(), 10);
    fixture
}

/// Verifies actual L1 history against external bytes and selections through formal policy saves.
/// 经正式策略保存，将实际 L1 历史与外置字节和选择结果对照。
/// Takes real client/port/service ownership, original policy and independently validated native IDs; returns added call count.
/// 接收真实客户端／端口／服务归属、原策略及已独立验证的原生身份；返回新增调用次数。
pub(super) async fn verify(
    client: &NativeVmmClient,
    port: &CapturingAnalysisPort,
    service: &crate::state::SharedServiceBridgeState,
    project_id: u64,
    original_policy: &VmmNativeRecallPolicySummary,
    full: &[serde_json::Value],
) -> usize {
    // Expected source ordinals map only to real accepted IDs already verified by the parent fixture.
    // 预期来源序号仅映射到父夹具已验证的真实接纳身份。
    let fixture = fixture();
    assert_eq!(fixture.assistant, ASSISTANT);
    assert_eq!(full.len(), fixture.turns.len());
    // Preserve external content bytes, including JSON escapes inside RAW_TURN strings.
    // 保留外置正文字节，包括 RAW_TURN 字符串内部的 JSON 转义。
    let expected = fixture
        .turns
        .iter()
        .enumerate()
        .map(|(index, row)| {
            assert_eq!(
                row["turn_id"].as_u64(),
                Some(u64::try_from(index + 1).expect("ordinal"))
            );
            let mut mapped = row.clone();
            mapped["turn_id"] = full[index]["turn_id"].clone();
            mapped
        })
        .collect::<Vec<_>>();
    // Collect both shape and selection failures without allowing the first mismatch to hide later boundaries.
    // 同时收集形态与选择失败，避免首个不匹配遮蔽后续边界。
    let mut mismatches = Vec::new();
    for (index, (actual, external)) in full.iter().zip(&expected).enumerate() {
        if actual != external {
            mismatches.push(format!(
                "full history ordinal {}: native {actual}, external {external}",
                index + 1
            ));
        }
    }
    // Change only the historical budget; caller ownership and the four-turn window remain fixed.
    // 仅修改历史预算；调用方归属及四回合窗口保持固定。
    let mut policy = original_policy.clone();
    for budget in &fixture.budgets {
        policy.precheck_max_input_tokens = budget.max_tokens;
        save_global(service, &policy).await;
        // Production performs actual admission, L1 capture and empty physical retrieval on every iteration.
        // 每次迭代均由生产执行实际准入、L1 捕获及空物理检索。
        let actual = check_history(client, port, project_id, "precheck-host-b").await;
        let selected = budget
            .selected
            .iter()
            .map(|ordinal| expected[usize::try_from(*ordinal).expect("source ordinal") - 1].clone())
            .collect::<Vec<_>>();
        if actual != selected {
            mismatches.push(format!(
                "budget {}: native {actual:?}, external {selected:?}",
                budget.max_tokens
            ));
        }
    }
    save_global(service, original_policy).await;
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    fixture.budgets.len()
}
