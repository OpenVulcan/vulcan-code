// Catalog and library regressions exercise real actions through isolated channels.
// 目录及知识库回归测试通过隔离通道执行真实动作。

use super::action::{Edit, LocalSetting, UiAction, perform};
use super::pages::{RowKind, navigation, rows};
use super::*;
use crate::service_api::{Slot, knowledge_base as kb, workbench_snapshot};
use serde_json::{Value, json};
use std::sync::mpsc::{self, Receiver};

/// Builds a disconnected fixture app with an observer and no worker or discovery I/O.
/// 构建带观察端的断开夹具应用，不运行工作器或服务发现输入输出。
/// Returns the app, command receiver and immutable synthetic source document.
/// 返回应用、命令接收端及不可变合成来源文档。
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
        call: kb::list_knowledge_libraries(),
        value: fixture["loads"]["service_list_knowledge_libraries"].clone(),
    });
    receiver.try_iter().for_each(drop);
    (app, receiver, fixture)
}

/// Extracts actual catalog item identities, excluding toolbar controls.
/// 提取实际目录条目身份，排除工具栏控件。
/// `app` and `page` select rendered rows; returns their stable Item identifiers.
/// `app` 及 `page` 选择渲染行；返回其稳定 Item 标识。
fn item_ids(app: &App, page: PageId) -> Vec<String> {
    rows(app, page)
        .into_iter()
        .filter_map(|row| match row.kind {
            RowKind::Item { id, .. } => Some(id),
            _ => None,
        })
        .collect()
}
/// Editing a search query clears stale hits immediately and blank input clears the local filter.
/// 编辑查询时立即清空旧命中，空输入清除本地筛选。
#[test]
fn search_edits_prefill_queries_and_invalidate_previous_hits() {
    // Inert app with an existing completed search.
    // 已有完成搜索的静默应用。
    let (mut app, commands, _) = fixture_app();
    app.knowledge_query = "old".into();
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeSearch));
    commands.try_iter().for_each(drop);
    app.slots.set(
        Slot::KnowledgeSearch,
        kb::search_managed_knowledge_library("demo-library", "old", None),
        json!({"hits":[{"documentId":"old"}]}),
    );
    // Prepared prompt avoids overlapping mutable and immutable app borrows.
    // 预备输入框避免应用的可变及不可变借用重叠。
    let prompt = PageId::KnowledgeSearch.search(&app).unwrap();
    perform(&mut app, prompt);
    assert!(matches!(&app.overlay,Some(Overlay::Prompt(dialog)) if dialog.input == "old"));
    if let Some(Overlay::Prompt(dialog)) = &mut app.overlay {
        dialog.input = "new".into();
    }
    perform(&mut app, UiAction::PromptAccept);
    assert_eq!(app.knowledge_query, "new");
    assert!(!app.slots.loaded(Slot::KnowledgeSearch));
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method == "service_search_knowledge_library" && call.param("/request/query") == "new")));
    // Opening and clearing the same search uses its retained current query.
    // 打开并清除同一搜索时使用其保留的当前查询。
    let prompt = PageId::KnowledgeSearch.search(&app).unwrap();
    perform(&mut app, prompt);
    if let Some(Overlay::Prompt(dialog)) = &mut app.overlay {
        dialog.input.clear();
    }
    perform(&mut app, UiAction::PromptAccept);
    assert!(app.knowledge_query.is_empty());
    assert!(!commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method == "service_search_knowledge_library")));
}
/// Persistent reader errors reject stale owners and a matching retry restores readable content.
/// 持久阅读错误拒绝旧归属，匹配重试恢复可读正文。
#[test]
fn reader_failures_stay_visible_until_a_matching_retry() {
    // Isolated document reader and command observer.
    // 隔离文档阅读器及命令观察端。
    let (mut app, commands, _) = fixture_app();
    perform(
        &mut app,
        UiAction::OpenKnowledgeDocument("demo-document".into()),
    );
    commands.try_iter().for_each(drop);
    app.apply(BackendEvent::SlotFailed {
        slot: Slot::KnowledgeDocument,
        call: kb::read_managed_knowledge_document("other", "demo-document", "raw"),
        message: "stale error".into(),
    });
    assert!(!app.slot_errors.contains_key(&Slot::KnowledgeDocument));
    app.apply(BackendEvent::SlotFailed {
        slot: Slot::KnowledgeDocument,
        call: kb::read_managed_knowledge_document("demo-library", "demo-document", "raw"),
        message: "Document corpus is not built".into(),
    });
    app.toasts.clear();
    assert!(
        rows(&app, PageId::KnowledgeDocument)
            .iter()
            .any(|row| row.label == "Document corpus is not built")
    );
    perform(&mut app, UiAction::ReloadPage);
    assert!(!app.slot_errors.contains_key(&Slot::KnowledgeDocument));
    assert!(commands.try_iter().any(|command|matches!(command,Command::Call(call) if call.method == "service_read_knowledge_library_document")));
    app.apply(BackendEvent::Slot {slot:Slot::KnowledgeDocument,call:kb::read_managed_knowledge_document("demo-library","demo-document","raw"),
        value:json!({"document":{"title":"Current","relativePath":"readme.md"},"content":"restored text"})});
    assert!(
        rows(&app, PageId::KnowledgeDocument).iter().any(
            |row| matches!(&row.kind,RowKind::Document{content} if content == "restored text")
        )
    );
}

/// Server categories control navigation order, counts, source resolution and local filtering.
/// 服务分类控制导航顺序、计数、来源解析及本地筛选。
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
    // Removing a selected category resets both views without depending on unrelated project changes.
    // 删除已选分类时重置两种视图，不依赖无关的项目变化。
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

/// Changing libraries clears reader state and late document responses must match every browse dimension.
/// 切换知识库清空阅读状态，迟到文档响应必须匹配每个浏览维度。
#[test]
fn library_selection_and_document_scope_reject_stale_results() {
    // Synthetic app with one initially selected library.
    // 初始选中一个知识库的合成应用。
    let (mut app, commands, fixture) = fixture_app();
    // A second real catalog identity, not a sidebar page index.
    // 第二个真实目录身份，不使用侧栏页面下标。
    let mut catalog = fixture["loads"]["service_list_knowledge_libraries"].clone();
    // Separate empty library for owner switching.
    // 用于切换归属的独立空知识库。
    let mut second = catalog[0].clone();
    second["id"] = json!("second-library");
    second["name"] = json!("Second Library");
    second["documentCount"] = json!(0);
    catalog.as_array_mut().unwrap().push(second);
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeLibraries,
        call: kb::list_knowledge_libraries(),
        value: catalog,
    });
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeDocuments));
    assert_eq!(app.current_page(), Some(PageId::KnowledgeDocuments));
    assert_eq!(app.navigation_index(), 1);
    app.knowledge_folder = Some("guide".into());
    app.knowledge_offset = kb::DOCUMENT_PAGE_SIZE;
    app.knowledge_document = Some("demo-document".into());
    app.knowledge_query = "old query".into();
    app.slots.set(
        Slot::KnowledgeDocument,
        kb::read_managed_knowledge_document("demo-library", "demo-document", "raw"),
        json!({"content":"old text"}),
    );
    perform(&mut app, UiAction::Navigate(2));
    assert_eq!(app.library.as_deref(), Some("second-library"));
    assert_eq!(app.current_page(), Some(PageId::KnowledgeLibrary));
    assert_eq!(app.knowledge_folder, None);
    assert_eq!(app.knowledge_offset, 0);
    assert_eq!(app.knowledge_document, None);
    assert!(app.knowledge_query.is_empty());
    assert!(!app.slots.loaded(Slot::KnowledgeDocument));
    assert!(commands.try_iter().any(|command| matches!(command, Command::Call(call) if call.method == "service_get_knowledge_import_job" && call.param("/libraryId") == "second-library")));
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeDocuments));
    for call in [
        kb::list_managed_knowledge_documents("demo-library", None, 21, 0),
        kb::list_managed_knowledge_documents("second-library", Some("guide"), 21, 0),
        kb::list_managed_knowledge_documents("second-library", None, 21, 20),
    ] {
        app.apply(BackendEvent::Slot {
            slot: Slot::KnowledgeDocuments,
            call,
            value: json!([{"id":"stale"}]),
        });
        assert!(!app.slots.loaded(Slot::KnowledgeDocuments));
    }
    app.knowledge_document = Some("wanted".into());
    for call in [
        kb::read_managed_knowledge_document("demo-library", "wanted", "raw"),
        kb::read_managed_knowledge_document("second-library", "other", "raw"),
        kb::read_managed_knowledge_document("second-library", "wanted", "display"),
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
        call: kb::read_managed_knowledge_document("second-library", "wanted", "raw"),
        value: json!({"content":"current"}),
    });
    assert_eq!(app.slots.at(Slot::KnowledgeDocument, "/content"), "current");
}

/// Root-folder filtering stays distinct from all-folders pagination and readers preserve raw newlines.
/// 根目录筛选与全部目录分页保持独立，阅读器保留原始换行。
#[test]
fn managed_document_browse_uses_exact_folders_and_reader_contract() {
    // Synthetic app and backend observer.
    // 合成应用及后端观察端。
    let (mut app, commands, fixture) = fixture_app();
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeDocuments));
    assert!(commands.try_iter().any(|command| matches!(command, Command::Call(call)
        if call.method == "service_list_knowledge_library_documents" && call.param("/request/limit") == 21 && call.param("/request/folderPath").is_null())));
    perform(
        &mut app,
        UiAction::Edit(
            Edit::Local(LocalSetting::KnowledgeFolder),
            SettingValue::Text(json!("").to_string()),
        ),
    );
    assert_eq!(app.knowledge_folder.as_deref(), Some(""));
    assert!(commands.try_iter().any(|command| matches!(command, Command::Call(call)
        if call.method == "service_list_knowledge_library_documents" && call.param("/request/folderPath") == "" && call.param("/request/offset") == 0)));
    perform(
        &mut app,
        UiAction::OpenKnowledgeDocument("demo-document".into()),
    );
    assert!(commands.try_iter().any(|command| matches!(command, Command::Call(call)
        if call.method == "service_read_knowledge_library_document" && call.param("/documentId") == "demo-document" && call.param("/corpus") == "raw")));
    app.apply(BackendEvent::Slot {slot:Slot::KnowledgeDocument,call:kb::read_managed_knowledge_document("demo-library","demo-document","raw"),
        value:json!({"document":fixture["knowledgeDocuments"][0]["document"],"content":fixture["knowledgeDocuments"][0]["raw"]})});
    assert!(
        rows(&app, PageId::KnowledgeDocument)
            .iter()
            .any(|row| matches!(&row.kind, RowKind::Document { content } if content.contains("\n  indented line\n")))
    );
}

/// Active jobs schedule one observation at a time and terminal transitions refresh visible content.
/// 活动任务每次只安排一次观察，进入终态后刷新可见内容。
#[test]
fn knowledge_jobs_poll_without_overlap_and_refresh_at_completion() {
    // Synthetic app and exact command observer.
    // 合成应用及准确命令观察端。
    let (mut app, commands, _) = fixture_app();
    perform(&mut app, UiAction::GoPage(PageId::KnowledgeDocuments));
    commands.try_iter().for_each(drop);
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeJob,
        call: kb::get_knowledge_import_job("demo-library"),
        value: json!({"id":"job","state":"running"}),
    });
    // Poll deadline follows the successful observation, independently from render frame frequency.
    // 轮询时限跟随成功观察，与渲染帧频率独立。
    let due = app.now + KNOWLEDGE_JOB_POLL_INTERVAL;
    app.tick(due - Duration::from_millis(1));
    assert!(commands.try_recv().is_err());
    app.tick(due);
    assert!(
        matches!(commands.try_recv().unwrap(), Command::Call(call) if call.method == "service_get_knowledge_import_job")
    );
    app.tick(due + Duration::from_secs(20));
    assert!(
        commands.try_recv().is_err(),
        "an outstanding observation must not accumulate polls"
    );
    app.apply(BackendEvent::Slot {
        slot: Slot::KnowledgeJob,
        call: kb::get_knowledge_import_job("demo-library"),
        value: json!({"id":"job","state":"succeeded"}),
    });
    assert!(commands.try_iter().any(|command| matches!(command, Command::Call(call) if call.method == "service_list_knowledge_library_documents")));
    app.tick(due + Duration::from_secs(30));
    assert!(commands.try_recv().is_err(), "terminal jobs stop polling");
}
