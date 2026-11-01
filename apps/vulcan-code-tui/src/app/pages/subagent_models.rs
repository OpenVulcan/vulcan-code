// Subagent models page: the system default allowlist of models subagents may use, and the selected
// saved session's own list, mirroring the WebUI subagent models settings and the session settings
// entry. Every change saves the complete list; the next turn reads it.
// 子代理模型页：子代理可使用模型的系统默认白名单，以及所选已保存会话的自有清单，镜像 WebUI 子代理模型
// 设置与会话设置入口。每次修改都保存完整清单；下一回合即读取。

use super::*;
use crate::service_api::subagent_models as api_models;

/// The selected saved conversation; drafts have no session list yet.
/// 所选已保存会话；草稿尚无会话清单。
pub(super) fn saved_conversation(app: &App) -> Option<&ConversationSummary> {
    app.conversation()
        .filter(|conversation| !conversation.id.starts_with("draft:"))
}

/// Saves `models` to the session named by `ctx.sessionId`, else to the system list.
/// 将 `models` 保存到 `ctx.sessionId` 指明的会话；否则保存到系统清单。
fn save(ctx: &Value, models: Value) -> ServiceCall {
    match v(ctx, "/sessionId").as_str() {
        Some(session) => api_models::save_session_subagent_models(
            session,
            models,
            v(ctx, "/includeSessionModel").clone(),
        ),
        None => api_models::save_subagent_models(models, v(ctx, "/includeSessionModel").clone()),
    }
}

/// Saves a checkbox change with its complete list and owner; returns the unexecuted service call.
/// 将复选框变更与完整清单及所有者一起保存；返回尚未执行的服务调用。
fn set_inclusion(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    // Clone only the hydrated edit context, preserving models and session identity.
    // 仅复制已水合的编辑上下文，保留模型及会话身份。
    let mut next = ctx.clone();
    next["includeSessionModel"] = json!(value.as_bool());
    Some(save(&next, v(ctx, "/models").clone()))
}

/// `models` with the entry at `index` moved by `offset`, or `None` past either end.
/// 将 `index` 处条目移动 `offset` 后的 `models`；越过两端时返回 `None`。
fn moved(models: &[Value], index: usize, offset: isize) -> Option<Value> {
    let target = index.checked_add_signed(offset)?;
    if target >= models.len() {
        return None;
    }
    let mut next = models.to_vec();
    next.swap(index, target);
    Some(Value::Array(next))
}

/// `models` without the entry at `index`.
/// 移除 `index` 处条目后的 `models`。
fn removed(models: &[Value], index: usize) -> Value {
    let mut next = models.to_vec();
    next.remove(index);
    Value::Array(next)
}

/// Replaces one entry's note; `ctx` carries `{ models, index, sessionId? }`.
/// 替换一个条目的备注；`ctx` 携带 `{ models, index, sessionId? }`。
fn set_note(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let index = usize::try_from(v(ctx, "/index").as_u64()?).ok()?;
    let mut models = a(ctx, "/models").to_vec();
    models.get_mut(index)?["note"] = json!(value.as_str().trim());
    Some(save(ctx, Value::Array(models)))
}

/// Appends the picked model with an empty note; `ctx` carries `{ models, sessionId? }` and the
/// choice is the `[providerId, modelId]` pair. A model already listed is not added twice.
/// 追加所选模型且备注为空；`ctx` 携带 `{ models, sessionId? }`，选项为 `[providerId, modelId]`。
/// 已列出的模型不会重复添加。
fn add(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let [provider, model]: [String; 2] = serde_json::from_str(value.as_str()).ok()?;
    let mut models = a(ctx, "/models").to_vec();
    if models
        .iter()
        .any(|entry| s(entry, "/providerId") == provider && s(entry, "/modelId") == model)
    {
        return None;
    }
    models.push(json!({ "providerId": provider, "modelId": model, "note": "" }));
    Some(save(ctx, Value::Array(models)))
}

/// Turns the session's own list on, starting from the system list, or off to follow it.
/// 开启会话自有清单（以系统清单为起点），或关闭以跟随系统清单。
fn set_custom(snapshot: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let session = v(snapshot, "/sessionId").as_str()?;
    let models = if value.as_bool() {
        v(snapshot, "/systemModels").clone()
    } else {
        Value::Null
    };
    Some(api_models::save_session_subagent_models(
        session,
        models,
        if value.as_bool() {
            v(snapshot, "/systemIncludeSessionModel").clone()
        } else {
            Value::Null
        },
    ))
}

/// The available models as `[providerId, modelId]` choices, each offered once.
/// 可用模型作为 `[providerId, modelId]` 选项，每个只提供一次。
fn model_choices(app: &App) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    app.models
        .iter()
        .filter(|model| seen.insert((model.provider_id.clone(), model.model_id.clone())))
        .map(|model| {
            (
                json!([model.provider_id, model.model_id]).to_string(),
                format!("{} · {}", model.label, model.provider),
            )
        })
        .collect()
}

/// Display name of one listed model, or its identifier when it is not available now.
/// 已列出模型的显示名；当前不可用时为其标识。
fn model_name(app: &App, entry: &Value) -> (String, bool) {
    let provider = s(entry, "/providerId");
    let model = s(entry, "/modelId");
    app.models
        .iter()
        .find(|info| info.provider_id == provider && info.model_id == model)
        .map_or((model.clone(), false), |info| (info.label.clone(), true))
}

/// Builds the rows of one list: an item per entry with its note, ordering and removal, then the
/// add button. `ctx` carries `{ sessionId? }`; `limits` is the snapshot with the bounds.
/// 构建一份清单的行：每个条目一项，含备注、排序与移除，然后是添加按钮。`ctx` 携带 `{ sessionId? }`；
/// `limits` 为含边界的快照。
fn list_rows(app: &App, rows: &mut Vec<Row>, models: &[Value], ctx: &Value, limits: &Value) {
    let base = |extra: Value| {
        let mut next = ctx.clone();
        next["models"] = Value::Array(models.to_vec());
        if let (Some(next), Value::Object(extra)) = (next.as_object_mut(), extra) {
            next.extend(extra);
        }
        next
    };
    // Save the checkbox together with the complete list, including explicit empty policies.
    // 将复选框与完整清单一起保存，包括显式空策略。
    rows.push(toggle(
        tr!("subagentModels.sessionModel.include"),
        b(ctx, "/includeSessionModel"),
        Edit::Call(Setter::Build {
            ctx: base(json!({})),
            build: set_inclusion,
        }),
    ));
    if models.is_empty() {
        rows.push(note(
            if b(ctx, "/includeSessionModel") {
                tr!("subagentModels.empty.detail")
            } else {
                tr!("subagentModels.disabled.detail")
            },
            Tone::Muted,
        ));
    }
    for (index, entry) in models.iter().enumerate() {
        let (name, available) = model_name(app, entry);
        let key = format!("{}/{}", s(entry, "/providerId"), s(entry, "/modelId"));
        let entry_note = s(entry, "/note");
        let mut actions = vec![action(
            tr!("subagentModels.note.label", model = name.clone()),
            super::super::action::prompt(
                tr!("subagentModels.note.label", model = name.clone()),
                tr!("subagentModels.note.placeholder"),
                entry_note.clone(),
                PromptThen::Edit(Edit::Call(Setter::Build {
                    ctx: base(json!({ "index": index })),
                    build: set_note,
                })),
            ),
            Tone::Accent,
        )];
        if let Some(up) = moved(models, index, -1) {
            actions.push(action(
                tr!("subagentModels.moveUp", model = name.clone()),
                call(save(ctx, up)),
                Tone::Normal,
            ));
        }
        if let Some(down) = moved(models, index, 1) {
            actions.push(action(
                tr!("subagentModels.moveDown", model = name.clone()),
                call(save(ctx, down)),
                Tone::Normal,
            ));
        }
        actions.push(action(
            tr!("subagentModels.remove", model = name.clone()),
            call(save(ctx, removed(models, index))),
            Tone::Bad,
        ));
        let mut row = item(&key, name, key.clone())
            .lines(vec![if entry_note.is_empty() {
                tr!("subagentModels.note.placeholder")
            } else {
                entry_note
            }])
            .actions(actions);
        if !available {
            row = row.badge(tr!("subagentModels.unavailable"), Tone::Warn);
        }
        rows.push(row);
    }
    if models.len() as i64 >= n(limits, "/maxModels") {
        rows.push(note(tr!("subagentModels.full"), Tone::Muted));
    } else {
        rows.push(
            button(
                tr!("subagentModels.add"),
                UiAction::Pick {
                    title: tr!("subagentModels.add"),
                    edit: Edit::Call(Setter::Build {
                        ctx: base(json!({})),
                        build: add,
                    }),
                    options: model_choices(app),
                    current: String::new(),
                },
                Tone::Accent,
            )
            .detail(tr!("subagentModels.addHint")),
        );
    }
}

/// Builds the subagent models page.
/// 构建子代理模型页。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let mut rows = vec![
        section(tr!("subagentModels.system.title")),
        note(tr!("subagentModels.system.detail"), Tone::Muted),
        note(tr!("subagentModels.sessionModel.detail"), Tone::Muted),
    ];
    let system = app.slots.value(Slot::SubagentModels);
    if system.is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
    } else {
        list_rows(
            app,
            &mut rows,
            a(system, "/models"),
            &json!({ "includeSessionModel": v(system, "/includeSessionModel") }),
            system,
        );
    }
    rows.push(section(tr!("subagentModels.session.dialogTitle")));
    rows.push(note(
        tr!("subagentModels.session.dialogDetail"),
        Tone::Muted,
    ));
    let Some(conversation) = saved_conversation(app) else {
        rows.push(note(tr!("subagentModels.session.draft"), Tone::Muted));
        return rows;
    };
    if conversation.dedicated {
        rows.push(note(tr!("tui.session.dedicatedNoSubagents"), Tone::Muted));
        return rows;
    }
    rows.push(info(
        tr!("knowledge.composer.label"),
        conversation.title.clone(),
        Tone::Normal,
    ));
    // A snapshot of another session is a stale read and is not shown.
    // 其他会话的快照是过期读取，不予显示。
    let snapshot = app.slots.value(Slot::SessionSubagentModels);
    if v(snapshot, "/sessionId").as_str() != Some(conversation.id.as_str()) {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return rows;
    }
    let own = v(snapshot, "/models");
    rows.push(toggle(
        tr!("subagentModels.session.custom"),
        !own.is_null(),
        Edit::Call(Setter::Build {
            ctx: snapshot.clone(),
            build: set_custom,
        }),
    ));
    match own.as_array() {
        Some(models) => list_rows(
            app,
            &mut rows,
            models,
            &json!({ "sessionId": conversation.id, "includeSessionModel": v(snapshot, "/includeSessionModel") }),
            snapshot,
        ),
        None => rows.push(note(
            tr!(
                "subagentModels.session.followSummary",
                count = a(snapshot, "/systemModels").len()
            ),
            Tone::Muted,
        )),
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checkbox edits preserve the exact owner and empty list in the service request.
    /// 复选框编辑在服务请求中保留精确所有者和空清单。
    #[test]
    fn inclusion_toggle_preserves_empty_session_policy() {
        // Only inclusion changes; the session identity and complete list remain authoritative.
        // 仅包含选择变化；会话身份和完整清单保持权威。
        let context = json!({"sessionId":"s1", "models":[], "includeSessionModel":true});
        // No real backend is called; inspect the production RPC builder output.
        // 不调用真实后端；检查生产 RPC 构造器输出。
        let saved = set_inclusion(&context, &SettingValue::Bool(false)).unwrap();
        assert_eq!(
            saved.params["request"],
            json!({"sessionId":"s1","models":[],"includeSessionModel":false})
        );
    }

    // Entry edits save the complete list to its owner; adding skips a listed model.
    // 条目编辑将完整清单保存到其所属方；添加会跳过已列出的模型。
    #[test]
    fn list_edits_save_the_complete_list_to_its_owner() {
        let entry = json!({ "providerId": "p", "modelId": "m", "note": "old" });
        let system = json!({ "models": [entry.clone()], "index": 0, "includeSessionModel": false });
        let saved = set_note(&system, &SettingValue::Text(" new ".into())).unwrap();
        assert_eq!(saved.method, "service_save_subagent_models");
        assert_eq!(saved.params["request"]["models"][0]["note"], "new");
        assert_eq!(saved.params["request"]["includeSessionModel"], false);

        let session = json!({ "models": [entry], "sessionId": "s1", "includeSessionModel": false });
        let added = add(&session, &SettingValue::Choice(r#"["q","n"]"#.into())).unwrap();
        assert_eq!(added.method, "service_save_session_subagent_models");
        assert_eq!(added.params["request"]["sessionId"], "s1");
        assert_eq!(added.params["request"]["includeSessionModel"], false);
        assert_eq!(
            added.params["request"]["models"].as_array().unwrap().len(),
            2
        );
        assert!(add(&session, &SettingValue::Choice(r#"["p","m"]"#.into())).is_none());

        let models = [json!(1), json!(2), json!(3)];
        assert_eq!(moved(&models, 0, 1), Some(json!([2, 1, 3])));
        assert_eq!(moved(&models, 0, -1), None);
        assert_eq!(moved(&models, 2, 1), None);
        assert_eq!(removed(&models, 1), json!([1, 3]));
    }

    // The session switch copies the system list or follows it again.
    // 会话开关复制系统清单或恢复跟随。
    #[test]
    fn session_switch_copies_or_follows_the_system_list() {
        let snapshot = json!({ "sessionId": "s1", "models": null, "systemIncludeSessionModel": false, "systemModels": [{ "providerId": "p" }] });
        let on = set_custom(&snapshot, &SettingValue::Bool(true)).unwrap();
        assert_eq!(on.params["request"]["models"][0]["providerId"], "p");
        assert_eq!(on.params["request"]["includeSessionModel"], false);
        let off = set_custom(&snapshot, &SettingValue::Bool(false)).unwrap();
        assert!(off.params["request"]["models"].is_null());
        assert!(off.params["request"]["includeSessionModel"].is_null());
    }
}
