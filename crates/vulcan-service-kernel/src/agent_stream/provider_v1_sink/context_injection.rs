//! Physical memory overlays for the authoritative V1 request projection.
//! 权威 V1 请求投影的物理记忆覆盖层。

use super::*;
use vulcan_core::types::ProviderContextInjection;
use vulcan_core::types::{ConversationMessage, ConversationPayload, ConversationSnapshot};

/// Appends declared persistent user blocks to request-local layers, never the locked baseline.
/// 将已声明的持久用户块追加到请求局部层，绝不修改锁定基线。
/// Takes the composer layers and optional admitted overlay; returns layers for physical rendering.
/// 接收组合器层及可选准入覆盖层，返回用于物理渲染的层。
pub(super) fn with_persistent_blocks(
    layers: &ProviderPromptLayers,
    injection: Option<&ProviderContextInjection>,
) -> ProviderPromptLayers {
    // Reuse the Core placement contract with only request-dynamic layers as its input authority.
    // 复用 Core 放置契约，但输入权威仅限请求动态层。
    let config = vulcan_core::types::ControllerConfig {
        prompt_layers: layers.clone(),
        context_injection: injection.cloned(),
        ..Default::default()
    };
    config.provider_prompt_layers_with_persistent_injection()
}

/// Resolves immutable prefixes against exact current V1 user positions before physical rendering.
/// 在物理渲染前，按准确当前 V1 用户位置解析不可变前缀。
/// Takes the current projection and admitted overlay; returns ordered prefixes keyed by item index.
/// 接收当前投影及准入覆盖层，返回按条目索引组织的有序前缀。
/// Rejects absent or non-user targets instead of restoring any pre-gate conversation text.
/// 拒绝缺失或非用户目标，而不恢复任何闸门前会话正文。
pub(super) fn anchored_prefixes(
    snapshot: &SessionProviderProjectionSnapshot,
    injection: Option<&ProviderContextInjection>,
) -> Result<BTreeMap<usize, String>, SessionRuntimeError> {
    // Editing and compaction can reuse positions; both the generation and optional compaction must match.
    // 编辑与压缩都会复用位置；代次和可选压缩身份必须同时匹配。
    if let Some(injection) = injection {
        // Derive the exact identity from this captured V1 snapshot, including an uncompacted lineage.
        // 从本次捕获的 V1 快照派生准确身份，包括未压缩沿革。
        let expected_epoch = vulcan_vmm::VmmContextEpoch::new(
            snapshot.segment_generation,
            snapshot
                .current_compaction_id
                .map(|id| id.value().to_string()),
        )
        .map_err(provider_projection_runtime_error)?
        .cache_epoch_id();
        if (!injection.anchored_prefixes.is_empty() || injection.cache_epoch_id.is_some())
            && injection.cache_epoch_id.as_deref() != Some(expected_epoch.as_str())
        {
            return Err(provider_projection_runtime_error(
                "V1 memory overlay does not belong to the current durable context epoch.",
            ));
        }
    }
    // Current IDs use the same sole formatter as the logical controller projection and source lookup.
    // 当前 ID 使用与逻辑控制器投影及来源查询相同的唯一格式化函数。
    let user_positions = snapshot
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, ProjectedItem::UserInput { .. }))
        .map(|(index, _)| (provider_projection_message_id(index), index))
        .collect::<BTreeMap<_, _>>();
    // Group in declared anchor order; multiple distinct anchors may share one canonical user.
    // 按已声明锚点顺序分组；多个不同锚点可以共享同一规范用户。
    let mut prefixes = BTreeMap::<usize, String>::new();
    if let Some(injection) = injection {
        for anchor in &injection.anchored_prefixes {
            // A missing target is a projection contract error, not permission to pick another message.
            // 目标缺失属于投影契约错误，不意味着可以选择其他消息。
            let index = user_positions.get(&anchor.target_message_id).ok_or_else(|| {
                provider_projection_runtime_error(format!(
                    "V1 memory anchor `{}` has no exact canonical user target `{}` in the current projection.",
                    anchor.anchor_id, anchor.target_message_id
                ))
            })?;
            // Preserve exact nonempty prefix bytes and their declared relative order.
            // 保留非空前缀的准确字节及其声明的相对顺序。
            let prefix = prefixes.entry(*index).or_default();
            if !prefix.is_empty() && !anchor.prefix.is_empty() {
                prefix.push_str("\n\n");
            }
            prefix.push_str(&anchor.prefix);
        }
    }
    Ok(prefixes)
}

/// Projects physical anchor text into the request-only history measured by the existing budget owner.
/// 将物理锚点正文投影到既有预算归属方测量的请求专属历史。
/// Takes canonical history and validated position prefixes; returns a separate history without changing the ledger.
/// 接收规范历史及已验证位置前缀，返回独立历史而不修改账本。
pub(super) fn request_history_with_anchors(
    canonical: &ConversationSnapshot,
    prefixes: &BTreeMap<usize, String>,
) -> Result<Vec<ConversationMessage>, SessionRuntimeError> {
    // The physical request and its budget snapshot must contain identical prefix bytes.
    // 物理请求及其预算快照必须包含完全相同的前缀字节。
    let mut remaining = prefixes
        .iter()
        .filter(|(_, prefix)| !prefix.is_empty())
        .map(|(index, prefix)| (provider_projection_message_id(*index), prefix))
        .collect::<BTreeMap<_, _>>();
    // Only this request-local copy receives rendering; continuation checkpoints keep canonical text.
    // 仅此请求局部副本接收渲染；续接检查点保留规范正文。
    let mut history = canonical.messages.clone();
    for message in &mut history {
        if let Some(prefix) = remaining.remove(&message.id) {
            match &mut message.payload {
                ConversationPayload::UserText { text, .. } => {
                    *text = format!("{prefix}\n\n{text}");
                }
                _ => {
                    return Err(provider_projection_runtime_error(
                        "V1 memory anchor budget projection requires the same canonical user payload.",
                    ));
                }
            }
        }
    }
    if !remaining.is_empty() {
        return Err(provider_projection_runtime_error(
            "V1 memory anchor budget projection is missing a canonical user message.",
        ));
    }
    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_core::types::ProviderContextAnchor;

    /// Builds an isolated projection with a child return followed by one real user input.
    /// 构建子任务返回后接一个真实用户输入的隔离投影。
    /// Returns the source-free fixture used only to test item-position targeting without storage IO.
    /// 返回仅用于无存储 IO 条目位置定位测试的无来源夹具。
    fn projection() -> SessionProviderProjectionSnapshot {
        SessionProviderProjectionSnapshot {
            current_compaction_id: None,
            preparing_user_prompt_media_seqs: Vec::new(),
            segment_generation: 1,
            document_revision: 1,
            retained_from_seq: Seq::try_new(1).expect("retained sequence"),
            through_seq: Seq::try_new(2).expect("last sequence"),
            items: vec![
                ProjectedItem::SubagentReturnText {
                    text: String::from("not a user input"),
                },
                ProjectedItem::UserInput {
                    text: String::from("actual user input"),
                    resource_ids: Vec::new(),
                },
            ],
            sourced_items: Vec::new(),
            project_directory: PathBuf::new(),
            provider_usage_budget_anchor: None,
            initialized_locked_baseline: None,
        }
    }

    /// Checks exact targets, anchor ordering and explicit rejection without matching by prompt text.
    /// 检查准确目标、锚点顺序及显式拒绝，不按提示词文本匹配。
    /// Uses no external input and returns only after all projection invariants hold.
    /// 不接收外部输入，仅在全部投影约束成立后返回。
    #[test]
    fn anchors_require_exact_current_user_input_and_preserve_order() {
        // The first user-role transport is a child return and must not impersonate a real user input.
        // 第一条用户角色传输是子任务返回，不能冒充真实用户输入。
        let snapshot = projection();
        // Multiple anchors use their declared order, not lexical anchor IDs.
        // 多个锚点使用声明顺序，而非锚点 ID 字典序。
        let mut injection = ProviderContextInjection {
            cache_epoch_id: Some(
                vulcan_vmm::VmmContextEpoch::new(1, None)
                    .expect("context")
                    .cache_epoch_id(),
            ),
            anchored_prefixes: vec![
                ProviderContextAnchor {
                    anchor_id: String::from("z"),
                    target_message_id: provider_projection_message_id(1),
                    prefix: String::from("first"),
                },
                ProviderContextAnchor {
                    anchor_id: String::from("a"),
                    target_message_id: provider_projection_message_id(1),
                    prefix: String::from("second"),
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            anchored_prefixes(&snapshot, Some(&injection)).expect("exact target"),
            BTreeMap::from([(1, String::from("first\n\nsecond"))])
        );
        for target in [
            provider_projection_message_id(0),
            provider_projection_message_id(2),
        ] {
            injection.anchored_prefixes[0].target_message_id = target;
            assert!(anchored_prefixes(&snapshot, Some(&injection)).is_err());
        }
        assert!(
            anchored_prefixes(&snapshot, None)
                .expect("no overlay")
                .is_empty()
        );
    }

    /// Checks that an empty dynamic composer does not import any controller system baseline.
    /// 检查空动态组合器不会引入任何控制器系统基线。
    /// Uses a persistent memory block and returns after exact placement and input immutability assertions.
    /// 使用持久记忆块，在准确放置及输入不变断言通过后返回。
    #[test]
    fn persistent_blocks_use_only_declared_dynamic_layers() {
        // The input contains no frozen baseline and remains unchanged after request-local composition.
        // 输入不包含冻结基线，请求局部组合后保持不变。
        let layers = ProviderPromptLayers::default();
        // The existing Core contract places persistent context in hidden user content.
        // 既有 Core 契约将持久上下文放在隐藏用户内容中。
        let injection = ProviderContextInjection {
            persistent_blocks: vec![String::from("profile")],
            ..Default::default()
        };
        assert_eq!(
            with_persistent_blocks(&layers, Some(&injection)),
            ProviderPromptLayers {
                runtime_context: String::from("profile"),
                ..Default::default()
            }
        );
        assert_eq!(layers, ProviderPromptLayers::default());
        assert_eq!(with_persistent_blocks(&layers, None), layers);
    }

    /// Rejects stale memory even when compaction reuses the same canonical position identifier.
    /// 即使压缩复用相同规范位置标识，也拒绝过期记忆。
    /// Uses an exact durable compaction identity; no storage or Provider calls are made.
    /// 使用准确持久压缩身份，不执行存储或 Provider 调用。
    #[test]
    fn compaction_epoch_is_required_before_reusing_user_positions() {
        // The current projection still contains position two, but its durable epoch has changed.
        // 当前投影仍包含位置二，但其持久纪元已经变化。
        let mut snapshot = projection();
        snapshot.current_compaction_id =
            Some(vulcan_session_core::v1::ids::CompactionId::try_new(9).expect("compaction"));
        // This old anchor would incorrectly pass position-only validation.
        // 此旧锚点会错误通过仅按位置进行的校验。
        let mut injection = ProviderContextInjection {
            cache_epoch_id: Some(
                vulcan_vmm::VmmContextEpoch::new(1, Some("8".to_owned()))
                    .expect("context")
                    .cache_epoch_id(),
            ),
            anchored_prefixes: vec![ProviderContextAnchor {
                anchor_id: String::from("old"),
                target_message_id: provider_projection_message_id(1),
                prefix: String::from("memory"),
            }],
            ..Default::default()
        };
        assert!(anchored_prefixes(&snapshot, Some(&injection)).is_err());
        injection.cache_epoch_id = Some(
            vulcan_vmm::VmmContextEpoch::new(1, Some("9".to_owned()))
                .expect("context")
                .cache_epoch_id(),
        );
        assert_eq!(
            anchored_prefixes(&snapshot, Some(&injection))
                .expect("current epoch")
                .len(),
            1
        );
        // Editing can change the generation without removing the current compaction identity.
        // 编辑可以改变代次而不移除当前压缩身份。
        snapshot.segment_generation = 2;
        assert!(anchored_prefixes(&snapshot, Some(&injection)).is_err());
        snapshot.current_compaction_id = None;
        assert!(anchored_prefixes(&snapshot, Some(&injection)).is_err());
        injection.cache_epoch_id = Some(
            vulcan_vmm::VmmContextEpoch::new(2, None)
                .expect("rewound")
                .cache_epoch_id(),
        );
        assert_eq!(
            anchored_prefixes(&snapshot, Some(&injection))
                .expect("rewound epoch")
                .len(),
            1
        );
        snapshot.segment_generation = 3;
        assert!(anchored_prefixes(&snapshot, Some(&injection)).is_err());
        injection.anchored_prefixes.clear();
        assert!(anchored_prefixes(&snapshot, Some(&injection)).is_err());
    }
}
