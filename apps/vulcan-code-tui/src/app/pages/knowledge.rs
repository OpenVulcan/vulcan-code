//! Human library lifecycle surfaces, with AI-owned content exposed read-only.
//! 人工知识库生命周期界面，以只读方式暴露 AI 拥有内容。

use super::*;
use crate::app::{
    action::{LocalSetting, prompt},
    knowledge_actions::{KnowledgeAction, draft_edit},
    knowledge_state::{
        KNOWLEDGE_PAGE_SIZE, KnowledgeDocumentAddress, KnowledgeDraftField, candidate_key,
        embedding_candidate, mode_label,
    },
};
use api::knowledge_base as knowledge;
use vulcan_service_api::*;

#[path = "knowledge_content.rs"]
mod content;
#[path = "knowledge_history.rs"]
mod history_views;
#[path = "knowledge_management.rs"]
mod management;
pub(super) use content::{document, documents, search};
pub(super) use history_views::{change, history};
pub(super) use management::{bindings, dictionary, maintenance, packages, policy, settings};
#[path = "knowledge_session.rs"]
mod session;
pub(super) use session::session_selection;

/// Maps an explicit local view code to its only knowledge page.
/// 将明确本地视图代码映射为唯一知识库页面。
/// Parameters: key is produced by view_options. Returns None for an unknown view.
/// 参数：key 由 view_options 产出。未知视图返回空值。
pub fn knowledge_view(key: &str) -> Option<PageId> {
    match key {
        "overview" => Some(PageId::KnowledgeLibrary),
        "documents" => Some(PageId::KnowledgeDocuments),
        "search" => Some(PageId::KnowledgeSearch),
        "settings" => Some(PageId::KnowledgeSettings),
        "dictionary" => Some(PageId::KnowledgeDictionary),
        "bindings" => Some(PageId::KnowledgeBindings),
        "maintenance" => Some(PageId::KnowledgeMaintenance),
        "history" => Some(PageId::KnowledgeHistory),
        "packages" => Some(PageId::KnowledgePackages),
        _ => None,
    }
}

/// Lists exact local view identities, without service category indexes.
/// 列出精确本地视图身份，不使用服务分类下标。
/// Parameters: none. Returns translated local navigation options.
/// 参数：无。返回翻译本地导航选项。
fn view_options() -> Vec<(String, String)> {
    [
        "overview",
        "documents",
        "search",
        "history",
        "maintenance",
        "bindings",
        "dictionary",
        "settings",
        "packages",
    ]
    .into_iter()
    .map(|key| {
        (
            key.into(),
            knowledge_view(key)
                .expect("declared knowledge view")
                .label(),
        )
    })
    .collect()
}

/// Finds only the selected physical library in the current registry.
/// 仅在当前注册表中查找所选物理知识库。
/// Parameters: app owns the exact selected ID. Returns its row or no selection.
/// 参数：app 拥有精确选定身份。返回其行或无选择。
fn selected(app: &App) -> Option<&Value> {
    // The registry is the sole owner of library metadata and mode.
    // 注册表为知识库元信息及模式唯一所有者。
    let id = app.library.as_deref()?;
    a(app.slots.value(Slot::KnowledgeLibraries), "")
        .iter()
        .find(|library| library.get("id").and_then(Value::as_str) == Some(id))
}

/// Restricts actionable detail views to the current exact-format active library.
/// 将可操作详情视图限制为当前精确格式活动知识库。
/// Parameters: app supplies the registry. Returns only an active row with current settings.
/// 参数：app 提供注册表。仅返回具有当前设置的活动行。
fn active_library(app: &App) -> Option<&Value> {
    selected(app).filter(|library| {
        library.get("lifecycleState").and_then(Value::as_str) == Some("active")
            && library.get("settings").is_some_and(Value::is_object)
    })
}

/// Builds the shared owner header; unavailable old formats expose only their diagnostic and delete.
/// 构建共享所有者标题；不可用旧格式仅暴露其诊断及删除。
/// Parameters: app owns the exact selection; page identifies the current view. Returns read-only header rows.
/// 参数：app 拥有精确选择；page 标识当前视图。返回只读标题行。
fn header(app: &App, page: PageId) -> Vec<Row> {
    // Missing selection is distinct from a fabricated empty current library.
    // 缺少选择与伪造空当前知识库不同。
    let Some(library) = selected(app) else {
        return vec![
            note(tr!("knowledge.selectLibrary"), Tone::Muted),
            button(
                tr!("tui.knowledge.catalog"),
                UiAction::GoPage(PageId::KnowledgeBases),
                Tone::Accent,
            ),
        ];
    };
    // The stable registry ID is used by all lifecycle calls.
    // 全部生命周期调用使用稳定注册表身份。
    let id = s(library, "/id");
    // Metadata remains visible when the format cannot be read.
    // 格式无法读取时元信息仍保持可见。
    let mut rows = vec![
        section(s(library, "/name")),
        note(s(library, "/description"), Tone::Muted),
    ];
    if active_library(app).is_none() {
        rows.push(note(tr!("knowledge.unavailableDeleteOnly"), Tone::Bad));
        if let Some(error) = library.get("errorMessage").and_then(Value::as_str) {
            rows.push(note(error.into(), Tone::Bad));
        }
        rows.push(button(
            tr!("knowledge.delete"),
            confirm(
                tr!("knowledge.delete"),
                tr!("knowledge.deleteConfirm", name = s(library, "/name")),
                tr!("knowledge.delete"),
                call(knowledge::delete_knowledge_library(&id)),
            ),
            Tone::Bad,
        ));
        return rows;
    }
    // Reader and immutable detail belong to their corresponding parent tabs.
    // 阅读器及不可变详情属于对应父标签。
    let active = match page {
        PageId::KnowledgeDocument => PageId::KnowledgeDocuments,
        PageId::KnowledgeChange => PageId::KnowledgeHistory,
        _ => page,
    };
    // Every declared detail view has one explicit navigation code.
    // 每个声明详情视图具有一个明确导航代码。
    let current = view_options()
        .into_iter()
        .find(|(key, _)| knowledge_view(key) == Some(active))
        .map(|(key, _)| key);
    if let Some(current) = current {
        rows.push(choice(
            tr!("tui.knowledge.view"),
            view_options(),
            current,
            Edit::Local(LocalSetting::KnowledgeView),
        ));
    }
    rows
}

/// Keeps failed reads distinct from empty results, while the generic page supplies exact read retry.
/// 保持读取失败与空结果不同，同时通用页面提供精确读取重试。
/// Parameters: app/slot identify the owning read; rows receives loading feedback. Returns whether the current DTO exists.
/// 参数：app 及 slot 标识所属读取；rows 接收加载反馈。返回当前 DTO 是否存在。
fn ready(app: &App, slot: Slot, rows: &mut Vec<Row>) -> bool {
    if app.slots.loaded(slot)
        && !app.slot_errors.contains_key(&slot)
        && app
            .slots
            .loader(slot)
            .is_some_and(|call| app.slot_is_current(slot, call))
    {
        return true;
    }
    if !app.slot_errors.contains_key(&slot) {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
    }
    false
}

/// Constructs each read from exact current selectors and shared request DTOs.
/// 从精确当前选择器及共享请求 DTO 构建每个读取。
/// Parameters: app supplies selection; page declares its needed slots. Returns one-to-one WebUI service calls.
/// 参数：app 提供选择；page 声明其所需槽位。返回一对一 WebUI 服务调用。
pub(super) fn loaders(app: &App, page: PageId) -> Vec<ServiceCall> {
    // Registry is always a real read, with no successful-empty transport fallback.
    // 注册表始终为真实读取，不提供成功空传输兜底。
    let mut calls = vec![knowledge::list_knowledge_libraries(None)];
    if matches!(page, PageId::KnowledgeSettings | PageId::KnowledgePolicy) {
        calls.push(api::settings_actions::load_public_ai_settings());
    }
    if page == PageId::KnowledgePackages {
        calls.push(knowledge::browse_knowledge_packages(
            app.knowledge.package_path.as_deref(),
        ));
    }
    if page == PageId::KnowledgeSession {
        if let Some((conversation_id, project_id)) = session::session_scope(app) {
            calls.push(knowledge::get_session_knowledge_libraries(&conversation_id));
            calls.push(knowledge::list_knowledge_libraries(Some(&project_id)));
            calls.push(knowledge::list_project_knowledge_bindings(&project_id));
        }
        return calls;
    }
    // No library-owned read is sent until the registry proves an active current-format owner.
    // 注册表证明当前格式活动所有者前，不发送任何库所属读取。
    let Some(library) = active_library(app) else {
        return calls;
    };
    // Physical ownership never comes from display names or positional indexes.
    // 物理归属绝不来自展示名称或位置下标。
    let id = s(library, "/id");
    match page {
        PageId::KnowledgeLibrary | PageId::KnowledgeMaintenance => {
            calls.push(knowledge::get_knowledge_index_integrity(&id));
            calls.push(knowledge::get_knowledge_operation(
                KnowledgeOperationRequest {
                    library_id: id.clone(),
                    operation_id: None,
                },
            ));
            calls.push(knowledge::get_knowledge_embedding_status(
                KnowledgeEmbeddingStatusRequest {
                    library_id: id,
                    offset: *app
                        .knowledge
                        .failure_offsets
                        .last()
                        .expect("initial failure offset"),
                    limit: KNOWLEDGE_PAGE_SIZE,
                },
            ));
        }
        PageId::KnowledgeBindings => calls.push(knowledge::list_knowledge_project_bindings(&id)),
        PageId::KnowledgeDictionary => calls.push(knowledge::list_knowledge_custom_words(&id)),
        PageId::KnowledgeDocuments => calls.push(knowledge::list_managed_knowledge_documents(
            app.knowledge.directory_request(&id),
        )),
        PageId::KnowledgeDocument => {
            if let Some(call) = app.knowledge.document_call(&id) {
                calls.push(call);
            }
        }
        PageId::KnowledgeSearch
            if !app.knowledge.query.is_empty()
                && library
                    .pointer("/settings/indexMode")
                    .and_then(Value::as_str)
                    != Some("browse") =>
        {
            calls.push(knowledge::search_managed_knowledge_library(
                KnowledgeLibrarySearchRequest {
                    library_id: id,
                    query: app.knowledge.query.clone(),
                    limit: KNOWLEDGE_PAGE_SIZE,
                },
            ))
        }
        PageId::KnowledgeHistory => calls.push(knowledge::list_knowledge_changes(
            KnowledgeChangeListRequest {
                library_id: id,
                before_seq: *app
                    .knowledge
                    .history_before
                    .last()
                    .expect("initial history boundary"),
                limit: KNOWLEDGE_PAGE_SIZE + 1,
            },
        )),
        PageId::KnowledgeChange => {
            if let Some(seq) = app.knowledge.change_seq {
                calls.push(knowledge::get_knowledge_change(KnowledgeChangeGetRequest {
                    library_id: id,
                    seq,
                    offset: *app
                        .knowledge
                        .change_offsets
                        .last()
                        .expect("initial change offset"),
                    limit: KNOWLEDGE_PAGE_SIZE,
                }));
            }
        }
        _ => {}
    }
    calls
}

/// Lists real registry entries and explicit empty-library/package creation controls.
/// 列出真实注册表条目及明确空库和套件创建控件。
/// Parameters: app owns the registry read. Returns lifecycle actions, never content-writing controls.
/// 参数：app 拥有注册表读取。返回生命周期动作，绝不返回内容写入控件。
pub(super) fn libraries(app: &App) -> Vec<Row> {
    // These actions collect explicit inputs before any write.
    // 这些动作在任何写入前收集明确输入。
    let mut rows = vec![note(tr!("knowledge.managedDescription"), Tone::Muted)];
    if !ready(app, Slot::KnowledgeLibraries, &mut rows) {
        return rows;
    }
    rows.push(button(
        tr!("knowledge.policy.createTitle"),
        UiAction::Knowledge(KnowledgeAction::Create),
        Tone::Accent,
    ));
    rows.push(button(
        tr!("knowledge.package.import"),
        UiAction::GoPage(PageId::KnowledgePackages),
        Tone::Normal,
    ));
    rows.push(button(
        tr!("knowledge.composer.aria"),
        UiAction::GoPage(PageId::KnowledgeSession),
        Tone::Normal,
    ));
    for library in a(app.slots.value(Slot::KnowledgeLibraries), "") {
        // Each row captures its immutable physical ID at action construction.
        // 每行在构建动作时捕获其不可变物理身份。
        let id = s(library, "/id");
        rows.push(
            item(&id, s(library, "/name"), s(library, "/description"))
                .badge(
                    tr!(
                        "knowledge.documentCount",
                        count = n(library, "/documentCount")
                    ),
                    Tone::Muted,
                )
                .actions(vec![
                    action(
                        tr!("knowledge.view"),
                        UiAction::OpenLibrary(id.clone()),
                        Tone::Accent,
                    ),
                    action(
                        tr!("knowledge.delete"),
                        confirm(
                            tr!("knowledge.delete"),
                            tr!("knowledge.deleteConfirm", name = s(library, "/name")),
                            tr!("knowledge.delete"),
                            call(knowledge::delete_knowledge_library(&id)),
                        ),
                        Tone::Bad,
                    ),
                ]),
        );
    }
    if a(app.slots.value(Slot::KnowledgeLibraries), "").is_empty() {
        rows.push(note(tr!("knowledge.empty"), Tone::Muted));
    }
    rows
}

/// Shows human purpose, current counts, mode and live derived-state observations.
/// 显示人工用途、当前数量、模式及实时派生状态观察。
/// Parameters: app supplies the current library and bounded status slots. Returns its read-only overview.
/// 参数：app 提供当前知识库及有界状态槽位。返回其只读概览。
pub(super) fn library(app: &App) -> Vec<Row> {
    // The header handles unavailable formats before any actionable panel is rendered.
    // 标题在渲染任何可操作面板前处理不可用格式。
    let mut rows = header(app, PageId::KnowledgeLibrary);
    let Some(library) = active_library(app) else {
        return rows;
    };
    rows.push(note(tr!("knowledge.content.readOnly"), Tone::Muted));
    rows.push(info(
        tr!("knowledge.indexMode"),
        mode_label(&s(library, "/settings/indexMode")),
        Tone::Accent,
    ));
    rows.push(stats(vec![
        (
            tr!("tui.knowledge.documents"),
            n(library, "/documentCount").to_string(),
            Tone::Normal,
        ),
        (
            tr!("tui.knowledge.chunks"),
            n(library, "/chunkCount").to_string(),
            Tone::Normal,
        ),
    ]));
    rows.push(button(
        tr!("knowledge.tabs.content"),
        UiAction::GoPage(PageId::KnowledgeDocuments),
        Tone::Accent,
    ));
    rows.push(button(
        tr!("knowledge.tabs.history"),
        UiAction::GoPage(PageId::KnowledgeHistory),
        Tone::Normal,
    ));
    rows.extend(management::status_rows(app));
    rows
}
