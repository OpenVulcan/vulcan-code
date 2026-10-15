//! Knowledge lifecycle regressions execute real UI actions through inert channels.
//! 知识库生命周期回归通过静默通道执行真实界面动作。

use super::action::{Edit, LocalSetting, UiAction, perform};
use super::knowledge_actions::KnowledgeAction;
use super::knowledge_state::{KNOWLEDGE_PAGE_SIZE, KnowledgeDocumentAddress, KnowledgeDraftField};
use super::pages::{RowKind, navigation, rows};
use super::*;
use crate::service_api::{ServiceCall, Slot, knowledge_base as kb, workbench_snapshot};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};
use vulcan_service_api::*;

/// Builds an inert app and current shared fixtures without discovery or user-state I/O.
/// 构建静默应用及当前共享夹具，不执行发现或用户状态输入输出。
/// Parameters: none. Returns the app, request observer and immutable fixture document.
/// 参数：无。返回应用、请求观察端及不可变夹具文档。
fn fixture_app() -> (App, Receiver<Command>, Value) {
    // Isolated command channel exposes requests without executing them.
    // 隔离命令通道公开请求，但不执行。
    let (commands, receiver) = mpsc::channel();
    // No backend event producer is connected.
    // 不连接后端事件生产者。
    let (_, events) = mpsc::channel();
    // Repository-owned synthetic fixture, never a user service file.
    // 仓库持有的合成夹具，不使用用户服务文件。
    let fixture: Value =
        serde_json::from_str(include_str!("../backend/mock/service-fixtures.json")).unwrap();
    // Complete UI app using inert transport.
    // 使用静默传输的完整界面应用。
    let mut app = App::new(
        BackendHandle {
            commands,
            events,
            label: "test".into(),
            is_mock: true,
        },
        Theme::dark(),
        true,
    );
    app.apply(BackendEvent::Slot {
        slot: Slot::Workbench,
        call: workbench_snapshot::load_workbench_snapshot(),
        value: fixture["workbench"].clone(),
    });
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeLibraries,
        call: kb::list_knowledge_libraries(None),
        value: fixture["loads"]["service_list_knowledge_libraries"].clone(),
    });
    receiver.try_iter().for_each(drop);
    (app, receiver, fixture)
}

/// Collects stable item identities from a real rendered page.
/// 从真实渲染页面收集稳定条目身份。
/// Parameters: app and page own the view. Returns item IDs only.
/// 参数：app 及 page 拥有视图。仅返回条目身份。
fn item_ids(app: &App, page: PageId) -> Vec<String> {
    rows(app, page)
        .into_iter()
        .filter_map(|row| match row.kind {
            RowKind::Item { id, .. } => Some(id),
            _ => None,
        })
        .collect()
}

/// Builds the exact current reader request used by the UI's initial body window.
/// 构建界面初始正文窗口使用的精确当前阅读请求。
/// Parameters: library/id select the current owner and document; start selects its first line. Returns an unexecuted call.
/// 参数：library 及 id 选择当前所有者及文档；start 选择首行。返回未执行调用。
fn read_call(library: &str, id: &str, start: usize) -> ServiceCall {
    kb::read_managed_knowledge_document(KnowledgeDocumentReadRequest {
        library_id: library.into(),
        document_id: Some(id.into()),
        path: None,
        start_line: Some(start),
        end_line: None,
    })
}

/// Builds an explicit single-library human search request.
/// 构建明确单库人工检索请求。
/// Parameters: library/query are exact current inputs. Returns an unexecuted search call.
/// 参数：library 及 query 为精确当前输入。返回未执行检索调用。
fn search_call(library: &str, query: &str) -> ServiceCall {
    kb::search_managed_knowledge_library(KnowledgeLibrarySearchRequest {
        library_id: library.into(),
        query: query.into(),
        limit: KNOWLEDGE_PAGE_SIZE,
    })
}

/// Saved dedicated sessions retain visible effective selections without exposing rejected writes.
/// 已保存专职会话保留可见有效选择，不暴露会被拒绝的写入。
/// Parameters: none. Returns after checking ordinary and dedicated pages through the same real renderer.
/// 参数：无。经同一真实渲染器检查普通及专职页面后返回。
#[test]
fn dedicated_knowledge_selection_is_read_only_without_hiding_enabled_libraries() {
    // An inert transport makes any accidental page-triggered request observable.
    // 静默传输使任何页面意外触发的请求可被观察。
    let (mut app, commands, _) = fixture_app();
    // The workbench kind flag is the same authoritative field used by the live mapper.
    // 工作台种类标记与实时映射器使用同一个权威字段。
    app.conversations = vec![crate::model::ConversationSummary {
        id: "saved-session".into(),
        title: "Saved".into(),
        project_id: Some("project".into()),
        run_phase: crate::model::RunPhase::Completed,
        last_updated_label: String::new(),
        unread: false,
        attention: false,
        dedicated: false,
    }];
    app.selected = Some("saved-session".into());
    for (slot, call, value) in [
        (
            Slot::KnowledgeSelection,
            kb::get_session_knowledge_libraries("saved-session"),
            json!({"mode":"explicit","libraryIds":["enabled-library"],"effectiveLibraryIds":["enabled-library"]}),
        ),
        (
            Slot::KnowledgeAuthorizedLibraries,
            kb::list_knowledge_libraries(Some("project")),
            json!([
                {"id":"enabled-library","name":"Enabled","description":"Purpose","settings":{"indexMode":"bm25"}},
                {"id":"disabled-library","name":"Disabled","description":"Purpose","settings":{"indexMode":"browse"}}
            ]),
        ),
        (
            Slot::ProjectKnowledgeBindings,
            kb::list_project_knowledge_bindings("project"),
            json!([]),
        ),
    ] {
        app.apply(BackendEvent::Slot { slot, call, value });
    }
    commands.try_iter().for_each(drop);
    // An ordinary saved session still owns all three explicit mutation switches.
    // 普通已保存会话仍拥有全部三个明确变更开关。
    let ordinary = rows(&app, PageId::KnowledgeSession);
    assert_eq!(ordinary.iter().filter(|row| matches!(&row.kind, RowKind::Item { actions, .. } if actions.iter().any(|action| action.action.is_some()))).count(), 3);
    // The same selection becomes read-only solely because its actual workbench kind changes.
    // 仅因实际工作台种类改变，同一选择变为只读。
    app.conversations
        .iter_mut()
        .find(|conversation| conversation.id == "saved-session")
        .unwrap()
        .dedicated = true;
    // Effective membership stays visible while every switch loses its dispatch action.
    // 有效成员关系保持可见，同时全部开关失去派发动作。
    let dedicated = rows(&app, PageId::KnowledgeSession);
    assert!(dedicated.iter().any(|row| matches!(&row.kind, RowKind::Item { id, actions, .. } if id == "enabled-library" && actions.iter().any(|action| action.switch == Some(true)))));
    assert!(dedicated.iter().all(|row| match &row.kind {
        RowKind::Item { actions, .. } => actions.iter().all(|action| action.action.is_none()),
        // Read-only recovery remains available on every service-backed page.
        // 每个服务支持页面仍保留只读恢复。
        RowKind::Button {
            action: UiAction::ReloadPage,
            ..
        } => true,
        RowKind::Button { .. }
        | RowKind::Toggle { .. }
        | RowKind::Choice { .. }
        | RowKind::Number { .. }
        | RowKind::Text { .. } => false,
        _ => true,
    }));
    assert!(commands.try_iter().next().is_none());
}

/// Search edits invalidate earlier results and blank input never calls a model.
/// 检索编辑撤销早先结果，空输入绝不调用模型。
#[test]
fn search_edits_prefill_queries_and_invalidate_previous_hits() {
    // The channel records real dispatcher effects.
    // 通道记录真实分发器效果。
    let (mut app, commands, _) = fixture_app();
    app.knowledge.query = "old".into();
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeSearch));
    commands.try_iter().for_each(drop);
    app.slots.set(
        Slot::KnowledgeSearch,
        search_call("demo-library", "old"),
        json!({"hits":[{"documentId":"old"}]}),
    );
    // Shared prompt handling is the only query submission path.
    // 共享输入处理为唯一查询提交路径。
    let prompt = PageId::KnowledgeSearch.search(&app).unwrap();
    perform(&mut app, prompt);
    assert!(matches!(&app.overlay,Some(Overlay::Prompt(dialog)) if dialog.input == "old"));
    if let Some(Overlay::Prompt(dialog)) = &mut app.overlay {
        dialog.input = "new".into();
    }
    perform(&mut app, UiAction::PromptAccept);
    assert_eq!(app.knowledge.query, "new");
    assert!(!app.slots.loaded(Slot::KnowledgeSearch));
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method == "service_search_knowledge_library" && call.param("/request/query") == "new")));
    // Clearing retains no query result and emits no search call.
    // 清空后不保留查询结果，也不发出检索调用。
    let prompt = PageId::KnowledgeSearch.search(&app).unwrap();
    perform(&mut app, prompt);
    if let Some(Overlay::Prompt(dialog)) = &mut app.overlay {
        dialog.input.clear();
    }
    perform(&mut app, UiAction::PromptAccept);
    assert!(app.knowledge.query.is_empty());
    assert!(!commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method == "service_search_knowledge_library")));
}

/// Reader errors remain visible, reject stale ownership and recover only by an exact read retry.
/// 阅读错误保持可见、拒绝过期归属，并仅经精确读取重试恢复。
#[test]
fn reader_failures_stay_visible_until_a_matching_retry() {
    // No RPC executes in this regression.
    // 此回归不执行 RPC。
    let (mut app, commands, fixture) = fixture_app();
    perform(
        &mut app,
        UiAction::Knowledge(KnowledgeAction::Document(
            KnowledgeDocumentAddress::Current("demo-document".into()),
        )),
    );
    commands.try_iter().for_each(drop);
    app.apply(BackendEvent::SlotFailed {
        slot: Slot::KnowledgeDocument,
        call: read_call("other", "demo-document", 1),
        message: "stale error".into(),
    });
    assert!(!app.slot_errors.contains_key(&Slot::KnowledgeDocument));
    // This synthetic diagnostic is retained by exact scope, independently of toast lifetime.
    // 此合成诊断按精确范围保留，独立于提示寿命。
    let diagnostic = "ERR_TEST_BODY_UNAVAILABLE";
    app.apply(BackendEvent::SlotFailed {
        slot: Slot::KnowledgeDocument,
        call: read_call("demo-library", "demo-document", 1),
        message: diagnostic.into(),
    });
    app.toasts.clear();
    assert!(
        rows(&app, PageId::KnowledgeDocument)
            .iter()
            .any(|row| row.label == diagnostic)
    );
    perform(&mut app, UiAction::ReloadPage);
    assert!(!app.slot_errors.contains_key(&Slot::KnowledgeDocument));
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method=="service_read_knowledge_library_document")));
    // Locate the fixture by stable identity rather than an evolving array position.
    // 按稳定身份定位夹具，而非会演进的数组位置。
    let entry = fixture["knowledgeDocuments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["document"]["id"] == "demo-document")
        .expect("document fixture may have changed");
    app.apply(BackendEvent::Slot { slot:Slot::KnowledgeDocument,call:read_call("demo-library","demo-document",1),value:json!({"document":entry["document"],"content":entry["body"],"startLine":1,"endLine":5,"nextLine":null}) });
    assert!(rows(&app,PageId::KnowledgeDocument).iter().any(|row|matches!(&row.kind, RowKind::Document {content} if content==entry["body"].as_str().unwrap())));
}

/// Library changes reset local selectors and reject stale current and historical body responses.
/// 知识库变化重置本地选择器，并拒绝过期当前及历史正文响应。
#[test]
fn library_selection_and_document_scope_reject_stale_results() {
    // A second explicit registry owner exercises library identity isolation.
    // 第二个明确注册表所有者验证知识库身份隔离。
    let (mut app, commands, fixture) = fixture_app();
    // Clone the selected exact row solely to synthesize another physical library.
    // 仅克隆所选精确条目以合成另一物理知识库。
    let mut second = fixture["loads"]["service_list_knowledge_libraries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "demo-library")
        .unwrap()
        .clone();
    second["id"] = json!("second-library");
    second["name"] = json!("Second");
    // Both current metadata rows belong to one complete registry snapshot.
    // 两个当前元信息条目属于一个完整注册表快照。
    let mut catalog = fixture["loads"]["service_list_knowledge_libraries"].clone();
    catalog.as_array_mut().unwrap().push(second);
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeLibraries,
        call: kb::list_knowledge_libraries(None),
        value: catalog,
    });
    app.knowledge.folder = "guide".into();
    app.knowledge.cursors.push(Some("opaque".into()));
    app.knowledge.query = "old".into();
    perform(&mut app, UiAction::OpenLibrary("second-library".into()));
    assert_eq!(app.library.as_deref(), Some("second-library"));
    assert!(app.knowledge.folder.is_empty() && app.knowledge.query.is_empty());
    assert_eq!(app.knowledge.cursors, [None]);
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeDocuments));
    commands.try_iter().for_each(drop);
    // Owner, path and cursor must each match the exact current directory request.
    // 所有者、路径及游标必须各自匹配精确当前目录请求。
    for (library, path, cursor) in [
        ("demo-library", "", None),
        ("second-library", "guide", None),
        ("second-library", "", Some("old".to_string())),
    ] {
        app.apply(BackendEvent::Slot {
            slot: Slot::KnowledgeDocuments,
            call: kb::list_managed_knowledge_documents(KnowledgeLibraryDocumentListRequest {
                library_id: library.into(),
                path: path.into(),
                depth: 1,
                cursor,
                limit: KNOWLEDGE_PAGE_SIZE,
            }),
            value: json!({"entries":[]}),
        });
        assert!(!app.slots.loaded(Slot::KnowledgeDocuments));
    }
    app.knowledge.document = Some(KnowledgeDocumentAddress::Current("wanted".into()));
    for call in [
        read_call("demo-library", "wanted", 1),
        read_call("second-library", "other", 1),
        read_call("second-library", "wanted", 9),
        kb::read_knowledge_change_document(KnowledgeChangeDocumentReadRequest {
            library_id: "second-library".into(),
            seq: 7,
            document_id: "wanted".into(),
            side: KnowledgeChangeSideRequest::Before,
            start_line: Some(1),
            end_line: None,
        }),
    ] {
        app.apply(BackendEvent::Slot {
            slot: Slot::KnowledgeDocument,
            call,
            value: json!({"content":"stale"}),
        });
        assert!(!app.slots.loaded(Slot::KnowledgeDocument));
    }
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeDocument,
        call: read_call("second-library", "wanted", 1),
        value: json!({"content":"current"}),
    });
    assert_eq!(app.slots.at(Slot::KnowledgeDocument, "/content"), "current");
}

/// Directory and body continuations use service-issued values without local arithmetic.
/// 目录及正文续读使用服务签发值，不进行本地算术推测。
#[test]
fn managed_document_browse_uses_exact_folders_and_reader_contract() {
    // Only actual dispatcher requests are observed.
    // 仅观察实际分发器请求。
    let (mut app, commands, _) = fixture_app();
    perform(
        &mut app,
        UiAction::Knowledge(KnowledgeAction::Folder("guide".into())),
    );
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method=="service_list_knowledge_library_documents" && call.param("/request/path")=="guide" && call.param("/request/cursor").is_null())));
    perform(
        &mut app,
        UiAction::Knowledge(KnowledgeAction::DirectoryNext("opaque-head-cursor".into())),
    );
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method=="service_list_knowledge_library_documents" && call.param("/request/cursor")=="opaque-head-cursor")));
    perform(
        &mut app,
        UiAction::Knowledge(KnowledgeAction::DirectoryBack),
    );
    assert_eq!(app.knowledge.cursors, [None]);
    perform(
        &mut app,
        UiAction::Knowledge(KnowledgeAction::Document(
            KnowledgeDocumentAddress::Historical {
                seq: 23,
                document_id: "stable".into(),
                side: KnowledgeChangeSideRequest::Before,
            },
        )),
    );
    commands.try_iter().for_each(drop);
    perform(&mut app, UiAction::Knowledge(KnowledgeAction::BodyNext(37)));
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method=="service_read_knowledge_change_document" && call.param("/request/seq")==23 && call.param("/request/side")=="before" && call.param("/request/startLine")==37)));
}

/// Durable jobs poll without overlap, stop on errors and never replay a paid search.
/// 持久任务轮询不重叠、在错误时停止，并绝不重放付费检索。
#[test]
fn knowledge_jobs_poll_without_overlap_and_refresh_at_completion() {
    // Maintenance is an explicit live-status page.
    // 维护为明确实时状态页面。
    let (mut app, commands, _) = fixture_app();
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeMaintenance));
    commands.try_iter().for_each(drop);
    // Latest-job discovery retains one exact library request.
    // 最新任务发现保留一个精确知识库请求。
    let call = kb::get_knowledge_operation(KnowledgeOperationRequest {
        library_id: "demo-library".into(),
        operation_id: None,
    });
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeJob,
        call: call.clone(),
        value: json!({"id":"job","state":"running"}),
    });
    // Successful observations alone arm the next polling deadline.
    // 仅成功观察设置下一轮询时限。
    let due = app.now + KNOWLEDGE_JOB_POLL_INTERVAL;
    app.tick(due - Duration::from_millis(1));
    assert!(commands.try_recv().is_err());
    app.tick(due);
    assert!(
        matches!(commands.try_recv().unwrap(),Command::Call(call) if call.method=="service_get_knowledge_operation")
    );
    app.tick(due + Duration::from_secs(20));
    assert!(commands.try_recv().is_err());
    app.apply(BackendEvent::SlotFailed {
        slot: Slot::KnowledgeJob,
        call: call.clone(),
        message: "ERR_TEST_JOB".into(),
    });
    app.tick(due + Duration::from_secs(30));
    assert!(commands.try_recv().is_err());
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeJob,
        call: call.clone(),
        value: json!({"id":"job","state":"running"}),
    });
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeJob,
        call,
        value: json!({"id":"job","state":"completed"}),
    });
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method=="service_get_knowledge_index_integrity")));
}

/// Changed heads revoke search results and continuations without implicitly submitting another query.
/// 变化头撤销检索结果及续读标识，不隐式提交另一查询。
#[test]
fn registry_changes_revoke_queries_without_paid_replay() {
    // The original query has completed under a specific content head.
    // 原查询在特定内容头下已完成。
    let (mut app, commands, fixture) = fixture_app();
    app.knowledge.query = "synthetic".into();
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeSearch));
    commands.try_iter().for_each(drop);
    app.slots.set(
        Slot::KnowledgeSearch,
        search_call("demo-library", "synthetic"),
        json!({"hits":[]}),
    );
    app.knowledge.cursors.push(Some("old-head".into()));
    // A new authoritative head makes old results stale even when the library ID is unchanged.
    // 即使知识库身份未变，新权威头仍让旧结果过期。
    let mut catalog = fixture["loads"]["service_list_knowledge_libraries"].clone();
    catalog
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["id"] == "demo-library")
        .unwrap()["headChangeSeq"] = json!(9);
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeLibraries,
        call: kb::list_knowledge_libraries(None),
        value: catalog,
    });
    assert!(app.knowledge.query.is_empty());
    assert!(!app.slots.loaded(Slot::KnowledgeSearch));
    assert_eq!(app.knowledge.cursors, [None]);
    assert!(!commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method=="service_search_knowledge_library")));
}

/// Human creation requires a selected mode and saves only once after explicit submission.
/// 人工创建要求选定模式，并仅在明确提交后保存一次。
#[test]
fn creation_drafts_are_local_explicit_and_not_replayed() {
    // No lifecycle request is sent while choosing local fields.
    // 选择本地字段期间不发送生命周期请求。
    let (mut app, commands, _) = fixture_app();
    perform(&mut app, UiAction::Knowledge(KnowledgeAction::Create));
    commands.try_iter().for_each(drop);
    assert!(app.knowledge.draft.as_ref().unwrap().mode.is_none());
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::KnowledgeDraft(KnowledgeDraftField::Name)),
            SettingValue::Text("New".into()),
        ),
    );
    perform(&mut app, UiAction::Knowledge(KnowledgeAction::SaveDraft));
    assert!(commands.try_recv().is_err());
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::KnowledgeDraft(KnowledgeDraftField::Mode)),
            SettingValue::Text("bm25".into()),
        ),
    );
    assert!(commands.try_recv().is_err());
    perform(&mut app, UiAction::Knowledge(KnowledgeAction::SaveDraft));
    // The shared mutation gate prevents a second keyboard or mouse activation.
    // 共享变更闸门阻止第二次键盘或鼠标激活。
    perform(&mut app, UiAction::Knowledge(KnowledgeAction::SaveDraft));
    // Only one fully explicit create envelope reaches the backend boundary.
    // 仅一个完整明确创建封装到达后端边界。
    let sent = commands
        .try_iter()
        .filter_map(|command| {
            if let Command::Call(call) = command {
                Some(call)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, "service_create_knowledge_library");
    assert_eq!(sent[0].param("/request/settings/indexMode"), "bm25");
    app.apply(BackendEvent::Notice(crate::backend::Notice::Completed(
        "create_knowledge_library",
    )));
    assert!(app.knowledge.draft.is_none());
}

/// Unknown and failed job reads expose no lifecycle operation, while exact active IDs remain cancellable.
/// 未知及失败任务读取不暴露生命周期操作，精确活动身份仍可取消。
#[test]
fn maintenance_requires_a_known_current_operation_state() {
    // This app never executes the rendered operation calls.
    // 此应用绝不执行已渲染操作调用。
    let (mut app, _, _) = fixture_app();
    // The current operation loader has a stable physical owner.
    // 当前操作加载器具有稳定物理所有者。
    let call = kb::get_knowledge_operation(KnowledgeOperationRequest {
        library_id: "demo-library".into(),
        operation_id: None,
    });
    for value in [json!({"state":"future_state"}), json!({}), json!(false)] {
        app.slots.set(Slot::KnowledgeJob, call.clone(), value);
        assert!(!rows(&app,PageId::KnowledgeMaintenance).iter().any(|row|matches!(&row.kind,RowKind::Button {action:UiAction::Send(Command::Call(call)),..} if matches!(call.kind,crate::service_api::CallKind::Mutate(_)))));
    }
    app.slots.set(Slot::KnowledgeJob, call.clone(), Value::Null);
    assert!(rows(&app,PageId::KnowledgeMaintenance).iter().any(|row|matches!(&row.kind,RowKind::Button {action:UiAction::Send(Command::Call(call)),..} if call.method=="service_rebuild_knowledge_index")));
    app.apply(BackendEvent::SlotFailed {
        slot: Slot::KnowledgeJob,
        call: call.clone(),
        message: "ERR_TEST_JOB".into(),
    });
    assert!(!rows(&app,PageId::KnowledgeMaintenance).iter().any(|row|matches!(&row.kind,RowKind::Button {action:UiAction::Send(Command::Call(call)),..} if matches!(call.kind,crate::service_api::CallKind::Mutate(_)))));
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeJob,
        call,
        value: json!({"id":"stable-operation","state":"running"}),
    });
    assert!(rows(&app,PageId::KnowledgeMaintenance).iter().any(|row|matches!(&row.kind,RowKind::Button {action:UiAction::Send(Command::Call(call)),..} if call.method=="service_cancel_knowledge_operation" && call.param("/request/operationId")=="stable-operation")));
}

/// Immutable history navigation retains real sequences and restoration waits for final confirmation.
/// 不可变历史导航保留真实序号，还原等待最终确认。
#[test]
fn history_uses_actual_sequences_and_explicit_restore_confirmation() {
    // Gapped synthetic sequences prevent contiguous-sequence assumptions.
    // 有间隔合成序号防止连续序号假设。
    let (mut app, commands, _) = fixture_app();
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeHistory));
    commands.try_iter().for_each(drop);
    // The next-page sentinel is a viewport policy, separate from storage bounds.
    // 下一页哨兵为视口策略，与存储边界分离。
    let changes=(0..=KNOWLEDGE_PAGE_SIZE).map(|index|json!({"seq":300-u64::from(index)*7,"operation":"update","actor":{"kind":"ai"},"note":"test"})).collect::<Vec<_>>();
    app.slots.set(
        Slot::KnowledgeChanges,
        kb::list_knowledge_changes(KnowledgeChangeListRequest {
            library_id: "demo-library".into(),
            before_seq: None,
            limit: KNOWLEDGE_PAGE_SIZE + 1,
        }),
        json!(changes),
    );
    // The rendered action freezes the actual last visible immutable sequence.
    // 已渲染动作冻结实际最后可见不可变序号。
    let next = rows(&app, PageId::KnowledgeHistory)
        .into_iter()
        .find_map(|row| match row.kind {
            RowKind::Button {
                action: UiAction::Knowledge(KnowledgeAction::HistoryNext(seq)),
                ..
            } => Some(seq),
            _ => None,
        })
        .unwrap();
    assert_eq!(next, 300 - u64::from(KNOWLEDGE_PAGE_SIZE - 1) * 7);
    perform(
        &mut app,
        UiAction::Knowledge(KnowledgeAction::HistoryNext(next)),
    );
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method=="service_list_knowledge_changes" && call.param("/request/beforeSeq")==next)));
    perform(&mut app, UiAction::Knowledge(KnowledgeAction::HistoryBack));
    commands.try_iter().for_each(drop);
    app.slots.set(
        Slot::KnowledgeChanges,
        kb::list_knowledge_changes(KnowledgeChangeListRequest {
            library_id: "demo-library".into(),
            before_seq: None,
            limit: KNOWLEDGE_PAGE_SIZE + 1,
        }),
        json!(changes),
    );
    // Choose an actual row's confirmation, without relying on its action index or translated label.
    // 选择实际行确认，不依赖动作下标或翻译标签。
    let restore=rows(&app,PageId::KnowledgeHistory).into_iter().find_map(|row|match row.kind {RowKind::Item {actions,..}=>actions.into_iter().filter_map(|action|action.action).find(|action|matches!(action,UiAction::Confirm(dialog) if matches!(&dialog.then,UiAction::Send(Command::Call(call)) if call.method=="service_restore_knowledge_library" && call.param("/request/targetSeq")==300))),_=>None}).unwrap();
    perform(&mut app, restore);
    assert!(commands.try_recv().is_err());
    perform(&mut app, UiAction::ConfirmAccept);
    assert!(
        matches!(commands.try_recv().unwrap(),Command::Call(call) if call.method=="service_restore_knowledge_library" && call.param("/request/libraryId")=="demo-library" && call.param("/request/targetSeq")==300)
    );
    perform(&mut app, UiAction::ConfirmAccept);
    assert!(commands.try_recv().is_err());
}

/// Server categories still own agent and skill navigation independently of knowledge changes.
/// 服务分类仍独立于知识库变化拥有智能体及技能导航。
#[test]
fn catalogs_use_real_categories_and_effective_skill_counts() {
    // Synthetic app and exact command observer.
    // 合成应用及准确命令观察端。
    let (mut app, commands, _) = fixture_app();
    // Deliberately different category order proves agent sorting and skill server order.
    // 特意采用不同分类顺序，用于验证智能体排序及技能服务顺序。
    let categories = json!([{"categoryId":"a","name":"A","emoji":"A","order":2},{"categoryId":"b","name":"B","emoji":"B","order":1}]);
    // User overrides shadow a same-name system record in the effective default view.
    // 用户覆盖在默认生效视图中隐藏同名系统记录。
    let mut workbench = json!({
        "subagentCatalog":{"categories":categories,"definitions":[
            {"agentId":"agent-b","categoryId":"b","name":"Agent B","enabled":true,"scopeKey":"user"},
            {"agentId":"agent-a","categoryId":"a","name":"Agent A","enabled":false,"scopeKey":"system"},
            {"agentId":"uncategorized","name":"Uncategorized","enabled":true,"scopeKey":"plugin"}]},
        "agentSkills":{"categories":categories,"records":[
            {"skillKey":"system/shared","name":"shared","displayName":"Shared","categoryId":"a","scopeKey":"system","effective":false,"enabled":true,"baseDir":"/system/shared","locales":{}},
            {"skillKey":"user/shared","name":"shared","displayName":"Shared override","categoryId":"a","scopeKey":"user","effective":true,"enabled":true,"overrideOf":"system/shared","baseDir":"/user/shared","locales":{"zh-CN":{"shortDescription":"中文匹配"}}},
            {"skillKey":"plugin/other","name":"other","displayName":"Other","categoryId":"b","scopeKey":"plugin","effective":true,"enabled":false,"baseDir":"/plugin/other","locales":{}}],
            "errors":[{"path":"/plugin/other/SKILL.md"}]}
    });
    app.apply(BackendEvent::Slot {
        slot: Slot::Workbench,
        call: workbench_snapshot::load_workbench_snapshot(),
        value: workbench.clone(),
    });
    perform(&mut app, UiAction::Area(Area::Agents));
    // Flattened projected rows retain concrete shared actions.
    // 展平的投影行保留具体共享动作。
    let entries: Vec<_> = navigation(&app)
        .into_iter()
        .flat_map(|(_, entries)| entries)
        .collect();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].badge.as_ref().unwrap().0, "3");
    assert!(matches!(&entries[1].action, UiAction::AgentCategory(id) if id == "b"));
    perform(&mut app, UiAction::Navigate(1));
    assert_eq!(item_ids(&app, PageId::AgentList), ["agent-b"]);
    assert_eq!(app.navigation_index(), 1);
    perform(&mut app, UiAction::Area(Area::AgentSkills));
    assert_eq!(
        item_ids(&app, PageId::SkillList),
        ["user/shared", "plugin/other"]
    );
    // Skill sidebar counts exclude the shadowed system version.
    // 技能侧栏计数排除已被覆盖的系统版本。
    let entries: Vec<_> = navigation(&app)
        .into_iter()
        .flat_map(|(_, entries)| entries)
        .collect();
    assert_eq!(entries[0].badge.as_ref().unwrap().0, "2");
    assert_eq!(entries[1].badge.as_ref().unwrap().0, "1");
    assert!(matches!(&entries[1].action, UiAction::SkillCategory(id) if id == "a"));
    perform(&mut app, UiAction::Navigate(1));
    assert_eq!(item_ids(&app, PageId::SkillList), ["user/shared"]);
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::SkillSource),
            SettingValue::Text("system".into()),
        ),
    );
    assert_eq!(item_ids(&app, PageId::SkillList), ["system/shared"]);
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::SkillSource),
            SettingValue::Text("overrides".into()),
        ),
    );
    app.skill_query = "中文匹配".into();
    assert_eq!(item_ids(&app, PageId::SkillList), ["user/shared"]);
    app.skill_query.clear();
    perform(&mut app, UiAction::SkillCategory(String::new()));
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::SkillSource),
            SettingValue::Text("all".into()),
        ),
    );
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::SkillStatus),
            SettingValue::Text("issues".into()),
        ),
    );
    assert_eq!(item_ids(&app, PageId::SkillList), ["plugin/other"]);
    // Removing a selected category resets both views without depending on VCE project changes.
    // 删除已选分类时重置两种视图，不依赖 VCE 项目变化。
    app.skill_category = "b".into();
    workbench["subagentCatalog"]["categories"] = json!([]);
    workbench["agentSkills"]["categories"] = json!([]);
    app.apply(BackendEvent::Slot {
        slot: Slot::Workbench,
        call: workbench_snapshot::load_workbench_snapshot(),
        value: workbench,
    });
    assert!(app.agent_category.is_empty() && app.skill_category.is_empty());
    assert!(
        commands.try_recv().is_err(),
        "catalog filters must not write to the backend"
    );
}
