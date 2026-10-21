// Review rules page: natural-language rules the reviewing LLM applies before its own guidance, as a
// system default and per-project replacements, mirroring the WebUI review rules page of the
// permission judge settings. The next review reads the saved rules.
// 复核规则页：复核 LLM 在自身指引之前应用的自然语言规则，包括系统默认值与各项目的替换值，镜像 WebUI
// 权限判定设置中的复核规则页。下一次复核即读取已保存的规则。

use super::*;
use crate::service_api::permission_judge as judge;
use crate::service_api::review_prompt;

/// Rule groups in the order the reviewer applies them; also the field names of the rules object.
/// 按复核器应用顺序排列的规则分组；同时是规则对象的字段名。
const GROUPS: [&str; 3] = ["deny", "ask", "allow"];

/// Labels one rule group.
/// 为一个规则分组生成标签。
fn group_label(group: &str) -> String {
    match group {
        "deny" => tr!("settings.permissionJudge.reviewRules.deny"),
        "ask" => tr!("settings.permissionJudge.reviewRules.ask"),
        _ => tr!("settings.permissionJudge.reviewRules.allow"),
    }
}

/// Explains one rule group.
/// 说明一个规则分组。
fn group_hint(group: &str) -> String {
    match group {
        "deny" => tr!("settings.permissionJudge.reviewRules.denyHint"),
        "ask" => tr!("settings.permissionJudge.reviewRules.askHint"),
        _ => tr!("settings.permissionJudge.reviewRules.allowHint"),
    }
}

/// Returns `rules` with one group replaced by `value` without surrounding whitespace; the other
/// groups keep their saved text, so every save sends the complete object.
/// 返回将一个分组替换为去除首尾空白的 `value` 后的 `rules`；其他分组保留已保存文本，因此每次保存都
/// 发送完整对象。
fn with_group(rules: &Value, group: &str, value: &SettingValue) -> Value {
    let mut next = json!({ "allow": "", "ask": "", "deny": "" });
    for key in GROUPS {
        next[key] = json!(s(rules, &format!("/{key}")));
    }
    next[group] = json!(value.as_str().trim());
    next
}

/// Saves one edited group of the system rules; `ctx` carries `{ group, rules }`.
/// 保存系统规则中被编辑的一个分组；`ctx` 携带 `{ group, rules }`。
fn save_system(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let group = v(ctx, "/group").as_str()?;
    Some(judge::save_review_rules_system_default(with_group(
        v(ctx, "/rules"),
        group,
        value,
    )))
}

/// Saves one edited group of the project's own rules; `ctx` carries `{ projectId, group, rules }`.
/// 保存项目自有规则中被编辑的一个分组；`ctx` 携带 `{ projectId, group, rules }`。
fn save_project(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let group = v(ctx, "/group").as_str()?;
    Some(judge::save_project_review_rules(
        v(ctx, "/projectId").as_str()?,
        with_group(v(ctx, "/rules"), group, value),
    ))
}

/// Turns the project's own rules on, starting from the system rules, or off to follow them.
/// 开启项目自有规则（以系统规则为起点），或关闭以跟随系统规则。
fn set_custom(snapshot: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let project = v(snapshot, "/projectId").as_str()?;
    let rules = if value.as_bool() {
        v(snapshot, "/systemRules").clone()
    } else {
        Value::Null
    };
    Some(judge::save_project_review_rules(project, rules))
}

/// Builds one editable row per rule group of `rules`, saving through `build`.
/// 为 `rules` 的每个规则分组构建一个可编辑行，经 `build` 保存。
fn group_rows(
    rows: &mut Vec<Row>,
    rules: &Value,
    project: Option<&Value>,
    build: fn(&Value, &SettingValue) -> Option<ServiceCall>,
) {
    for group in GROUPS {
        let mut ctx = json!({ "group": group, "rules": rules });
        if let Some(project) = project {
            ctx["projectId"] = project.clone();
        }
        rows.push(
            text(
                group_label(group),
                s(rules, &format!("/{group}")),
                Edit::Call(Setter::Build { ctx, build }),
            )
            .detail(group_hint(group)),
        );
    }
}

/// Tells whether every group of `rules` is empty.
/// 判断 `rules` 的所有分组是否都为空。
fn all_empty(rules: &Value) -> bool {
    GROUPS
        .iter()
        .all(|group| s(rules, &format!("/{group}")).is_empty())
}

/// Builds the review rules sections.
/// 构建复核规则区段。
fn rules_rows(app: &App) -> Vec<Row> {
    let mut rows = vec![
        section(tr!("settings.permissionJudge.reviewRules.systemTitle")),
        note(
            tr!("settings.permissionJudge.reviewRules.systemHint"),
            Tone::Muted,
        ),
    ];
    let system = app.slots.value(Slot::ReviewRulesSystemDefault);
    if !system.is_null() {
        group_rows(&mut rows, v(system, "/rules"), None, save_system);
    }
    rows.push(section(tr!(
        "settings.permissionJudge.reviewRules.projectTitle"
    )));
    rows.push(note(
        tr!("settings.permissionJudge.reviewRules.projectHint"),
        Tone::Muted,
    ));
    if app.projects.is_empty() {
        rows.push(note(
            tr!("settings.permissionJudge.policy.noProjects"),
            Tone::Muted,
        ));
        return rows;
    }
    rows.push(choice(
        tr!("tui.field.project"),
        app.projects
            .iter()
            .map(|project| (project.id.clone(), project.name.clone()))
            .collect(),
        app.judge_policy_project.clone().unwrap_or_default(),
        Edit::Local(LocalSetting::JudgePolicyProject),
    ));
    let snapshot = app.slots.value(Slot::ProjectReviewRules);
    if snapshot.is_null() {
        return rows;
    }
    let own = v(snapshot, "/rules");
    rows.push(
        toggle(
            tr!("settings.permissionJudge.reviewRules.custom"),
            !own.is_null(),
            Edit::Call(Setter::Build {
                ctx: snapshot.clone(),
                build: set_custom,
            }),
        )
        .detail(tr!("settings.permissionJudge.reviewRules.customHint")),
    );
    if own.is_null() {
        rows.push(note(
            if all_empty(v(snapshot, "/systemRules")) {
                tr!("settings.permissionJudge.reviewRules.followsEmpty")
            } else {
                tr!("settings.permissionJudge.reviewRules.follows")
            },
            Tone::Muted,
        ));
    } else {
        group_rows(
            &mut rows,
            own,
            Some(v(snapshot, "/projectId")),
            save_project,
        );
    }
    rows
}

/// Saves the system default review prompt without surrounding whitespace.
/// 保存去除首尾空白的系统默认复核提示词。
fn save_system_prompt(_: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(review_prompt::save_review_prompt_system_default(
        value.as_str().trim(),
    ))
}

/// Saves the session's own review prompt; `ctx` carries `{ sessionId }`.
/// 保存会话自有复核提示词；`ctx` 携带 `{ sessionId }`。
fn save_session_prompt(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(review_prompt::save_session_review_prompt(
        v(ctx, "/sessionId").as_str()?,
        Some(value.as_str().trim()),
    ))
}

/// Turns the session's own prompt on, starting from the system default, or off to follow it.
/// 开启会话自有提示词（以系统默认值为起点），或关闭以跟随系统默认值。
fn set_session_custom(snapshot: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let session = v(snapshot, "/sessionId").as_str()?;
    let system = s(snapshot, "/systemPrompt");
    Some(review_prompt::save_session_review_prompt(
        session,
        value.as_bool().then_some(system.as_str()),
    ))
}

/// Builds the additional review prompt sections: the system default, then the selected saved
/// session's own prompt, mirroring the WebUI settings section and the composer dialog.
/// 构建附加复核提示词区段：系统默认值，然后是所选已保存会话的自有提示词，镜像 WebUI 设置区段与
/// 输入区对话框。
fn prompt_rows(app: &App) -> Vec<Row> {
    let mut rows = vec![
        section(tr!("settings.permissionJudge.reviewPrompt.systemTitle")),
        note(
            tr!("settings.permissionJudge.reviewPrompt.systemHint"),
            Tone::Muted,
        ),
    ];
    let system = app.slots.value(Slot::ReviewPromptSystemDefault);
    if !system.is_null() {
        rows.push(
            text(
                tr!("settings.permissionJudge.reviewPrompt.prompt"),
                s(system, "/prompt"),
                Edit::Call(Setter::Build {
                    ctx: Value::Null,
                    build: save_system_prompt,
                }),
            )
            .detail(tr!("permissionMode.reviewPrompt.placeholder")),
        );
    }
    rows.push(section(tr!("permissionMode.reviewPrompt.title")));
    rows.push(note(tr!("permissionMode.reviewPrompt.body"), Tone::Muted));
    let Some(conversation) = super::subagent_models::saved_conversation(app) else {
        rows.push(note(
            tr!("permissionMode.unattended.draftHint"),
            Tone::Muted,
        ));
        return rows;
    };
    rows.push(info(
        tr!("knowledge.composer.label"),
        conversation.title.clone(),
        Tone::Normal,
    ));
    // A snapshot of another session is a stale read and is not shown.
    // 其他会话的快照是过期读取，不予显示。
    let snapshot = app.slots.value(Slot::SessionReviewPrompt);
    if v(snapshot, "/sessionId").as_str() != Some(conversation.id.as_str()) {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return rows;
    }
    let own = v(snapshot, "/prompt");
    rows.push(toggle(
        tr!("permissionMode.reviewPrompt.sourceSession"),
        !own.is_null(),
        Edit::Call(Setter::Build {
            ctx: snapshot.clone(),
            build: set_session_custom,
        }),
    ));
    if own.is_null() {
        rows.push(note(
            tr!("permissionMode.reviewPrompt.sourceSystem"),
            Tone::Muted,
        ));
    } else {
        rows.push(
            text(
                tr!("permissionMode.reviewPrompt.label"),
                s(snapshot, "/prompt"),
                Edit::Call(Setter::Build {
                    ctx: json!({ "sessionId": conversation.id }),
                    build: save_session_prompt,
                }),
            )
            .detail(tr!("permissionMode.reviewPrompt.placeholder")),
        );
    }
    rows
}

/// Builds the review rules page: the rules, then the additional review prompt.
/// 构建复核规则页：先是规则，然后是附加复核提示词。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let mut rows = rules_rows(app);
    rows.extend(prompt_rows(app));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    // The system prompt saves trimmed text; the session switch copies the system default or
    // follows it again, and the session text saves to its own session.
    // 系统提示词保存去除首尾空白的文本；会话开关复制系统默认值或恢复跟随，会话文本保存到其会话。
    #[test]
    fn review_prompt_edits_name_their_owner() {
        let system = save_system_prompt(&Value::Null, &SettingValue::Text(" p ".into())).unwrap();
        assert_eq!(system.method, "service_save_review_prompt_system_default");
        assert_eq!(system.param("/request/prompt"), &json!("p"));

        let snapshot = json!({ "sessionId": "s1", "prompt": null, "systemPrompt": "sys" });
        let on = set_session_custom(&snapshot, &SettingValue::Bool(true)).unwrap();
        assert_eq!(on.method, "service_save_session_review_prompt");
        assert_eq!(on.param("/request/prompt"), &json!("sys"));
        let off = set_session_custom(&snapshot, &SettingValue::Bool(false)).unwrap();
        assert_eq!(off.param("/request/prompt"), &Value::Null);

        let own = save_session_prompt(
            &json!({ "sessionId": "s1" }),
            &SettingValue::Text(" o ".into()),
        )
        .unwrap();
        assert_eq!(own.param("/request/sessionId"), &json!("s1"));
        assert_eq!(own.param("/request/prompt"), &json!("o"));
    }

    // Editing one group sends the complete trimmed rules; the project switch copies or follows the system.
    // 编辑一个分组会发送完整且去除首尾空白的规则；项目开关复制或跟随系统规则。
    #[test]
    fn edits_send_complete_rules_and_the_switch_copies_or_follows_the_system() {
        let rules = json!({ "allow": "reading logs", "ask": "", "deny": "force push" });
        let ctx = json!({ "group": "ask", "rules": rules });
        let system = save_system(&ctx, &SettingValue::Text("  installing packages ".into()))
            .expect("a group context");
        assert_eq!(system.method, "service_save_review_rules_system_default");
        assert_eq!(
            system.params,
            json!({ "request": { "rules": {
                "allow": "reading logs", "ask": "installing packages", "deny": "force push"
            } } })
        );
        let project_ctx = json!({ "group": "deny", "rules": rules, "projectId": "project-a" });
        let project = save_project(&project_ctx, &SettingValue::Text(String::new())).unwrap();
        assert_eq!(project.method, "service_save_project_review_rules");
        assert_eq!(project.param("/request/projectId"), &json!("project-a"));
        assert_eq!(project.param("/request/rules/deny"), &json!(""));
        assert_eq!(
            project.param("/request/rules/allow"),
            &json!("reading logs")
        );

        let snapshot = json!({ "projectId": "project-a", "rules": null, "systemRules": rules });
        let on = set_custom(&snapshot, &SettingValue::Bool(true)).expect("a loaded snapshot");
        assert_eq!(on.param("/request/rules"), &rules);
        let off = set_custom(&snapshot, &SettingValue::Bool(false)).expect("a loaded snapshot");
        assert_eq!(off.param("/request/rules"), &Value::Null);
    }
}
