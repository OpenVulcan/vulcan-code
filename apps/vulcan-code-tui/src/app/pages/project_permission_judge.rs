// Permission strategy page: how the permission judge treats each category of the model in use for
// one project, mirroring the WebUI project settings section. It is listed only while the judge is
// enabled, and each change saves the complete choice map at once.
// 权限策略页：权限判定在单个项目中如何处理使用中模型的各个类别，镜像 WebUI 项目设置区块。仅在判定
// 启用时列出，每次修改立即保存完整选择映射。

use super::*;
use crate::service_api::permission_judge as judge;

// Choice value that drops the project's choice; policy modes are plain words.
// 移除项目选择的选择值；策略档位都是普通单词。
const FOLLOW_DEFAULT: &str = "@default";

/// Labels a policy category; categories come from the model in use, so unknown ones show as is.
/// 为策略类别生成标签；类别来自使用中的模型，未知类别按原样显示。
fn option_label(code: &str) -> String {
    match code {
        "cross_project_writes" => tr!("projectSettings.permissionJudge.option.crossProjectWrites"),
        "database_reset" => tr!("projectSettings.permissionJudge.option.databaseReset"),
        "project_reset" => tr!("projectSettings.permissionJudge.option.projectReset"),
        other => other.to_string(),
    }
}

/// Labels a policy mode; unknown modes show as is.
/// 为策略档位生成标签；未知档位按原样显示。
fn mode_label(code: &str) -> String {
    match code {
        "allow" => tr!("projectSettings.permissionJudge.mode.allow"),
        "review" => tr!("projectSettings.permissionJudge.mode.review"),
        "confirm" => tr!("projectSettings.permissionJudge.mode.confirm"),
        other => other.to_string(),
    }
}

/// Mirrors `nextPolicyOptions`: every current choice of the model in use, with `key` set to the
/// chosen mode or removed to follow the default.
/// 镜像 `nextPolicyOptions`：使用中模型的全部当前选择，`key` 设为所选档位，或移除以跟随默认值。
fn next_options(snapshot: &Value, key: &str, mode: &str) -> Value {
    let mut options = serde_json::Map::new();
    for option in a(snapshot, "/options") {
        if let Some(chosen) = v(option, "/chosenMode").as_str() {
            options.insert(s(option, "/key"), json!(chosen));
        }
    }
    if mode == FOLLOW_DEFAULT {
        options.remove(key);
    } else {
        options.insert(key.to_string(), json!(mode));
    }
    Value::Object(options)
}

/// Saves one changed category; `context` carries the snapshot and the category key.
/// 保存一个修改的类别；`context` 携带快照与类别键。
fn select_mode(context: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let snapshot = v(context, "/snapshot");
    Some(judge::save_project_permission_judge_policy(
        v(snapshot, "/projectId").as_str()?,
        next_options(snapshot, &s(context, "/key"), value.as_str()),
    ))
}

/// Builds the permission strategy page for the chosen project.
/// 为所选项目构建权限策略页。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let mut rows = vec![
        choice(
            tr!("tui.field.project"),
            app.projects
                .iter()
                .map(|project| (project.id.clone(), project.name.clone()))
                .collect(),
            app.judge_policy_project.clone().unwrap_or_default(),
            Edit::Local(LocalSetting::JudgePolicyProject),
        ),
        note(
            tr!("projectSettings.permissionJudge.description"),
            Tone::Muted,
        ),
    ];
    let snapshot = app.slots.value(Slot::ProjectPermissionJudgePolicy);
    if snapshot.is_null() {
        return rows;
    }
    if let Some(detail) = v(snapshot, "/issue/detail").as_str() {
        rows.push(info(
            tr!("projectSettings.permissionJudge.issue"),
            detail,
            Tone::Bad,
        ));
    }
    let options = a(snapshot, "/options");
    if options.is_empty() {
        rows.push(note(
            tr!("projectSettings.permissionJudge.empty"),
            Tone::Muted,
        ));
    }
    for option in options {
        let key = s(option, "/key");
        let mut choices = vec![(
            FOLLOW_DEFAULT.to_string(),
            tr!(
                "projectSettings.permissionJudge.followDefault",
                mode = mode_label(&s(option, "/defaultMode"))
            ),
        )];
        choices.extend(
            a(option, "/modes")
                .iter()
                .filter_map(Value::as_str)
                .map(|mode| (mode.to_string(), mode_label(mode))),
        );
        rows.push(choice(
            option_label(&key),
            choices,
            v(option, "/chosenMode")
                .as_str()
                .unwrap_or(FOLLOW_DEFAULT)
                .to_string(),
            Edit::Call(Setter::Build {
                ctx: json!({ "snapshot": snapshot, "key": key }),
                build: select_mode,
            }),
        ));
    }
    let unsupported: Vec<String> = v(snapshot, "/unsupportedOptions")
        .as_object()
        .map(|kept| kept.keys().map(|key| option_label(key)).collect())
        .unwrap_or_default();
    if !unsupported.is_empty() {
        rows.push(note(
            tr!(
                "projectSettings.permissionJudge.unsupported",
                options = unsupported.join(", ")
            ),
            Tone::Muted,
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendHandle;
    use crate::ui::theme::Theme;
    use std::sync::mpsc;

    // The settings navigation lists the permission strategy only while the judge is enabled.
    // 仅在判定启用时，设置导航才列出权限策略。
    #[test]
    fn listed_only_while_the_judge_is_enabled() {
        let (commands, _receiver) = mpsc::channel();
        let (_, events) = mpsc::channel();
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
        app.set_area(Area::Settings);
        for (judge, listed) in [
            (None, false),
            (Some(JudgeState::Off), false),
            (Some(JudgeState::Enabled), true),
        ] {
            app.permission_judge_state = judge;
            let found = navigation(&app)
                .into_iter()
                .flat_map(|(_, entries)| entries)
                .any(|entry| {
                    matches!(
                        entry.action,
                        UiAction::GoPage(PageId::ProjectPermissionJudge)
                    )
                });
            assert_eq!(found, listed, "judge state {judge:?}");
        }
    }

    // A change keeps the other current choices; following the default removes the choice.
    // 修改保留其他当前选择；跟随默认值会移除该选择。
    #[test]
    fn a_change_saves_the_complete_choices() {
        let snapshot = json!({
            "projectId": "project-a",
            "options": [
                { "key": "cross_project_writes", "chosenMode": "review" },
                { "key": "database_reset", "chosenMode": null },
            ],
        });
        assert_eq!(
            next_options(&snapshot, "database_reset", "allow"),
            json!({ "cross_project_writes": "review", "database_reset": "allow" })
        );
        assert_eq!(
            next_options(&snapshot, "cross_project_writes", FOLLOW_DEFAULT),
            json!({})
        );
        let call = select_mode(
            &json!({ "snapshot": snapshot, "key": "database_reset" }),
            &SettingValue::Choice("allow".into()),
        )
        .expect("a loaded snapshot names its project");
        assert_eq!(call.param("/request/projectId"), &json!("project-a"));
    }
}
