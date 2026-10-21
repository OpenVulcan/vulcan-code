// MCP service page: the endpoint switch and address, then one item per external AI client with its
// grants, mirroring the WebUI MCP service settings. Every change saves the client's complete
// document. Creating a client and rotating a token return a token the terminal never shows, so
// those stay in the WebUI.
// MCP 服务页：端点开关与地址，其后每个外部 AI 客户端一项及其授权，镜像 WebUI MCP 服务设置。每次修改
// 都保存客户端的完整文档。新建客户端与轮换令牌会返回终端从不显示的令牌，因此只在 WebUI 中进行。

use super::*;
use crate::service_api::mcp_service as api_mcp;

// SESSION_FLAGS lists the write permissions in WebUI order; each needs read access.
// SESSION_FLAGS 按 WebUI 顺序列出写权限；每项都需要读取权限。
const SESSION_FLAGS: [&str; 4] = ["send", "create", "stop", "answerQuestions"];

/// The short label of one session flag.
/// 一个会话开关的简短标签。
fn flag_label(flag: &str) -> String {
    match flag {
        "send" => tr!("mcpService.summary.send"),
        "create" => tr!("mcpService.summary.create"),
        "stop" => tr!("mcpService.summary.stop"),
        "answerQuestions" => tr!("mcpService.summary.answer"),
        _ => tr!("mcpService.summary.read"),
    }
}

// ALL_PROJECTS is the project choice that grants every project.
// ALL_PROJECTS 是授予全部项目的项目选项。
const ALL_PROJECTS: &str = "*";

/// The complete update request of one client summary.
/// 一个客户端摘要对应的完整更新请求。
fn request(client: &Value) -> Value {
    json!({
        "clientId": v(client, "/id"),
        "name": v(client, "/name"),
        "enabled": b(client, "/enabled"),
        "sessions": v(client, "/sessions"),
        "mediaTargets": v(client, "/mediaTargets"),
    })
}

/// Saves `client` after `change` edits its request.
/// 由 `change` 编辑请求后保存 `client`。
fn updated(client: &Value, change: impl FnOnce(&mut Value)) -> ServiceCall {
    let mut next = request(client);
    change(&mut next);
    api_mcp::update_mcp_client(next)
}

/// Sets one session flag with the WebUI rules: turning read off clears every write flag, and
/// turning a write flag on also turns read on.
/// 按 WebUI 规则设置一个会话开关：关闭读取会清除所有写权限，开启写权限会同时开启读取。
fn set_session_flag(sessions: &mut Value, flag: &str, on: bool) {
    sessions[flag] = json!(on);
    if flag == "read" && !on {
        for write in SESSION_FLAGS {
            sessions[write] = json!(false);
        }
    } else if flag != "read" && on {
        sessions["read"] = json!(true);
    }
}

/// Renames the client in `ctx`; an empty name is refused.
/// 重命名 `ctx` 中的客户端；空名称被拒绝。
fn rename(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let name = value.as_str().trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(updated(ctx, |next| next["name"] = json!(name)))
}

/// Applies a project choice to the client in `ctx`: `*` grants every project, any other id is
/// added or removed. Removing the last selected project would widen nothing safely, so it is
/// refused; the user picks all projects explicitly instead.
/// 将项目选项应用于 `ctx` 中的客户端：`*` 授予全部项目，其他标识则加入或移除。移除最后一个所选项目无法
/// 安全地表达，因此被拒绝；用户需显式选择全部项目。
fn pick_project(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let choice = value.as_str();
    if choice == ALL_PROJECTS {
        return Some(updated(ctx, |next| {
            next["sessions"]["projects"] = json!({ "mode": "all" });
        }));
    }
    let mut ids: Vec<Value> = a(ctx, "/sessions/projects/projectIds").to_vec();
    match ids.iter().position(|id| id.as_str() == Some(choice)) {
        Some(index) if ids.len() > 1 => {
            ids.remove(index);
        }
        Some(_) => return None,
        None => ids.push(json!(choice)),
    }
    Some(updated(ctx, |next| {
        next["sessions"]["projects"] = json!({ "mode": "selected", "projectIds": ids });
    }))
}

/// Adds or removes the media grant named by the `[connectionId, targetId]` choice.
/// 加入或移除 `[connectionId, targetId]` 选项所指的媒体授权。
fn pick_media(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let [connection, target]: [String; 2] = serde_json::from_str(value.as_str()).ok()?;
    let mut targets = a(ctx, "/mediaTargets").to_vec();
    let existing = targets.iter().position(|entry| {
        s(entry, "/connectionId") == connection && s(entry, "/targetId") == target
    });
    match existing {
        Some(index) => {
            targets.remove(index);
        }
        None => targets.push(json!({ "connectionId": connection, "targetId": target })),
    }
    Some(updated(ctx, |next| {
        next["mediaTargets"] = Value::Array(targets)
    }))
}

/// Marks a picker option as granted.
/// 将选择器选项标记为已授权。
fn marked(granted: bool, label: String) -> String {
    format!("{} {label}", if granted { "[x]" } else { "[ ]" })
}

/// Project choices: all projects first, then each project with its grant mark.
/// 项目选项：先是全部项目，其后为带授权标记的各项目。
fn project_choices(app: &App, client: &Value) -> Vec<(String, String)> {
    let all = s(client, "/sessions/projects/mode") == "all";
    let selected = a(client, "/sessions/projects/projectIds");
    let mut options = vec![(
        ALL_PROJECTS.to_string(),
        marked(all, tr!("mcpService.field.scopeAll")),
    )];
    options.extend(app.projects.iter().map(|project| {
        let granted = !all && selected.iter().any(|id| id.as_str() == Some(&project.id));
        (
            project.id.clone(),
            marked(granted, format!("{} · {}", project.name, project.path)),
        )
    }));
    options
}

/// Media choices: every grantable model with its grant mark, then granted models that are gone.
/// 媒体选项：每个可授权模型及其授权标记，其后为已不可用的已授权模型。
fn media_choices(snapshot: &Value, client: &Value) -> Vec<(String, String)> {
    let granted = a(client, "/mediaTargets");
    let has = |entry: &Value| {
        granted.iter().any(|target| {
            s(target, "/connectionId") == s(entry, "/connectionId")
                && s(target, "/targetId") == s(entry, "/targetId")
        })
    };
    let key = |entry: &Value| json!([s(entry, "/connectionId"), s(entry, "/targetId")]).to_string();
    let candidates = a(snapshot, "/mediaCandidates");
    let mut options: Vec<(String, String)> = candidates
        .iter()
        .map(|candidate| {
            let label = format!(
                "{} · {} · {}",
                s(candidate, "/displayName"),
                s(candidate, "/capability"),
                s(candidate, "/accountName")
            );
            (key(candidate), marked(has(candidate), label))
        })
        .collect();
    for target in granted {
        let listed = candidates.iter().any(|candidate| {
            s(candidate, "/connectionId") == s(target, "/connectionId")
                && s(candidate, "/targetId") == s(target, "/targetId")
        });
        if !listed {
            let label = format!(
                "{} · {}",
                s(target, "/targetId"),
                tr!("mcpService.field.mediaStale")
            );
            options.push((key(target), marked(true, label)));
        }
    }
    options
}

/// The short grant summary shown under a client's name, as the WebUI list shows it.
/// 显示在客户端名称下的简短授权摘要，与 WebUI 列表一致。
fn summary(client: &Value) -> String {
    let mut parts = Vec::new();
    if b(client, "/sessions/read") {
        parts.push(tr!("mcpService.summary.read"));
        for flag in SESSION_FLAGS {
            if b(client, &format!("/sessions/{flag}")) {
                parts.push(flag_label(flag));
            }
        }
        parts.push(if s(client, "/sessions/projects/mode") == "all" {
            tr!("mcpService.summary.allProjects")
        } else {
            tr!(
                "mcpService.summary.projects",
                count = a(client, "/sessions/projects/projectIds").len()
            )
        });
    } else {
        parts.push(tr!("mcpService.summary.noSessions"));
    }
    let media = a(client, "/mediaTargets").len();
    if media > 0 {
        parts.push(tr!("mcpService.summary.media", count = media));
    }
    parts.join(" · ")
}

/// One client item: its usage lines, switches for the client and each session permission, then
/// the project, media, rename and delete actions.
/// 一个客户端条目：使用情况行、客户端与各会话权限开关，其后是项目、媒体、重命名与删除操作。
fn client_row(app: &App, snapshot: &Value, client: &Value) -> Row {
    let id = s(client, "/id");
    let name = s(client, "/name");
    let read = b(client, "/sessions/read");
    let last_used = match v(client, "/lastUsedAtMs").as_i64() {
        Some(at) => tr!(
            "mcpService.client.lastUsed",
            time = super::settings::utc_time(at),
            count = n(client, "/callCount")
        ),
        None => tr!("mcpService.client.neverUsed"),
    };
    let flag_switch = |flag: &str| {
        let on = b(client, &format!("/sessions/{flag}"));
        switch(
            flag_label(flag),
            on,
            Some(call(updated(client, |next| {
                set_session_flag(&mut next["sessions"], flag, !on)
            }))),
        )
    };
    let mut actions = vec![
        switch(
            tr!("mcpService.field.enabled"),
            b(client, "/enabled"),
            Some(call(updated(client, |next| {
                next["enabled"] = json!(!b(client, "/enabled"))
            }))),
        ),
        flag_switch("read"),
    ];
    actions.extend(SESSION_FLAGS.map(flag_switch));
    actions.push(if read {
        action(
            tr!("mcpService.field.projects"),
            UiAction::Pick {
                title: tr!("mcpService.field.projects"),
                edit: Edit::Call(Setter::Build {
                    ctx: client.clone(),
                    build: pick_project,
                }),
                options: project_choices(app, client),
                current: String::new(),
            },
            Tone::Normal,
        )
    } else {
        absent(tr!("mcpService.field.projects"))
    });
    let media = media_choices(snapshot, client);
    actions.push(if media.is_empty() {
        absent(tr!("mcpService.field.media"))
    } else {
        action(
            tr!("mcpService.field.media"),
            UiAction::Pick {
                title: tr!("mcpService.field.media"),
                edit: Edit::Call(Setter::Build {
                    ctx: client.clone(),
                    build: pick_media,
                }),
                options: media,
                current: String::new(),
            },
            Tone::Normal,
        )
    });
    actions.push(action(
        tr!("mcpService.client.edit"),
        super::super::action::prompt(
            tr!("mcpService.field.name"),
            tr!(
                "mcpService.field.nameTooLong",
                max = n(snapshot, "/limits/maxNameChars")
            ),
            name.clone(),
            PromptThen::Edit(Edit::Call(Setter::Build {
                ctx: client.clone(),
                build: rename,
            })),
        ),
        Tone::Accent,
    ));
    actions.push(action(
        tr!("mcpService.client.delete"),
        confirm(
            tr!("mcpService.delete.title", name = name.clone()),
            tr!("mcpService.delete.detail", name = name.clone()),
            tr!("mcpService.delete.confirm"),
            call(api_mcp::delete_mcp_client(&id)),
        ),
        Tone::Bad,
    ));
    let mut row = item(&id, name, summary(client))
        .lines(vec![
            tr!("mcpService.client.token", hint = s(client, "/tokenHint")),
            last_used,
        ])
        .actions(actions);
    if !b(client, "/enabled") {
        row = row.badge(tr!("mcpService.client.disabled"), Tone::Warn);
    }
    row
}

/// Builds the MCP service page.
/// 构建 MCP 服务页。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let snapshot = app.slots.value(Slot::McpService);
    let mut rows = vec![
        section(tr!("mcpService.endpoint.title")),
        note(tr!("mcpService.endpoint.detail"), Tone::Muted),
    ];
    if snapshot.is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return rows;
    }
    let enabled = b(snapshot, "/enabled");
    rows.push(
        toggle(
            tr!("mcpService.endpoint.enable"),
            enabled,
            Edit::Call(Setter::Build {
                ctx: Value::Null,
                build: |_, value| Some(api_mcp::set_mcp_service_enabled(value.as_bool())),
            }),
        )
        .detail(tr!("mcpService.endpoint.enableDetail")),
    );
    rows.push(match v(snapshot, "/endpointUrl").as_str() {
        Some(url) => info(tr!("mcpService.endpoint.url"), url, Tone::Accent)
            .detail(tr!("mcpService.endpoint.urlDetail")),
        None => info(
            tr!("mcpService.endpoint.url"),
            tr!("mcpService.endpoint.unavailable"),
            Tone::Warn,
        ),
    });
    rows.push(note(tr!("mcpService.guide.approvalsDetail"), Tone::Muted));
    rows.push(note(tr!("mcpService.guide.viewDetail"), Tone::Muted));

    let clients = a(snapshot, "/clients");
    rows.push(section(tr!("mcpService.clients.title")));
    rows.push(info(
        tr!("mcpService.clients.title"),
        tr!(
            "mcpService.clients.count",
            count = clients.len(),
            max = n(snapshot, "/limits/maxClients")
        ),
        Tone::Normal,
    ));
    rows.push(note(tr!("tui.mcpService.webuiOnly"), Tone::Info));
    if clients.is_empty() {
        rows.push(note(tr!("mcpService.clients.emptyDetail"), Tone::Muted));
    }
    for client in clients {
        rows.push(client_row(app, snapshot, client));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Value {
        json!({
            "id": "c1",
            "name": "Laptop",
            "enabled": true,
            "tokenHint": "wxyz",
            "createdAtMs": 1,
            "callCount": 0,
            "sessions": {
                "read": true, "send": true, "create": false, "stop": false, "answerQuestions": false,
                "projects": { "mode": "selected", "projectIds": ["p1"] }
            },
            "mediaTargets": [{ "connectionId": "conn", "targetId": "img" }]
        })
    }

    // Updates send the complete client document with read-dependency rules applied.
    // 更新发送完整客户端文档，并应用读取依赖规则。
    #[test]
    fn session_flags_keep_read_dependencies() {
        let off = updated(&client(), |next| {
            set_session_flag(&mut next["sessions"], "read", false)
        });
        assert_eq!(off.method, "service_update_mcp_client");
        let sessions = &off.params["request"]["sessions"];
        assert_eq!(sessions["read"], false);
        assert_eq!(sessions["send"], false);
        assert_eq!(off.params["request"]["clientId"], "c1");
        assert_eq!(off.params["request"]["mediaTargets"][0]["targetId"], "img");

        let mut none = client();
        set_session_flag(&mut none["sessions"], "read", false);
        let on = updated(&none, |next| {
            set_session_flag(&mut next["sessions"], "stop", true)
        });
        assert_eq!(on.params["request"]["sessions"]["read"], true);
        assert_eq!(on.params["request"]["sessions"]["stop"], true);
    }

    // Project picks add, remove and widen only on the explicit all-projects choice.
    // 项目选择可加入、移除，只有显式选择全部项目时才扩大范围。
    #[test]
    fn project_picks_never_widen_implicitly() {
        let added = pick_project(&client(), &SettingValue::Choice("p2".into())).unwrap();
        assert_eq!(
            added.params["request"]["sessions"]["projects"]["projectIds"],
            json!(["p1", "p2"])
        );
        assert!(pick_project(&client(), &SettingValue::Choice("p1".into())).is_none());
        let all = pick_project(&client(), &SettingValue::Choice(ALL_PROJECTS.into())).unwrap();
        assert_eq!(
            all.params["request"]["sessions"]["projects"],
            json!({ "mode": "all" })
        );
    }

    #[test]
    fn media_picks_toggle_one_grant() {
        let removed =
            pick_media(&client(), &SettingValue::Choice(r#"["conn","img"]"#.into())).unwrap();
        assert_eq!(removed.params["request"]["mediaTargets"], json!([]));
        let added =
            pick_media(&client(), &SettingValue::Choice(r#"["conn","vid"]"#.into())).unwrap();
        assert_eq!(
            added.params["request"]["mediaTargets"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
    }
}
