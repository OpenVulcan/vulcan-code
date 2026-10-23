//! Physical memory overlays for the authoritative V1 request projection.
//! 权威 V1 请求投影的物理记忆覆盖层。

use super::*;
use vulcan_core::types::ProviderContextInjection;
use vulcan_core::types::{ConversationMessage, ConversationPayload, ConversationSnapshot};
use vulcan_session_core::v1::dynamic_context::{
    DYNAMIC_CONTEXT_DEVELOPER_KEY, DYNAMIC_CONTEXT_PERSISTENT_KEY, DYNAMIC_CONTEXT_RUNTIME_KEY,
    DYNAMIC_CONTEXT_SYSTEM_KEY, DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY, DynamicContextInput,
    DynamicContextOccurrence,
};
use vulcan_session_core::v1::event::SystemContextScope;

/// Stable producer for the current model identity after a route switch.
/// 路由切换后当前模型身份的稳定生产者。
pub(super) const MODEL_SELECTION_CONTEXT_KEY: &str = "dynamic.model_selection";

/// Stable producer describing the session permission mode the tool gate enforces.
/// 描述工具闸门所执行会话权限模式的稳定生产者。
pub(super) const PERMISSION_MODE_CONTEXT_KEY: &str = "dynamic.permission_mode";

/// Stable producer describing provider tools after the user changed them mid-session.
/// 描述用户在会话中途修改后的供应商工具的稳定生产者。
pub(super) const PROVIDER_TOOLS_CONTEXT_KEY: &str = "dynamic.provider_tools";

/// Render the interactive answer mode prompt with the unique Core component limits.
/// 使用唯一的 Core 组件上限渲染互动回答模式提示词。
/// No input; returns one trimmed producer body without independently defined limits.
/// 无输入；返回已去除首尾空白且没有独立限额定义的一份生产者正文。
pub(super) fn answer_mode_interactive_prompt() -> String {
    use vulcan_session_core::v1::answer_components::{
        ANSWER_CHART_MAX_SERIES, ANSWER_CODE_MAX_EXCERPTS, ANSWER_CODE_MAX_LINES,
        ANSWER_COMPONENT_MAX_ROWS,
    };
    vulcan_runtime_assets::ANSWER_MODE_INTERACTIVE_MD
        .trim()
        .replace("{{max_rows}}", &ANSWER_COMPONENT_MAX_ROWS.to_string())
        .replace("{{max_series}}", &ANSWER_CHART_MAX_SERIES.to_string())
        .replace("{{code_max_lines}}", &ANSWER_CODE_MAX_LINES.to_string())
        .replace(
            "{{code_max_excerpts}}",
            &ANSWER_CODE_MAX_EXCERPTS.to_string(),
        )
}

/// Builds an append-only identity notice from `document` history and the frozen `request`.
/// 根据 `document` 历史及冻结的 `request` 构建只追加的身份通知。
/// Returns no initial notice; once a switch is observed, the current identity remains durable and idempotent.
/// 初次不产生通知；观察到切换后，当前身份保持持久化和幂等。
fn model_selection_input(
    document: &vulcan_session_core::v1::document::SessionDocument,
    request: &vulcan_session_core::v1::event::ApiRequestSnapshot,
) -> DynamicContextInput {
    use vulcan_session_core::v1::event::{ApiPurposeKind, EventBody};
    // Only main-generation requests establish the conversational identity; summaries use separate routes.
    // 只有主生成请求建立对话身份；摘要使用独立路由。
    let previous = document
        .events()
        .iter()
        .rev()
        .find_map(|event| match &event.event {
            EventBody::ApiStarted(started) if started.purpose == ApiPurposeKind::MainGeneration => {
                Some(&started.request)
            }
            _ => None,
        });
    // Persisted identity also covers a failed preparation followed by a switch back before dispatch.
    // 持久身份也覆盖准备失败后、派发前又切回原模型的情况。
    let changed = previous.is_some_and(|previous| {
        previous.provider != request.provider
            || previous.model != request.model
            || previous
                .profile
                .as_ref()
                .zip(request.profile.as_ref())
                .is_some_and(|(old, new)| old != new)
    });
    let retained = document
        .metadata
        .dynamic_context
        .contains_key(MODEL_SELECTION_CONTEXT_KEY);
    // Stable text depends only on target identity, so retries and effort changes cannot append duplicates.
    // 稳定正文只依赖目标身份，使重试及思考强度变化不会重复追加。
    // Protocol is optional in persisted request snapshots; report it only when frozen facts supply it.
    // 持久请求快照中的协议是可选值；仅在冻结事实提供它时报告。
    let protocol = request
        .profile
        .as_ref()
        .map(|profile| format!(", protocol {profile:?}"))
        .unwrap_or_default();
    let body = (changed || retained).then(|| format!(
        "Model selection update: this session now continues with provider {:?}, model {:?}{protocol}. Earlier assistant messages retain their original model provenance and do not establish your current identity. Provider-bound encrypted reasoning and signatures cannot be transferred between routes; available historical text is retained. Continue the existing task without acknowledging this context update.",
        request.provider, request.model,
    ));
    DynamicContextInput::new(
        MODEL_SELECTION_CONTEXT_KEY,
        body,
        SystemContextScope::Session,
        None,
    )
}

/// Builds the permission mode notice from the session's own persisted mode.
/// 根据会话自身持久化的模式构建权限模式通知。
/// The default mode needs no notice until another mode was observed; child sessions follow their
/// root, whose mode the gate enforces, so they receive no notice of their own; tool-free dedicated
/// sessions never receive one.
/// 默认模式在观察到其他模式之前无需通知；子会话跟随根会话由闸门执行，因此不单独收到通知；
/// 无工具专职会话从不收到通知。
fn permission_mode_input(
    document: &vulcan_session_core::v1::document::SessionDocument,
) -> DynamicContextInput {
    use vulcan_session_core::v1::config::PermissionMode;
    let mode = document.metadata.mode.access.permission;
    let retained = document
        .metadata
        .dynamic_context
        .contains_key(PERMISSION_MODE_CONTEXT_KEY);
    let notice = match mode {
        PermissionMode::ReadOnly => {
            "Permission mode: read-only. Reading files and running known read-only commands proceed; file writes, state-changing commands, and extension tools that may change state are denied before they run, and the denial explains why. When a task needs a change, say what you would change so the user can switch modes."
        }
        PermissionMode::WorkspaceWrite => {
            "Permission mode: workspace write. Edits inside the workspace and known read-only commands run directly; other actions need the user's approval. Attempt the task normally and let the host decide each call."
        }
        PermissionMode::AutoApprove => {
            "Permission mode: auto-approve. Edits inside the workspace and known read-only commands run directly; other actions are checked by an automatic permission judge and reviewer, and may still need the user's approval. Attempt the task normally and let the host decide each call."
        }
        PermissionMode::FullAccess => {
            "Permission mode: full access. Tool calls run without permission checks. Still avoid destructive actions the user did not ask for."
        }
    };
    // A dedicated session without general tools has nothing a permission mode could govern.
    // 未开放通用工具的专职会话没有可受权限模式约束的操作。
    let tool_free = document.dedicated_profile().is_some_and(|profile| {
        profile.tool_access == vulcan_session_core::v1::DedicatedToolAccess::None
    });
    let body = (document.metadata.child_execution_profile.is_none()
        && !tool_free
        && (mode != PermissionMode::default() || retained))
        .then(|| {
            format!(
                "{notice} Continue the existing task without acknowledging this context update."
            )
        });
    DynamicContextInput::new(
        PERMISSION_MODE_CONTEXT_KEY,
        body,
        SystemContextScope::Session,
        None,
    )
}

/// Builds the provider-tool notice from the session's own persisted tool selection.
/// 根据会话自身持久化的工具选择构建供应商工具通知。
/// Nothing is said until the user changes tools after the first request; from then on the notice
/// states the complete current configuration, so several changes combine into one stable text.
/// 首次请求后用户修改工具之前不发送任何内容；此后通知陈述完整当前配置，多次修改合并为一份稳定正文。
fn provider_tools_input(
    document: &vulcan_session_core::v1::document::SessionDocument,
) -> DynamicContextInput {
    use vulcan_session_core::v1::config::ProviderExtensionModeKind;
    let tools = &document.metadata.config.selection.provider_tools;
    let body = tools.changed_mid_session.then(|| {
        let mut lines = Vec::new();
        let direct = tools
            .enabled_capabilities
            .iter()
            .map(|capability| {
                tools
                    .selections
                    .iter()
                    .find(|selection| &selection.capability == capability)
                    .map(|selection| format!("{capability} (`{}`)", selection.target.target_id))
                    .unwrap_or_else(|| capability.clone())
            })
            .collect::<Vec<_>>();
        lines.push(if direct.is_empty() {
            String::from("- Search, extraction and media-understanding tools: none enabled.")
        } else {
            format!(
                "- Search, extraction and media-understanding tools enabled: {}.",
                direct.join(", ")
            )
        });
        for capability in vulcan_provider_adapters::ProviderToolCapabilityKind::MODEL_EXTENSIONS {
            let capability = serde_json::to_value(capability)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default();
            let mode = tools
                .extension_modes
                .iter()
                .find(|entry| entry.capability == capability);
            let line = match mode {
                Some(entry) if entry.mode == ProviderExtensionModeKind::Multi => {
                    let targets = tools
                        .extension_target_sets
                        .iter()
                        .find(|set| set.capability == capability)
                        .map(|set| {
                            set.targets
                                .iter()
                                .map(|target| format!("`{}`", target.target_id))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if targets.is_empty() {
                        format!("- {capability}: unavailable.")
                    } else {
                        format!(
                            "- {capability}: multi-model mode choosing among {}; call get_model_extension_parameters first, then pass `model` and `args`.",
                            targets.join(", ")
                        )
                    }
                }
                Some(entry) => match &entry.single_target {
                    Some(target) => format!(
                        "- {capability}: fixed model `{}`; call the tool directly with the arguments in its schema, without a model argument.",
                        target.target_id
                    ),
                    None => format!("- {capability}: unavailable."),
                },
                None => format!("- {capability}: unavailable."),
            };
            lines.push(line);
        }
        format!(
            "Provider tool update: the user changed this session's provider tools after the conversation started. Earlier tool calls in this conversation may have used tools, models or arguments that no longer apply; follow only the current tool definitions. A tool shown as unavailable is not registered. Current configuration:\n{}\nContinue the existing task without acknowledging this context update.",
            lines.join("\n")
        )
    });
    DynamicContextInput::new(
        PROVIDER_TOOLS_CONTEXT_KEY,
        body,
        SystemContextScope::Session,
        None,
    )
}

/// Publishes business producer changes; compaction has no authority to withdraw or replace them.
/// 发布业务生产者变化；压缩没有撤下或替换这些生产者的权限。
/// Uses the exact handle and request facts; returns only after the durable mutation succeeds.
/// 使用精确句柄及请求事实；仅在可靠修改成功后返回。
pub(super) fn synchronize_request_dynamic_context(
    handle: &SessionHandle,
    facts: &ProviderV1RequestFacts,
    candidate: &ProviderRequestPreparationCandidate,
) -> Result<(), SessionRuntimeError> {
    if facts.purpose == vulcan_session_core::v1::event::ApiPurposeKind::Compaction {
        return Ok(());
    }
    // The ordinary path keeps the complete producer inventory and its original durable admission.
    // 普通路径保留完整生产者清单及原有可靠准入。
    let mut inputs = dynamic_context_inputs(
        &candidate.dynamic_prompt_layers,
        candidate.controller_config.context_injection.as_ref(),
    )?;
    handle.mutate_durable_if_changed(
        SessionMutationAdmission::ExistingTurn,
        SessionFlushReason::MetadataChanged,
        move |document| {
            // Validate and choose the persisted baseline under the same mutation lock as context publication.
            // 在上下文发布的同一修改锁内校验并选择持久基线。
            inputs.push(host_guidance::upgrade_input(
                document,
                &candidate.locked_baseline,
            )?);
            inputs.push(model_selection_input(document, &facts.request));
            inputs.push(permission_mode_input(document));
            inputs.push(provider_tools_input(document));
            // Freeze the answer mode under the same lock as its request-context publication.
            // 在请求上下文发布的同一把锁内冻结回答模式。
            document
                .synchronize_context_with_answer_mode(
                    inputs,
                    &answer_mode_interactive_prompt(),
                    vulcan_runtime_assets::ANSWER_MODE_STANDARD_MD.trim(),
                    facts.prepared_at_ms,
                )
                .map(|result| match result {
                    vulcan_session_core::v1::document::IdempotentMutation::Committed(seqs) => {
                        SessionMutationChange::Changed(seqs)
                    }
                    vulcan_session_core::v1::document::IdempotentMutation::Idempotent(seqs) => {
                        SessionMutationChange::Unchanged(seqs)
                    }
                })
        },
    )?;
    Ok(())
}

/// Builds request-local User controls for a summary without adding events to the business document.
/// 为摘要构建请求局部 User 控制消息，不向业务文档添加事件。
/// Exact typed layers and the prepared final prompt are rendered once; returns an error for invalid envelopes.
/// 精确类型化提示层和已准备最终提示只渲染一次；信封无效时返回错误。
pub(super) fn compaction_request_messages(
    facts: &ProviderV1RequestFacts,
    candidate: &ProviderRequestPreparationCandidate,
) -> Result<Vec<ConversationMessage>, SessionRuntimeError> {
    if facts.purpose != vulcan_session_core::v1::event::ApiPurposeKind::Compaction {
        return Ok(Vec::new());
    }
    if candidate.request_snapshot.prompt.trim().is_empty() {
        return Err(provider_projection_runtime_error(
            "Compaction request has no final summary instruction.",
        ));
    }
    // Fixed source names come from the typed composer fields, never from parsing user content.
    // 固定来源名称来自类型化组合器字段，绝不通过解析用户内容推测。
    let layers = &candidate.dynamic_prompt_layers;
    // Ordered controls are appended after authoritative history and included in request budgeting.
    // 有序控制消息追加在权威历史之后，并计入请求预算。
    let mut messages = Vec::new();
    for (source, body) in [
        ("system", layers.system.as_str()),
        ("developer", layers.developer.as_str()),
        ("tool_guidance", layers.tool_guidance.as_str()),
        ("runtime_context", layers.runtime_context.as_str()),
        ("instruction", candidate.request_snapshot.prompt.as_str()),
    ] {
        if body.trim().is_empty() {
            continue;
        }
        // Bind each local control to this exact physical request and fixed composer field.
        // 将每条局部控制绑定到本次精确物理请求及固定组合器字段。
        let event_id = format!("compaction-request:{}:{source}", facts.provider_call_id);
        // This typed envelope is request-local; no body text can create durable collaboration authority.
        // 此类型化信封仅属于请求；正文不能创建持久协作权限。
        let envelope = vulcan_ai_contract::HostMessageEnvelope {
            event_id: event_id.clone(),
            kind: vulcan_ai_contract::HostMessageKind::ContextUpdate,
            source: format!("context_compaction:{source}"),
            target: String::from("model"),
            task_id: None,
            question_id: None,
            scope: String::from("compaction_request"),
            body: body.to_owned(),
        };
        messages.push(ConversationMessage {
            id: event_id,
            payload: ConversationPayload::UserText {
                text: envelope
                    .render()
                    .map_err(provider_projection_runtime_error)?,
                attachments: Vec::new(),
            },
        });
    }
    Ok(messages)
}

/// Builds the stable prompt inputs, independent transient blocks, and typed expiry facts.
/// 构建稳定提示层输入、独立临时块及类型化过期事实。
///
/// Empty layers are represented as `None`, so the domain method can append one explicit
/// invalidation fact when a producer disappears. The request bridge owns no envelope rendering;
/// projection later wraps the raw event body exactly once.
/// 空层使用 `None` 表示，使领域方法能在生产者消失时追加一条明确失效事实。请求桥接不负责信封渲染，
/// 后续投影只对原始事件正文套壳一次。
/// Persistent values use Session scope; each typed transient block uses one Turn-scoped producer.
/// 持久值使用 Session 作用域；每个类型化临时块使用一个 Turn 作用域生产者。
pub(super) fn dynamic_context_inputs(
    layers: &ProviderPromptLayers,
    injection: Option<&ProviderContextInjection>,
) -> Result<Vec<DynamicContextInput>, SessionRuntimeError> {
    let persistent = injection.map(|value| {
        value
            .persistent_blocks
            .iter()
            .filter_map(|block| normalized_raw_content(block))
            .collect::<Vec<_>>()
            .join("\n\n")
    });
    let mut inputs = [
        (
            DYNAMIC_CONTEXT_SYSTEM_KEY,
            normalized_raw_content(&layers.system),
            SystemContextScope::Session,
            None,
        ),
        (
            DYNAMIC_CONTEXT_DEVELOPER_KEY,
            normalized_raw_content(&layers.developer),
            SystemContextScope::Session,
            None,
        ),
        (
            DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY,
            normalized_raw_content(&layers.tool_guidance),
            SystemContextScope::Session,
            None,
        ),
        (
            DYNAMIC_CONTEXT_RUNTIME_KEY,
            normalized_raw_content(&layers.runtime_context),
            SystemContextScope::Session,
            None,
        ),
        (
            DYNAMIC_CONTEXT_PERSISTENT_KEY,
            persistent.filter(|value| !value.is_empty()),
            SystemContextScope::Session,
            None,
        ),
    ]
    .into_iter()
    .map(|(producer_key, raw_content, scope, ttl_turns)| {
        DynamicContextInput::new(producer_key, raw_content, scope, ttl_turns)
    })
    .collect::<Vec<_>>();
    if let Some(injection) = injection {
        for block in injection.host_blocks.iter().filter(|block| block.transient) {
            if block.scope != "turn" {
                return Err(provider_projection_runtime_error(format!(
                    "transient context block `{}` has scope `{}` instead of `turn`",
                    block.source_id, block.scope
                )));
            }
            let tag = block.tag.clone().ok_or_else(|| {
                provider_projection_runtime_error(format!(
                    "transient context block `{}` has no typed tag",
                    block.source_id
                ))
            })?;
            let content_hash = block.content_hash.clone().ok_or_else(|| {
                provider_projection_runtime_error(format!(
                    "transient context block `{}` has no typed content hash",
                    block.source_id
                ))
            })?;
            let turn_id = block.turn_id.clone().ok_or_else(|| {
                provider_projection_runtime_error(format!(
                    "transient context block `{}` has no typed turn identity",
                    block.source_id
                ))
            })?;
            let raw_content = normalized_raw_content(&block.body).ok_or_else(|| {
                provider_projection_runtime_error(format!(
                    "transient context block `{}` has no raw body",
                    block.source_id
                ))
            })?;
            let occurrence = DynamicContextOccurrence::new(
                block.source_id.clone(),
                block.occurrence_id.clone(),
                tag,
                content_hash,
                turn_id,
            );
            inputs.push(
                DynamicContextInput::new(
                    block.source_id.clone(),
                    Some(raw_content),
                    SystemContextScope::Turn,
                    Some(1),
                )
                .with_occurrence(occurrence),
            );
        }
        for event in &injection.expired_events {
            if event.scope != "turn" {
                return Err(provider_projection_runtime_error(format!(
                    "expired context event `{}` has scope `{}` instead of `turn`",
                    event.event_id, event.scope
                )));
            }
            let occurrence = DynamicContextOccurrence::new(
                event.source_id.clone(),
                event.occurrence_id.clone(),
                event.tag.clone(),
                event.content_hash.clone(),
                event.turn_id.clone(),
            );
            inputs.push(
                DynamicContextInput::new(
                    event.source_id.clone(),
                    Some(event.body.clone()),
                    SystemContextScope::Turn,
                    None,
                )
                .as_remove(occurrence),
            );
        }
    }
    Ok(inputs)
}

/// Trims one producer body while rejecting whitespace-only values.
/// 规范化一个生产者正文并拒绝全空白值。
fn normalized_raw_content(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
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
            open_turn_id: None,
            child_business_execution_id: None,
            preparing_user_prompt_media_seqs: Vec::new(),
            compaction_frontier_seq: None,
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

    /// Checks all stable producers map to raw session-scoped inputs and preserve declared text.
    /// 检查所有稳定生产者映射为原始会话范围输入，并保留声明正文。
    #[test]
    fn dynamic_inputs_cover_layers_and_injection_once() {
        let layers = ProviderPromptLayers {
            system: String::from(" system "),
            developer: String::from("developer"),
            tool_guidance: String::from("tools"),
            runtime_context: String::from("runtime"),
        };
        let injection = ProviderContextInjection {
            persistent_blocks: vec![String::from("profile"), String::from(" rules ")],
            transient_prefix: String::from(" transient "),
            host_blocks: vec![vulcan_core::types::ProviderContextHostBlock {
                source_id: String::from("context_injection:profile:hash-1"),
                occurrence_id: String::from("context_injection:profile:hash-1:occurrence:1"),
                source: String::from("context_injection"),
                tag: Some(String::from("profile")),
                content_hash: Some(String::from("hash-1")),
                turn_id: Some(String::from("turn-1")),
                transient: true,
                target: String::from("model"),
                task_id: None,
                scope: String::from("turn"),
                body: String::from("transient"),
            }],
            ..Default::default()
        };

        let inputs = dynamic_context_inputs(&layers, Some(&injection)).expect("typed inputs");

        assert_eq!(inputs.len(), 6);
        assert_eq!(inputs[0].producer_key, DYNAMIC_CONTEXT_SYSTEM_KEY);
        assert_eq!(inputs[0].raw_content.as_deref(), Some("system"));
        assert_eq!(inputs[1].producer_key, DYNAMIC_CONTEXT_DEVELOPER_KEY);
        assert_eq!(inputs[2].producer_key, DYNAMIC_CONTEXT_TOOL_GUIDANCE_KEY);
        assert_eq!(inputs[3].producer_key, DYNAMIC_CONTEXT_RUNTIME_KEY);
        assert_eq!(inputs[4].producer_key, DYNAMIC_CONTEXT_PERSISTENT_KEY);
        assert_eq!(inputs[4].raw_content.as_deref(), Some("profile\n\nrules"));
        assert_eq!(inputs[5].producer_key, "context_injection:profile:hash-1");
        assert_eq!(inputs[5].raw_content.as_deref(), Some("transient"));
        assert!(inputs[..5].iter().all(|input| {
            input.scope == SystemContextScope::Session && input.ttl_turns.is_none()
        }));
        assert_eq!(inputs[5].scope, SystemContextScope::Turn);
        assert_eq!(inputs[5].ttl_turns, Some(1));
    }

    /// Preserves one typed expiration marker as a raw durable input with a stable producer key.
    /// 将一条强类型失效标记作为带稳定生产者键的原始可靠输入保留下来。
    #[test]
    fn dynamic_inputs_preserve_expiration_body_and_identity() {
        let injection = ProviderContextInjection {
            expired_events: vec![vulcan_core::types::ProviderContextExpiredEvent {
                event_id: String::from("context-expired:profile:hash-1:turn-1"),
                source_id: String::from("context_injection:profile:hash-1"),
                occurrence_id: String::from("context_injection:profile:hash-1:occurrence:1"),
                tag: String::from("profile"),
                content_hash: String::from("hash-1"),
                source: String::from("context_injection"),
                target: String::from("model"),
                scope: String::from("turn"),
                turn_id: String::from("turn-1"),
                body: String::from(
                    "Context injection expired after TTL: tag=profile; content_hash=hash-1; turn_id=turn-1",
                ),
            }],
            ..Default::default()
        };

        let inputs = dynamic_context_inputs(&ProviderPromptLayers::default(), Some(&injection))
            .expect("typed inputs");
        let expiration = inputs.last().expect("expiration input");
        assert_eq!(expiration.producer_key, "context_injection:profile:hash-1");
        assert_eq!(
            expiration.raw_content.as_deref(),
            Some(injection.expired_events[0].body.as_str())
        );
        assert_eq!(expiration.scope, SystemContextScope::Turn);
        assert_eq!(expiration.ttl_turns, None);
        assert_eq!(
            expiration.operation,
            vulcan_session_core::v1::dynamic_context::DynamicContextOperation::Remove
        );
        assert_eq!(
            expiration
                .occurrence
                .as_ref()
                .map(|value| value.occurrence_id.as_str()),
            Some("context_injection:profile:hash-1:occurrence:1")
        );
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
