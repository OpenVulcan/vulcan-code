//! Saved-session knowledge selection follows the exact project authorization ceiling.
//! 已保存会话知识库选择遵循精确项目授权上限。

use super::*;

/// Resolves the selected saved session and its actual project.
/// 解析所选已保存会话及其实际项目。
/// Parameters: app owns the current workbench conversation. Returns no scope for drafts or projectless sessions.
/// 参数：app 拥有当前工作台会话。草稿或无项目会话不返回范围。
pub(super) fn session_scope(app: &App) -> Option<(String, String)> {
    // The backend's declared draft prefix is rejected before any persistence call.
    // 后端声明草稿前缀在任何持久化调用前拒绝。
    let conversation = app.conversation()?;
    if conversation.id.starts_with("draft:") {
        return None;
    }
    Some((conversation.id.clone(), conversation.project_id.clone()?))
}

/// Shows inheritance and explicit IDs without a retrieval strategy override.
/// 显示继承及明确身份，不包含检索策略覆盖。
/// Parameters: app supplies exact session, project, grants and effective selection. Returns writable ordinary or read-only dedicated switches.
/// 参数：app 提供精确会话、项目、授权及有效选择。返回普通会话可写或专职会话只读开关。
pub(crate) fn session_selection(app: &App) -> Vec<Row> {
    // Global selection is independent from the library currently open in the manager.
    // 全局会话选择独立于管理器当前打开知识库。
    let mut rows = vec![section(tr!("knowledge.composer.aria"))];
    let Some((conversation_id, project_id)) = session_scope(app) else {
        rows.push(note(tr!("tui.knowledge.sessionUnavailable"), Tone::Muted));
        return rows;
    };
    // Workbench ownership proves the saved kind; dedicated selections are fixed at creation.
    // 工作台归属证明已保存种类；专职选择在创建时固定。
    let conversation = app.conversation().expect("resolved session");
    // Only ordinary sessions may dispatch a selection mutation from the TUI.
    // 仅普通会话可从 TUI 派发选择变更。
    let editable = !conversation.dedicated;
    rows.push(info(
        tr!("knowledge.composer.label"),
        conversation.title.clone(),
        Tone::Normal,
    ));
    if !editable {
        rows.push(note(tr!("tui.knowledge.sessionFrozen"), Tone::Muted));
    }
    if !ready(app, Slot::KnowledgeSelection, &mut rows)
        || !ready(app, Slot::KnowledgeAuthorizedLibraries, &mut rows)
        || !ready(app, Slot::ProjectKnowledgeBindings, &mut rows)
    {
        return rows;
    }
    // The session response, not the project defaults, owns effective IDs in both modes.
    // 两种模式的有效身份均由会话响应拥有，而非项目默认值。
    let selection = app.slots.value(Slot::KnowledgeSelection);
    // Every effective member must have the declared string shape.
    // 每个有效成员必须具有声明字符串形状。
    let Some(effective) = selection
        .get("effectiveLibraryIds")
        .and_then(Value::as_array)
        .and_then(|ids| {
            ids.iter()
                .map(|id| id.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
    else {
        rows.push(note("ERR_KNOWLEDGE_SESSION_SELECTION".into(), Tone::Bad));
        return rows;
    };
    // The exact declared mode determines the inheritance switch.
    // 精确声明模式决定继承开关。
    let inherited = match selection.get("mode").and_then(Value::as_str) {
        Some("inherit") => true,
        Some("explicit") => false,
        _ => {
            rows.push(note("ERR_KNOWLEDGE_SESSION_SELECTION".into(), Tone::Bad));
            return rows;
        }
    };
    rows.push(
        item("inherit", tr!("knowledge.composer.inherit"), "").actions(vec![switch(
            tr!("knowledge.composer.inherit"),
            inherited,
            editable.then(|| {
                call(knowledge::set_session_knowledge_libraries(
                    SessionKnowledgeLibrarySelectionRequest {
                        project_id: project_id.clone(),
                        conversation_id: conversation_id.clone(),
                        mode: if inherited {
                            "explicit".into()
                        } else {
                            "inherit".into()
                        },
                        library_ids: if inherited {
                            effective.clone()
                        } else {
                            Vec::new()
                        },
                    },
                ))
            }),
        )]),
    );
    for library in a(app.slots.value(Slot::KnowledgeAuthorizedLibraries), "") {
        // Only the project-authorized library catalog may expose an explicit toggle.
        // 仅项目授权知识库目录可暴露明确开关。
        let id = s(library, "/id");
        // A deduplicated exact ID set preserves current effective selection when inheritance is changed.
        // 去重精确身份集合在继承变化时保留当前有效选择。
        let mut ids = effective
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        // Presence means the exact library is enabled in this session.
        // 存在表示此会话启用精确知识库。
        let enabled = ids.contains(&id);
        if enabled {
            ids.remove(&id);
        } else {
            ids.insert(id.clone());
        }
        // Default flags come only from the exact project's grant records.
        // 默认标记仅来自精确项目授权记录。
        let default_enabled = a(app.slots.value(Slot::ProjectKnowledgeBindings), "")
            .iter()
            .any(|grant| {
                grant.get("libraryId").and_then(Value::as_str) == Some(id.as_str())
                    && b(grant, "/defaultEnabled")
            });
        rows.push(
            item(&id, s(library, "/name"), s(library, "/description"))
                .badge(
                    if default_enabled {
                        tr!("knowledge.bindings.default")
                    } else {
                        mode_label(&s(library, "/settings/indexMode"))
                    },
                    Tone::Muted,
                )
                .actions(vec![switch(
                    tr!("knowledge.composer.label"),
                    enabled,
                    editable.then(|| {
                        call(knowledge::set_session_knowledge_libraries(
                            SessionKnowledgeLibrarySelectionRequest {
                                project_id: project_id.clone(),
                                conversation_id: conversation_id.clone(),
                                mode: "explicit".into(),
                                library_ids: ids.into_iter().collect(),
                            },
                        ))
                    }),
                )]),
        );
    }
    if a(app.slots.value(Slot::KnowledgeAuthorizedLibraries), "").is_empty() {
        rows.push(note(tr!("knowledge.composer.noneAuthorized"), Tone::Muted));
    }
    rows
}
