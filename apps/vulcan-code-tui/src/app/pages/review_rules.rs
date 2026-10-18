// Review rules page: natural-language rules the reviewing LLM applies before its own guidance, as a
// system default and per-project replacements, mirroring the WebUI review rules page of the
// permission judge settings. The next review reads the saved rules.
// 复核规则页：复核 LLM 在自身指引之前应用的自然语言规则，包括系统默认值与各项目的替换值，镜像 WebUI
// 权限判定设置中的复核规则页。下一次复核即读取已保存的规则。

use super::*;
use crate::service_api::permission_judge as judge;

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

/// Builds the review rules page.
/// 构建复核规则页。
pub(super) fn rows(app: &App) -> Vec<Row> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
