//! Compares real native prompt emission with the unmodified external business serializer and selector.
//! 将真实原生提示词输出与未经修改的外置业务序列化器及选择器对照。

use super::render_llm_input;
use serde::Deserialize;
use serde_json::Value;
use vulcan_memorymesh_core::{
    LlmInput, PostActionReferenceTurn, Role, ScopeId, SessionSourceId, SubjectId, TimelineItem,
    Turn, TurnId, TurnStatus, estimate_text_tokens,
};
use vulcan_vmm::{PostActionTimelineItem, PostActionTimelineType};

/// Holds the real external result for a single target-first integer budget boundary.
/// 保存单个目标优先整数预算边界的真实外置结果。
#[derive(Deserialize)]
struct Budget {
    /// Includes target serialization and selected reference details.
    /// 包含目标序列化与选中参考详情。
    max_tokens: u32,
    /// Preserves oldest-to-newest selected external identities.
    /// 保留从旧到新的选中外置身份。
    selected: Vec<u64>,
}

/// Holds one generated external payload with exact costs and history results.
/// 保存一条生成的外置载荷及其准确成本、历史结果。
#[derive(Deserialize)]
struct Fixture {
    /// Identifies the synthetic business case.
    /// 标识合成业务用例。
    name: String,
    /// Preserves exact bytes returned by the external serializer.
    /// 保留外置序列化器返回的准确字节。
    payload: String,
    /// Captures the external serialized-target estimator output.
    /// 捕获外置序列化目标估算结果。
    tokens: u32,
    /// Contains chronological refined history, before selection.
    /// 包含选择前按时间顺序排列的提炼历史。
    references: Vec<String>,
    /// Enumerates exact external selection results.
    /// 枚举准确外置选择结果。
    budgets: Vec<Budget>,
}

/// Deserializes only the external business dialogue, never Core ownership or lifecycle fields.
/// 仅反序列化外置业务对话，不接收 Core 归属或生命周期字段。
#[derive(Deserialize)]
struct Dialogue {
    /// Contains admitted main-user text.
    /// 包含准入后的主用户正文。
    user: String,
    /// Contains admitted ordered middle interactions.
    /// 包含准入后的有序中间交互。
    timeline: Vec<PostActionTimelineItem>,
    /// Contains final-answer text or an empty absent answer.
    /// 包含最终答复正文或代表缺失答复的空文本。
    assistant: String,
}

/// Loads generated evidence; malformed fixtures fail rather than changing expected behavior.
/// 加载生成证据；样本错误直接失败，不改变预期行为。
fn fixtures() -> Vec<Fixture> {
    serde_json::from_str(include_str!("fixtures/postaction_payload.json"))
        .expect("external fixtures")
}

/// Renders one admitted dialogue through the real native PostAction L1 projection.
/// 将一条准入后对话送入真实原生 PostAction L1 投影。
/// Ownership and monotonic timestamps are fixture-only; cleaning and real host intake are tested separately.
/// 归属与单调时间戳仅属于夹具；清洗与真实宿主准入另有独立测试。
fn render(fixture: &Fixture, max_input_tokens: u32) -> String {
    // Decode external business fields without changing their content or Unicode characters.
    // 解码外置业务字段，不改变正文或 Unicode 字符。
    let dialogue: Dialogue = serde_json::from_str(&fixture.payload).expect("external dialogue");
    // Build the canonical user/middle/final timestamp boundaries required by native projection.
    // 构造原生投影要求的规范用户／中间／最终时间戳边界。
    let mut timeline = vec![TimelineItem {
        role: Role::User,
        content: dialogue.user,
        occurred_at_ns: 1,
    }];
    for (index, item) in dialogue.timeline.into_iter().enumerate() {
        timeline.push(TimelineItem {
            role: match item.kind {
                PostActionTimelineType::User => Role::User,
                PostActionTimelineType::Assistant => Role::Assistant,
            },
            content: item.content,
            occurred_at_ns: i64::try_from(index).expect("small fixture index") + 2,
        });
    }
    if !dialogue.assistant.is_empty() {
        timeline.push(TimelineItem {
            role: Role::Assistant,
            content: dialogue.assistant,
            occurred_at_ns: 100,
        });
    }
    // Keep private fixture ownership identical for every chronological reference.
    // 为全部时间顺序参考保留一致的私有夹具归属。
    let turn = Turn {
        id: TurnId::new("vmm-turn-42").expect("turn"),
        subject_id: SubjectId::new("vmm-user-1").expect("subject"),
        scope_id: ScopeId::new("vmm-project-1").expect("scope"),
        session_source_id: SessionSourceId::new("external-payload-parity").expect("session"),
        timeline,
        previous_turn_ids: Vec::new(),
        next_turn_ids: Vec::new(),
        created_at_ns: 100,
        status: TurnStatus::Pending,
        user_input_kind: None,
        analysis_details: None,
    };
    // Map only approved numeric external Turn IDs to native stable ID strings.
    // 仅将已批准的外置数字 Turn ID 映射为原生稳定身份字符串。
    let references = fixture
        .references
        .iter()
        .enumerate()
        .map(|(index, details)| PostActionReferenceTurn {
            turn_id: TurnId::new(format!("vmm-turn-{}", index + 1)).expect("reference"),
            subject_id: turn.subject_id.clone(),
            scope_id: turn.scope_id.clone(),
            session_source_id: turn.session_source_id.clone(),
            details: details.clone(),
            created_at_ns: i64::try_from(index).expect("small history index"),
        })
        .collect();
    render_llm_input(&LlmInput::PostActionAnalysis {
        max_input_tokens,
        current_timestamp_ns: 200,
        reference_turns: references,
        target_turn: Box::new(turn),
        recent_direct_writes: Vec::new(),
    })
    .expect("native L1 prompt")
}

/// Extracts the actual raw_turn JSON bytes with the JSON decoder's consumed-byte boundary.
/// 使用 JSON 解码器消费字节边界提取实际 raw_turn JSON 字节。
/// This preserves escape overhead that a decode-and-reserialize assertion would erase.
/// 此方式保留解码后重序列化断言会抹掉的转义开销。
fn emitted_target(body: &str) -> &str {
    // The typed L1 envelope owns exactly one raw_turn key; escaped dialogue cannot match this key.
    // 强类型 L1 信封恰好拥有一个 raw_turn 键；转义后的对话无法匹配该键。
    let marker = "\"raw_turn\":";
    assert_eq!(body.matches(marker).count(), 1);
    // The stream decoder validates the subtree and reports its exact ending offset.
    // 流解码器校验子树并报告其准确结束偏移。
    let start = body.find(marker).expect("target key") + marker.len();
    let mut stream = serde_json::Deserializer::from_str(&body[start..]).into_iter::<Value>();
    stream
        .next()
        .expect("target value")
        .expect("valid target JSON");
    &body[start..start + stream.byte_offset()]
}

/// Compares semantic payloads and the costs of actual emitted bytes across every external sample.
/// 对全部外置样本比较语义载荷与实际输出字节的成本。
#[test]
fn postaction_payload_matches_external_serialization_budget() {
    // Collect all mismatches so one escaped sample cannot hide another independent boundary.
    // 收集全部不匹配，避免一个转义样本掩盖另一独立边界。
    let mut mismatches = Vec::new();
    for fixture in fixtures() {
        // Render without trimming to inspect the complete target independently of history selection.
        // 不裁剪地渲染，独立于历史选择检查完整目标。
        let body = render(&fixture, 0);
        let raw = emitted_target(&body);
        assert_eq!(
            serde_json::from_str::<Value>(raw).expect("native"),
            serde_json::from_str::<Value>(&fixture.payload).expect("external"),
            "{}",
            fixture.name
        );
        assert_eq!(
            estimate_text_tokens(&fixture.payload),
            fixture.tokens,
            "oracle estimator {}",
            fixture.name
        );
        if estimate_text_tokens(raw) != fixture.tokens {
            mismatches.push(format!(
                "{}: native {}, external {}",
                fixture.name,
                estimate_text_tokens(raw),
                fixture.tokens
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

/// Compares exact historical identities at all source-generated integer budget boundaries.
/// 在全部来源生成的整数预算边界比较准确历史身份。
#[test]
fn postaction_history_matches_external_budget_boundaries() {
    // Preserve every mismatch for an evidence-based differential report.
    // 保留每个不匹配，以形成有依据的差异报告。
    let mut mismatches = Vec::new();
    for fixture in fixtures() {
        for budget in &fixture.budgets {
            // Inspect the actual model input, not a second handwritten native selector.
            // 检查实际模型输入，而非第二个手写原生选择器。
            let body = render(&fixture, budget.max_tokens);
            let value: Value = serde_json::from_str(&body).expect("native prompt");
            let selected = value["reference_turns"]
                .as_array()
                .expect("history")
                .iter()
                .map(|row| row["turn_id"].as_str().expect("stable ID").to_owned())
                .collect::<Vec<_>>();
            let expected = budget
                .selected
                .iter()
                .map(|id| format!("vmm-turn-{id}"))
                .collect::<Vec<_>>();
            if selected != expected {
                mismatches.push(format!(
                    "{} at {}: native {selected:?}, external {expected:?}",
                    fixture.name, budget.max_tokens
                ));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
