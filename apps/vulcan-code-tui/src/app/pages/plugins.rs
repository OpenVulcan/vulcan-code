// LuaSkills and declarative plugin management pages.
// LuaSkills 与声明式插件管理页面。

use super::*;
use crate::service_api::skills_actions as skills;

/// Builds the GitHub install request accepted by LuaSkillsPanel; no unsupported URL mode.
/// 构建 LuaSkillsPanel 接受的 GitHub 安装请求，不提供不支持的 URL 模式。
fn install_lua(request: Value) -> ServiceCall {
    skills::install_luaskill(&s(&request, "/skillId"), &s(&request, "/source"), "github")
}

/// Lists real package ownership, lifecycle actions and per-tool registration modes.
/// 显示真实包归属、生命周期动作与逐工具注册模式。
pub(super) fn lua_skills(app: &App) -> Vec<Row> {
    // Catalog.
    // 服务发布的目录。
    let catalog = v(app.slots.value(Slot::Workbench), "/luaSkills");
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![
        button(
            tr!("tui.field.refresh"),
            call(skills::refresh_luaskills_catalog()),
            Tone::Normal,
        ),
        button(
            tr!("tui.field.install"),
            form(
                json!({"skillId":"","source":""}),
                vec![
                    field(tr!("tui.field.skillId"), "/skillId", false, true),
                    field(tr!("tui.field.githubSource"), "/source", false, false),
                ],
                install_lua,
            ),
            Tone::Accent,
        ),
    ];
    for progress in ["/operationProgress", "/managedRuntimeProgress"] {
        if v(catalog, progress).is_object() {
            rows.push(
                item(progress, tr!("tui.field.progress"), "").lines(details(v(catalog, progress))),
            );
        }
    }
    for skill in a(catalog, "/skills") {
        // Exact entity identity.
        // 准确实体身份。
        let id = s(skill, "/skillId");
        rows.push(toggle(
            s(skill, "/displayName"),
            b(skill, "/enabled"),
            dto(skill.clone(), "/enabled", |request| {
                skills::set_luaskill_enabled(&s(&request, "/skillId"), b(&request, "/enabled"))
            }),
        ));
        // Actions.
        // 声明的操作集合。
        let mut actions = Vec::new();
        if !b(skill, "/systemOwned") {
            actions.push(action(
                tr!("tui.field.update"),
                call(skills::update_luaskill(&id)),
                Tone::Normal,
            ));
            actions.push(action(
                tr!("tui.field.remove"),
                confirm(
                    tr!("tui.field.remove"),
                    s(skill, "/displayName"),
                    tr!("tui.field.remove"),
                    call(skills::uninstall_luaskill(&id)),
                ),
                Tone::Bad,
            ));
        }
        rows.push(
            item(&id, s(skill, "/displayName"), s(skill, "/version"))
                .lines(details(skill))
                .actions(actions),
        );
        for tool in a(skill, "/tools") {
            rows.push(choice(
                s(tool, "/localName"),
                vec![
                    ("explicit".into(), tr!("tui.field.explicitTool")),
                    ("implicit".into(), tr!("tui.field.implicitTool")),
                    ("disabled".into(), tr!("tui.field.disabledTool")),
                ],
                s(tool, "/registrationState"),
                Edit::Call(Setter::Build {
                    ctx: json!({"skillId":id,"localName":v(tool,"/localName")}),
                    build: |context, value| {
                        Some(skills::set_luaskill_tool_registration_state(
                            &s(context, "/skillId"),
                            &s(context, "/localName"),
                            value.as_str(),
                        ))
                    },
                }),
            ));
        }
    }
    rows
}

/// Saves one verified runtime field within the full authoritative settings document.
/// 在完整权威设置文档内保存一个已确认的运行时字段。
pub(super) fn lua_settings(app: &App) -> Vec<Row> {
    // Catalog.
    // 服务发布的目录。
    let catalog = v(app.slots.value(Slot::Workbench), "/luaSkills");
    // Settings.
    // 权威设置文档。
    let settings = v(catalog, "/runtimeSettings");
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![info(
        tr!("tui.field.runtimeRoot"),
        s(catalog, "/runtimeRoot"),
        Tone::Muted,
    )];
    if !settings.is_object() {
        return rows;
    }
    for (pointer, label) in [
        ("/vmPool/minSize", tr!("tui.field.vmMin")),
        ("/vmPool/maxSize", tr!("tui.field.vmMax")),
        ("/vmPool/idleTtlSecs", tr!("tui.field.vmIdle")),
        ("/runLuaPool/minSize", tr!("tui.field.runMin")),
        ("/runLuaPool/maxSize", tr!("tui.field.runMax")),
        ("/runLuaPool/idleTtlSecs", tr!("tui.field.runIdle")),
        ("/toolCache/maxEntries", tr!("tui.field.cacheEntries")),
        (
            "/toolCache/defaultTtlSecs",
            tr!("tui.field.cacheDefaultTtl"),
        ),
        ("/toolCache/maxTtlSecs", tr!("tui.field.cacheMaxTtl")),
        (
            "/managedRuntime/workerPoolMaxSizePerEnvironment",
            tr!("tui.field.workerLimit"),
        ),
        (
            "/managedRuntime/workerIdleTtlSecs",
            tr!("tui.field.workerIdle"),
        ),
        (
            "/managedRuntime/persistentSessionLimitPerEngine",
            tr!("tui.field.sessionLimit"),
        ),
        (
            "/managedRuntime/persistentSessionDefaultBufferLimitBytesPerStream",
            tr!("tui.field.bufferLimit"),
        ),
    ] {
        if v(settings, pointer).is_number() {
            rows.push(number(
                label,
                n(settings, pointer),
                (0, i32::MAX.into(), 1),
                dto(
                    settings.clone(),
                    pointer,
                    skills::save_luaskills_runtime_settings,
                ),
            ));
        }
    }
    for (pointer, label) in [
        ("/network/allowDownloads", tr!("tui.field.downloads")),
        (
            "/network/enablePrivateUrlSkillInstall",
            tr!("tui.field.privateSources"),
        ),
        (
            "/capabilities/enableSkillManagementBridge",
            tr!("tui.field.managementBridge"),
        ),
        (
            "/capabilities/enableManagedIoCompat",
            tr!("tui.field.ioCompat"),
        ),
        ("/aiTools/enableRunLua", tr!("tui.field.runLua")),
        (
            "/aiTools/enableRunLuaSession",
            tr!("tui.field.runLuaSession"),
        ),
    ] {
        rows.push(toggle(
            label,
            b(settings, pointer),
            dto(
                settings.clone(),
                pointer,
                skills::save_luaskills_runtime_settings,
            ),
        ));
    }
    for (pointer, label) in [
        ("/network/githubBaseUrl", tr!("tui.field.githubEndpoint")),
        ("/network/githubApiBaseUrl", tr!("tui.field.githubApi")),
        (
            "/network/officialSkillHubBaseUrl",
            tr!("tui.field.skillHub"),
        ),
        (
            "/capabilities/defaultTextEncoding",
            tr!("tui.field.encoding"),
        ),
    ] {
        if v(settings, pointer).is_string() {
            rows.push(text(
                label,
                s(settings, pointer),
                dto(
                    settings.clone(),
                    pointer,
                    skills::save_luaskills_runtime_settings,
                ),
            ));
        }
    }
    for target in ["python", "node", "all"] {
        rows.push(button(
            format!("{} · {target}", tr!("tui.field.retryRuntime")),
            call(skills::retry_luaskills_managed_runtimes(target)),
            Tone::Normal,
        ));
    }
    rows.push(string_list(
        tr!("tui.field.privateAllowlist"),
        settings,
        "/network/privateSkillSourceAllowlist",
        skills::save_luaskills_runtime_settings,
    ));
    rows.push(item("runtime-settings", tr!("tui.field.details"), "").lines(details(settings)));
    rows
}

/// Lists host-managed plugins and mount metadata without rendering Lua plugin interfaces.
/// 列出宿主管理的插件与挂载元数据，不渲染 Lua 插件界面。
pub(super) fn plugins(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![button(
        tr!("tui.field.installPlugin"),
        UiAction::InstallPlugin,
        Tone::Accent,
    )];
    for plugin in a(app.slots.value(Slot::Workbench), "/plugins") {
        // Exact entity identity.
        // 准确实体身份。
        let id = s(plugin, "/id");
        rows.push(toggle(
            s(plugin, "/name"),
            b(plugin, "/desiredEnabled"),
            dto(plugin.clone(), "/desiredEnabled", |request| {
                skills::set_vulcan_plugin_enabled(
                    &s(&request, "/id"),
                    b(&request, "/desiredEnabled"),
                )
            }),
        ));
        // Actions.
        // 声明的操作集合。
        let mut actions = super::plugin_settings::open_actions(app, &id);
        if s(plugin, "/extension/origin") == "userInstalled" {
            actions.push(action(
                tr!("tui.field.remove"),
                confirm(
                    tr!("tui.field.remove"),
                    s(plugin, "/name"),
                    tr!("tui.field.remove"),
                    call(skills::uninstall_user_plugin(&id)),
                ),
                Tone::Bad,
            ));
        }
        rows.push(
            item(&id, s(plugin, "/name"), s(plugin, "/runtimeState"))
                .lines(details(plugin))
                .actions(actions),
        );
    }
    rows
}
