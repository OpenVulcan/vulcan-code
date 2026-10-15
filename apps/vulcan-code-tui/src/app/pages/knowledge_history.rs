//! Permanent immutable history pages and explicitly confirmed restoration.
//! 永久不可变历史页面及明确确认还原。

use super::*;

/// Localizes only the declared content change operation codes.
/// 仅本地化声明内容变更操作代码。
/// Parameters: operation is the history row's code. Returns its shared user label.
/// 参数：operation 为历史行代码。返回其共享用户标签。
fn change_label(operation: &str) -> String {
    match operation {
        "create" => tr!("knowledge.change.create"),
        "update" => tr!("knowledge.change.update"),
        "edit" => tr!("knowledge.change.edit"),
        "move" => tr!("knowledge.change.move"),
        "delete" => tr!("knowledge.change.delete"),
        "folder" => tr!("knowledge.change.folder"),
        "delete_folder" => tr!("knowledge.change.deleteFolder"),
        "ingest" => tr!("knowledge.change.ingest"),
        "restore" => tr!("knowledge.change.restore"),
        "import" => tr!("knowledge.change.import"),
        _ => tr!("knowledge.operation.unknown"),
    }
}

/// Constructs a human confirmation bound to one exact immutable library/history target.
/// 构建绑定一个精确不可变知识库及历史目标的人工确认。
/// Parameters: library owns identity/name; seq is the explicit target, including zero. Returns only a confirmation action.
/// 参数：library 拥有身份及名称；seq 为包含零值的明确目标。仅返回确认动作。
fn restore_action(library: &Value, seq: u64) -> UiAction {
    confirm(
        tr!("knowledge.history.restore"),
        tr!(
            "knowledge.history.restoreConfirm",
            name = s(library, "/name"),
            seq = seq
        ),
        tr!("knowledge.history.restore"),
        call(knowledge::restore_knowledge_library(
            KnowledgeRestoreRequest {
                library_id: s(library, "/id"),
                target_seq: seq,
                note: String::new(),
            },
        )),
    )
}

/// Lists immutable summaries and uses the last displayed sequence as its continuation.
/// 列出不可变摘要，并使用最后展示序号作为续读值。
/// Parameters: app owns exact library and exclusive boundaries. Returns summary/detail navigation and confirmed restore actions.
/// 参数：app 拥有精确知识库及排他边界。返回摘要及详情导航和已确认还原动作。
pub(crate) fn history(app: &App) -> Vec<Row> {
    // Old formats cannot enter history or restoration controls.
    // 旧格式不能进入历史或还原控件。
    let mut rows = header(app, PageId::KnowledgeHistory);
    let Some(library) = active_library(app) else {
        return rows;
    };
    rows.push(note(tr!("knowledge.history.description"), Tone::Muted));
    rows.push(button(
        tr!("knowledge.history.restoreEmpty"),
        restore_action(library, 0),
        Tone::Bad,
    ));
    if !ready(app, Slot::KnowledgeChanges, &mut rows) {
        return rows;
    }
    // The service returns one extra summary solely to detect a following page.
    // 服务额外返回一个摘要，仅用于检测后续页。
    let changes = a(app.slots.value(Slot::KnowledgeChanges), "");
    // The viewport is presentation policy and is shared by all TUI knowledge lists.
    // 视口为展示策略，由全部 TUI 知识列表共享。
    let visible = changes
        .iter()
        .take(KNOWLEDGE_PAGE_SIZE as usize)
        .collect::<Vec<_>>();
    for change in &visible {
        // A malformed external sequence is explicit, not a fabricated zero restore point.
        // 畸形外部序号明确报告，绝不伪造零还原点。
        let Some(seq) = change.get("seq").and_then(Value::as_u64) else {
            rows.push(note("ERR_KNOWLEDGE_CHANGE_SEQUENCE".into(), Tone::Bad));
            continue;
        };
        rows.push(
            item(
                &seq.to_string(),
                tr!("knowledge.history.change", seq = seq),
                s(change, "/note"),
            )
            .badge(change_label(&s(change, "/operation")), Tone::Muted)
            .lines(details(v(change, "/actor")))
            .actions(vec![
                action(
                    tr!("knowledge.history.compare"),
                    UiAction::Knowledge(KnowledgeAction::Change(seq)),
                    Tone::Accent,
                ),
                action(
                    tr!("knowledge.history.restore"),
                    restore_action(library, seq),
                    Tone::Bad,
                ),
            ]),
        );
    }
    if changes.is_empty() {
        rows.push(note(tr!("knowledge.history.empty"), Tone::Muted));
    }
    if app.knowledge.history_before.len() > 1 {
        rows.push(button(
            tr!("knowledge.page.previous"),
            UiAction::Knowledge(KnowledgeAction::HistoryBack),
            Tone::Normal,
        ));
    }
    if changes.len() > KNOWLEDGE_PAGE_SIZE as usize
        && let Some(seq) = visible
            .last()
            .and_then(|change| change.get("seq"))
            .and_then(Value::as_u64)
    {
        rows.push(button(
            tr!("knowledge.page.next"),
            UiAction::Knowledge(KnowledgeAction::HistoryNext(seq)),
            Tone::Normal,
        ));
    }
    rows
}

/// Displays bounded immutable before/after metadata and explicit side-specific body actions.
/// 显示有界不可变前后元信息及明确侧别正文动作。
/// Parameters: app owns one exact selected change and its service offsets. Returns no current-body substitution or content editors.
/// 参数：app 拥有一个精确选定变更及其服务偏移。返回值不含当前正文替代或内容编辑器。
pub(crate) fn change(app: &App) -> Vec<Row> {
    // Read-only comparison shares the exact active format gate.
    // 只读比较共享精确活动格式门禁。
    let mut rows = header(app, PageId::KnowledgeChange);
    if active_library(app).is_none() {
        return rows;
    }
    rows.push(button(
        tr!("knowledge.tabs.history"),
        UiAction::GoPage(PageId::KnowledgeHistory),
        Tone::Normal,
    ));
    let Some(seq) = app.knowledge.change_seq else {
        return rows;
    };
    if !ready(app, Slot::KnowledgeChange, &mut rows) {
        return rows;
    }
    rows.push(section(tr!("knowledge.history.change", seq = seq)));
    rows.push(note(
        s(app.slots.value(Slot::KnowledgeChange), "/change/note"),
        Tone::Normal,
    ));
    for image in a(app.slots.value(Slot::KnowledgeChange), "/items") {
        // Entity kind is produced by the immutable history schema.
        // 实体种类由不可变历史结构产出。
        let entity = image.get("entity").and_then(Value::as_str);
        // The stable key, not either image path, selects a historical body.
        // 稳定键选择历史正文，而非任一镜像路径。
        let id = s(image, "/entityKey");
        for (side, label, field) in [
            (
                KnowledgeChangeSideRequest::Before,
                tr!("knowledge.history.before"),
                "before",
            ),
            (
                KnowledgeChangeSideRequest::After,
                tr!("knowledge.history.after"),
                "after",
            ),
        ] {
            // A missing image is a protocol error; explicit null means create/delete absence.
            // 缺失镜像为协议错误；明确空值表示创建或删除时不存在。
            let Some(value) = image.get(field) else {
                rows.push(note("ERR_KNOWLEDGE_CHANGE_IMAGE".into(), Tone::Bad));
                continue;
            };
            if value.is_null() {
                rows.push(info(label, tr!("knowledge.history.absent"), Tone::Muted));
                continue;
            }
            // Both metadata images remain visible; bodies are loaded only by explicit side actions.
            // 两侧元信息均保持可见；正文仅通过明确侧别动作加载。
            let row = item(
                &format!("{id}:{field}"),
                format!("{} · {}", label, s(value, "/path")),
                "",
            )
            .lines(details(value));
            rows.push(if entity == Some("document") {
                row.actions(vec![action(
                    tr!("knowledge.view"),
                    UiAction::Knowledge(KnowledgeAction::Document(
                        KnowledgeDocumentAddress::Historical {
                            seq,
                            document_id: id.clone(),
                            side,
                        },
                    )),
                    Tone::Accent,
                )])
            } else {
                row
            });
        }
    }
    if app.knowledge.change_offsets.len() > 1 {
        rows.push(button(
            tr!("knowledge.page.previous"),
            UiAction::Knowledge(KnowledgeAction::ChangeBack),
            Tone::Normal,
        ));
    }
    if let Some(offset) = app.slots.at(Slot::KnowledgeChange, "/nextOffset").as_u64() {
        rows.push(button(
            tr!("knowledge.page.next"),
            UiAction::Knowledge(KnowledgeAction::ChangeNext(offset)),
            Tone::Normal,
        ));
    }
    rows
}
