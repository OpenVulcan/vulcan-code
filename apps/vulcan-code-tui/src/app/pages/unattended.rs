// Unattended defaults page: the system deny policy and each project's replacement, mirroring the
// WebUI unattended page of the permission judge settings. They only prefill the dialog that turns
// unattended mode on; sessions keep the text the user confirmed.
// 无人值守默认值页：系统拒绝策略与各项目的替换值，镜像 WebUI 权限判定设置中的无人值守页。它们只
// 预填开启无人值守模式的对话框；会话保留用户确认的文本。

use super::*;
use crate::service_api::permission_judge as judge;

/// Saves the edited system default with surrounding whitespace removed.
/// 保存去除首尾空白后的系统默认值。
fn save_system(_: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(judge::save_unattended_system_default(value.as_str().trim()))
}

/// Turns the project's own policy on, starting from the system text, or off to follow the system.
/// 开启项目自有策略（以系统文本为起点），或关闭以跟随系统默认值。
fn set_custom(snapshot: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let project = v(snapshot, "/projectId").as_str()?;
    let system = v(snapshot, "/systemDenyPolicy")
        .as_str()
        .unwrap_or_default();
    Some(judge::save_project_unattended_default(
        project,
        value.as_bool().then_some(system),
    ))
}

/// Saves the project's own policy with surrounding whitespace removed.
/// 保存去除首尾空白后的项目自有策略。
fn save_project(snapshot: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(judge::save_project_unattended_default(
        v(snapshot, "/projectId").as_str()?,
        Some(value.as_str().trim()),
    ))
}

/// Builds the unattended defaults page.
/// 构建无人值守默认值页。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let mut rows = vec![
        section(tr!("settings.permissionJudge.unattended.systemTitle")),
        note(
            tr!("settings.permissionJudge.unattended.systemHint"),
            Tone::Muted,
        ),
    ];
    let system = app.slots.value(Slot::UnattendedSystemDefault);
    if !system.is_null() {
        rows.push(text(
            tr!("settings.permissionJudge.unattended.policy"),
            s(system, "/denyPolicy"),
            Edit::Call(Setter::Build {
                ctx: Value::Null,
                build: save_system,
            }),
        ));
    }
    rows.push(section(tr!(
        "settings.permissionJudge.unattended.projectTitle"
    )));
    rows.push(note(
        tr!("settings.permissionJudge.unattended.projectHint"),
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
    let snapshot = app.slots.value(Slot::ProjectUnattendedDefault);
    if snapshot.is_null() {
        return rows;
    }
    let own = v(snapshot, "/denyPolicy").as_str();
    rows.push(
        toggle(
            tr!("settings.permissionJudge.unattended.custom"),
            own.is_some(),
            Edit::Call(Setter::Build {
                ctx: snapshot.clone(),
                build: set_custom,
            }),
        )
        .detail(tr!("settings.permissionJudge.unattended.customHint")),
    );
    match own {
        Some(own) => rows.push(text(
            tr!("settings.permissionJudge.unattended.policy"),
            own.to_string(),
            Edit::Call(Setter::Build {
                ctx: snapshot.clone(),
                build: save_project,
            }),
        )),
        None => {
            let system = s(snapshot, "/systemDenyPolicy");
            rows.push(if system.is_empty() {
                note(
                    tr!("settings.permissionJudge.unattended.followsEmpty"),
                    Tone::Muted,
                )
            } else {
                info(
                    tr!("settings.permissionJudge.unattended.follows"),
                    system,
                    Tone::Muted,
                )
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    // Turning the project policy on starts from the system text; off follows the system again.
    // 开启项目策略以系统文本为起点；关闭则重新跟随系统默认值。
    #[test]
    fn the_project_switch_saves_the_system_text_or_follows_it() {
        let snapshot =
            json!({ "projectId": "project-a", "denyPolicy": null, "systemDenyPolicy": "no push" });
        let on = set_custom(&snapshot, &SettingValue::Bool(true)).expect("a loaded snapshot");
        assert_eq!(on.method, "service_save_project_unattended_default");
        assert_eq!(
            on.params,
            json!({ "request": { "projectId": "project-a", "denyPolicy": "no push" } })
        );
        let off = set_custom(&snapshot, &SettingValue::Bool(false)).expect("a loaded snapshot");
        assert_eq!(off.param("/request/denyPolicy"), &Value::Null);
        let text = save_project(&snapshot, &SettingValue::Text("  keep main  ".into())).unwrap();
        assert_eq!(text.param("/request/denyPolicy"), &json!("keep main"));
        let system = save_system(&Value::Null, &SettingValue::Text(" x ".into())).unwrap();
        assert_eq!(system.params, json!({ "request": { "denyPolicy": "x" } }));
    }
}
