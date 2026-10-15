//! Single action boundary for read-only knowledge navigation and human lifecycle drafts.
//! 只读知识导航及人工生命周期草稿的单一动作边界。

use super::{
    App,
    action::{Edit, LocalSetting, UiAction},
    knowledge_state::{
        KnowledgeDocumentAddress, KnowledgeDraftField, KnowledgePolicyDraft, candidate_key,
        candidate_route, embedding_candidate,
    },
    pages::PageId,
};
use crate::{
    backend::Command,
    model::{NoticeSeverity, SettingValue},
    service_api::{Slot, knowledge_base},
    tr,
};
use serde_json::Value;
use vulcan_service_api::{
    KnowledgeIndexModeRequest, KnowledgeIndexModeRequestPayload, KnowledgeLibraryCreateRequest,
};

/// Exact local navigation inputs, separate from AI-owned content mutations.
/// 精确本地导航输入，与 AI 拥有的内容变更分离。
#[derive(Debug, Clone)]
pub enum KnowledgeAction {
    /// Begins an unselected new-library policy draft.
    /// 开始未选择模式的新库策略草稿。
    Create,
    /// Freezes the selected library's current policy for explicit save.
    /// 冻结所选库当前策略以便明确保存。
    ConfigureMode,
    /// Discards only the local policy draft.
    /// 仅丢弃本地策略草稿。
    CancelDraft,
    /// Validates and sends one complete policy/create request.
    /// 验证并发送一个完整策略或创建请求。
    SaveDraft,
    /// Enters an exact logical folder and resets its cursor stack.
    /// 进入精确逻辑目录并重置游标栈。
    Folder(String),
    /// Follows one service-issued directory continuation.
    /// 遵循一个服务签发目录续读标识。
    DirectoryNext(String),
    /// Returns to the previous exact cursor.
    /// 返回先前精确游标。
    DirectoryBack,
    /// Opens an exact current or historical document.
    /// 打开精确当前或历史文档。
    Document(KnowledgeDocumentAddress),
    /// Follows one service-issued body line continuation.
    /// 遵循一个服务签发正文行续读值。
    BodyNext(usize),
    /// Returns to the previous body window.
    /// 返回先前正文窗口。
    BodyBack,
    /// Follows the last displayed immutable history sequence.
    /// 遵循最后展示不可变历史序号。
    HistoryNext(u64),
    /// Returns to the previous immutable history boundary.
    /// 返回先前不可变历史边界。
    HistoryBack,
    /// Opens one immutable change and its initial image page.
    /// 打开一个不可变变更及其初始镜像页。
    Change(u64),
    /// Follows an exact service-issued image offset.
    /// 遵循精确服务签发镜像偏移。
    ChangeNext(u64),
    /// Returns to the prior image page.
    /// 返回先前镜像页。
    ChangeBack,
    /// Follows an exact live embedding-failure offset.
    /// 遵循精确实时嵌入失败偏移。
    FailuresNext(u64),
    /// Returns to the previous failure page.
    /// 返回先前失败页。
    FailuresBack,
    /// Browses a native package directory declared by the host.
    /// 浏览宿主声明的原生套件目录。
    PackageDirectory(Option<String>),
}

/// Applies one local selection and loads only the corresponding exact read page.
/// 应用一个本地选择，并仅加载对应精确读取页面。
/// Parameters: app owns all current selectors; action is a declared human input. Returns after dispatching through UiAction.
/// 参数：app 拥有全部当前选择器；action 为声明人工输入。经 UiAction 派发后返回。
pub fn perform(app: &mut App, action: KnowledgeAction) {
    use KnowledgeAction::*;
    // A current catalog is required before opening or saving lifecycle controls.
    // 打开或保存生命周期控件前必须具有当前目录。
    if matches!(action, Create | ConfigureMode | SaveDraft)
        && (app.call_pending
            || !app.slots.loaded(Slot::KnowledgeLibraries)
            || app.slot_errors.contains_key(&Slot::KnowledgeLibraries))
    {
        return;
    }
    // Each branch chooses one authoritative page; subsequent loading uses its exact selectors.
    // 每个分支选择一个权威页面；后续加载使用其精确选择器。
    let page = match action {
        Create => {
            app.knowledge.draft = Some(KnowledgePolicyDraft::default());
            PageId::KnowledgePolicy
        }
        ConfigureMode => {
            // Exact selected registry row, without substituting a sibling library.
            // 精确所选注册表行，不替换为同级知识库。
            let Some(library) = app
                .slots
                .value(Slot::KnowledgeLibraries)
                .as_array()
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row.get("id").and_then(Value::as_str) == app.library.as_deref())
                })
                .cloned()
            else {
                return;
            };
            if library.get("lifecycleState").and_then(Value::as_str) != Some("active") {
                return;
            }
            // The current response settings shape is distinct from the human request route.
            // 当前响应设置形状与人工请求路由不同。
            let Some(mode) = library
                .pointer("/settings/indexMode")
                .cloned()
                .and_then(|value| serde_json::from_value::<KnowledgeIndexModeRequest>(value).ok())
            else {
                return;
            };
            // Existing route selection is retained only when the exact official candidate still exists.
            // 仅精确官方候选仍存在时保留既有路由选择。
            let embedding = library
                .pointer("/settings/embedding")
                .filter(|value| value.is_object());
            // Scalar fields come only from this frozen library.
            // 标量字段仅来自此冻结知识库。
            let (Some(name), Some(purpose)) =
                (library["name"].as_str(), library["description"].as_str())
            else {
                app.toast(NoticeSeverity::Error, tr!("tui.knowledge.invalidResponse"));
                return;
            };
            // Invalid external fields are reported instead of panicking inside terminal input handling.
            // 外部字段无效时明确报告，不在终端输入处理内触发 panic。
            let mut draft = KnowledgePolicyDraft {
                library_id: app.library.clone(),
                name: name.into(),
                purpose: purpose.into(),
                mode: Some(mode),
                ..KnowledgePolicyDraft::default()
            };
            if let Some(embedding) = embedding {
                // Frozen dimensions and batch have one current response owner.
                // 冻结维度及批量仅具有一个当前响应所有者。
                let (Some(dimensions), Some(batch)) = (
                    embedding["dimensions"].as_u64(),
                    embedding["batchSize"].as_u64(),
                ) else {
                    app.toast(NoticeSeverity::Error, tr!("tui.knowledge.invalidResponse"));
                    return;
                };
                draft.dimensions = dimensions.to_string();
                draft.batch = batch.to_string();
                if let Some(candidates) = app
                    .slots
                    .at(Slot::PublicAiSettings, "/internalInferenceCandidates")
                    .as_array()
                {
                    for candidate in candidates
                        .iter()
                        .filter(|candidate| embedding_candidate(candidate))
                    {
                        // Both shapes have their exact proven owners; only declared execution IDs are compared.
                        // 两种形状均具有已证明精确所有者；仅比较声明执行身份。
                        let Some(route) = candidate_route(candidate) else {
                            continue;
                        };
                        if route.provider_id == embedding["route"]["providerId"]
                            && route.service_entry_id == embedding["route"]["serviceEntryId"]
                            && route.connection_id == embedding["route"]["connectionId"]
                            && route.model_id == embedding["route"]["modelId"]
                            && serde_json::to_value(route.binding_kind)
                                .expect("binding")
                                .as_str()
                                == embedding["route"]["bindingKind"].as_str()
                            && route.protocol == embedding["route"]["protocol"]
                        {
                            draft.route_key = candidate_key(candidate).expect("admitted route");
                            break;
                        }
                    }
                }
            }
            app.knowledge.draft = Some(draft);
            PageId::KnowledgePolicy
        }
        CancelDraft => {
            app.knowledge.draft = None;
            PageId::KnowledgeSettings
        }
        SaveDraft => {
            // Saving uses the complete current candidate snapshot, never the first available model.
            // 保存使用完整当前候选快照，绝非首个可用模型。
            let Some(draft) = app.knowledge.draft.as_ref() else {
                return;
            };
            // A malformed or incomplete draft remains visible for correction.
            // 畸形或未完成草稿保持可见以便修正。
            let Some(settings) = draft.settings(app.slots.value(Slot::PublicAiSettings)) else {
                app.toast(
                    NoticeSeverity::Warning,
                    tr!("knowledge.policy.dimensionRequired"),
                );
                return;
            };
            // Only the selected immutable owner decides create versus mode mutation.
            // 仅选定不可变所有者决定创建或模式变更。
            let call = if let Some(library_id) = &draft.library_id {
                if app.library.as_deref() != Some(library_id.as_str()) {
                    return;
                }
                knowledge_base::set_knowledge_index_mode(KnowledgeIndexModeRequestPayload {
                    library_id: library_id.clone(),
                    settings,
                })
            } else {
                if draft.name.trim().is_empty() {
                    return;
                }
                knowledge_base::create_knowledge_library(KnowledgeLibraryCreateRequest {
                    name: draft.name.trim().into(),
                    description: draft.purpose.clone(),
                    project_ids: Vec::new(),
                    settings,
                })
            };
            super::action::perform(app, UiAction::Send(Command::Call(call)));
            return;
        }
        Folder(path) => {
            app.knowledge.folder = path;
            app.knowledge.cursors = vec![None];
            PageId::KnowledgeDocuments
        }
        DirectoryNext(cursor) => {
            app.knowledge.cursors.push(Some(cursor));
            PageId::KnowledgeDocuments
        }
        DirectoryBack => {
            if app.knowledge.cursors.len() > 1 {
                app.knowledge.cursors.pop();
            }
            PageId::KnowledgeDocuments
        }
        Document(address) => {
            app.knowledge.document = Some(address);
            app.knowledge.starts = vec![1];
            PageId::KnowledgeDocument
        }
        BodyNext(start) => {
            app.knowledge.starts.push(start);
            PageId::KnowledgeDocument
        }
        BodyBack => {
            if app.knowledge.starts.len() > 1 {
                app.knowledge.starts.pop();
            }
            PageId::KnowledgeDocument
        }
        HistoryNext(seq) => {
            app.knowledge.history_before.push(Some(seq));
            PageId::KnowledgeHistory
        }
        HistoryBack => {
            if app.knowledge.history_before.len() > 1 {
                app.knowledge.history_before.pop();
            }
            PageId::KnowledgeHistory
        }
        Change(seq) => {
            app.knowledge.change_seq = Some(seq);
            app.knowledge.change_offsets = vec![0];
            PageId::KnowledgeChange
        }
        ChangeNext(offset) => {
            app.knowledge.change_offsets.push(offset);
            PageId::KnowledgeChange
        }
        ChangeBack => {
            if app.knowledge.change_offsets.len() > 1 {
                app.knowledge.change_offsets.pop();
            }
            PageId::KnowledgeChange
        }
        FailuresNext(offset) => {
            app.knowledge.failure_offsets.push(offset);
            PageId::KnowledgeMaintenance
        }
        FailuresBack => {
            if app.knowledge.failure_offsets.len() > 1 {
                app.knowledge.failure_offsets.pop();
            }
            PageId::KnowledgeMaintenance
        }
        PackageDirectory(path) => {
            app.knowledge.package_path = path;
            PageId::KnowledgePackages
        }
    };
    // Loading clears the exact dependent DTO, while the backend independently rejects older slot sequences.
    // 加载清空精确依赖 DTO，同时后端独立拒绝较旧槽位序号。
    for call in page.loaders(app) {
        if let crate::service_api::CallKind::Load(slot) = call.kind
            && !matches!(slot, Slot::KnowledgeLibraries | Slot::PublicAiSettings)
        {
            app.slots.clear(slot);
            app.slot_errors.remove(&slot);
        }
    }
    app.pages.remove(&page);
    super::action::perform(app, UiAction::GoPage(page));
}

/// Updates only a process-local policy draft, without calling a persistence API.
/// 仅更新进程本地策略草稿，不调用持久化 API。
/// Parameters: app owns the draft, field is declared by its row, value is the exact human input. Returns after local validation.
/// 参数：app 拥有草稿，field 由其行声明，value 为精确人工输入。本地验证后返回。
pub fn edit_draft(app: &mut App, field: KnowledgeDraftField, value: &SettingValue) {
    // A new route's official default is read before borrowing the draft mutably.
    // 可变借用草稿前读取新路由官方默认值。
    let default_dimension = if field == KnowledgeDraftField::Route {
        app.slots
            .at(Slot::PublicAiSettings, "/internalInferenceCandidates")
            .as_array()
            .and_then(|candidates| {
                candidates.iter().find(|candidate| {
                    candidate_key(candidate).as_deref() == Some(value.as_str())
                        && embedding_candidate(candidate)
                })
            })
            .and_then(|candidate| candidate.pointer("/capabilities/embedding/default_dimensions"))
            .and_then(Value::as_u64)
            .map(|value| value.to_string())
    } else {
        None
    };
    // Fields belong only to the exact currently open draft.
    // 字段仅属于当前精确打开草稿。
    let Some(draft) = app.knowledge.draft.as_mut() else {
        return;
    };
    match field {
        KnowledgeDraftField::Name if draft.library_id.is_none() => {
            draft.name = value.as_str().into()
        }
        KnowledgeDraftField::Purpose if draft.library_id.is_none() => {
            draft.purpose = value.as_str().into()
        }
        KnowledgeDraftField::Mode => {
            draft.mode = serde_json::from_value(Value::String(value.as_str().into())).ok()
        }
        KnowledgeDraftField::Route => {
            draft.route_key = value.as_str().into();
            draft.dimensions = default_dimension.unwrap_or_default();
            draft.batch.clear();
        }
        KnowledgeDraftField::Dimensions => draft.dimensions = value.as_str().into(),
        KnowledgeDraftField::Batch => draft.batch = value.as_str().into(),
        _ => {}
    }
    app.pages.remove(&PageId::KnowledgePolicy);
}

/// Produces the local draft editor consumed by shared keyboard and mouse handlers.
/// 产生共享键盘及鼠标处理器消费的本地草稿编辑器。
/// Parameters: field is a declared draft member. Returns a nonpersistent editor.
/// 参数：field 为声明草稿成员。返回非持久化编辑器。
pub fn draft_edit(field: KnowledgeDraftField) -> Edit {
    Edit::Local(LocalSetting::KnowledgeDraft(field))
}
