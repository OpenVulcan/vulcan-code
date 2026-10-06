// LuaSkills, declarative plugin management and semantic index pages.
// LuaSkills、声明式插件管理与语义索引页面。

use super::*;
use crate::service_api::{skills_actions as skills, vce};

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

/// Builds a complete VceProjectSettingsInput while keeping overview-only metadata out.
/// 构建完整 VceProjectSettingsInput，同时排除仅属于概览的元数据。
fn save_index_field(context: &Value, value: &SettingValue) -> Option<ServiceCall> {
    // Settings.
    // 权威设置文档。
    let mut settings = context.get("settings")?.clone();
    // Pointer.
    // 已确认 JSON 指针。
    let pointer = context.get("pointer")?.as_str()?;
    *settings.pointer_mut(pointer)? = match value {
        SettingValue::Bool(value) => json!(value),
        SettingValue::Number(value) => json!(value),
        SettingValue::Text(value) | SettingValue::Choice(value) => json!(value),
    };
    Some(vce::update_vce_project_settings(
        context.get("projectId")?.as_str()?,
        context.get("revision")?.as_i64()?,
        settings,
    ))
}

/// Selects only input fields defined in the WebUI VCE type; metadata cannot reach save RPC.
/// 仅选择 WebUI VCE 类型定义的输入字段，元数据无法进入保存 RPC。
fn index_editor(app: &App, overview: &Value, pointer: &str) -> Option<Edit> {
    // Source.
    // 当前来源。
    let source = overview.get("settings")?.as_object()?;
    // Settings.
    // 权威设置文档。
    let mut settings = serde_json::Map::new();
    for key in [
        "enabled",
        "updateMode",
        "autoUpdateMinIntervalSeconds",
        "maxFileBytes",
        "extraIgnorePatterns",
        "allowQueryRewrite",
        "allowRerank",
        "embeddingMaxConcurrency",
        "queryMaxConcurrency",
        "maxResults",
        "maxContextChars",
    ] {
        settings.insert(key.into(), source.get(key)?.clone());
    }
    Some(Edit::Call(Setter::Build {
        ctx: json!({
        "projectId":app.vce_project.as_ref()?,"revision":source.get("revision")?,
        "settings":settings,"pointer":pointer}),
        build: save_index_field,
    }))
}

/// Displays project status, version-guarded settings, jobs and two-step maintenance plans.
/// 显示项目状态、版本保护的设置、任务与两步维护计划。
pub(super) fn index(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![choice(
        tr!("tui.field.project"),
        app.projects
            .iter()
            .map(|project| (project.id.clone(), project.name.clone()))
            .collect(),
        app.vce_project.clone().unwrap_or_default(),
        Edit::Local(LocalSetting::VceProject),
    )];
    // Status.
    // 已声明生命周期状态。
    let status = app.slots.value(Slot::VceStatus);
    rows.push(item("vce-status", tr!("tui.field.status"), "").lines(details(status)));
    // Project id.
    // 准确宿主项目身份。
    let Some(project_id) = app.vce_project.as_deref() else {
        return rows;
    };
    // Overview.
    // 所选项目权威概览。
    let overview = app.slots.value(Slot::VceOverview);
    if !overview.is_object() {
        return rows;
    }
    rows.push(
        item(
            "vce-overview",
            tr!("tui.field.overview"),
            s(overview, "/status/lifecycle"),
        )
        .lines(details(overview)),
    );
    for (pointer, label) in [
        ("/enabled", tr!("tui.field.enabled")),
        ("/allowQueryRewrite", tr!("tui.field.queryRewrite")),
        ("/allowRerank", tr!("tui.field.rerank")),
    ] {
        if let Some(edit) = index_editor(app, overview, pointer) {
            rows.push(toggle(label, b(v(overview, "/settings"), pointer), edit));
        }
    }
    if let Some(edit) = index_editor(app, overview, "/updateMode") {
        rows.push(choice(
            tr!("tui.field.updateMode"),
            vec![
                ("manual".into(), tr!("tui.field.manual")),
                ("automatic".into(), tr!("tui.field.automatic")),
            ],
            s(overview, "/settings/updateMode"),
            edit,
        ));
    }
    for (pointer, label, min) in [
        (
            "/autoUpdateMinIntervalSeconds",
            tr!("tui.field.autoInterval"),
            1,
        ),
        ("/maxFileBytes", tr!("tui.field.maxFileBytes"), 1),
        (
            "/embeddingMaxConcurrency",
            tr!("tui.field.embeddingConcurrency"),
            1,
        ),
        ("/queryMaxConcurrency", tr!("tui.field.queryConcurrency"), 1),
        ("/maxResults", tr!("tui.field.maxResults"), 1),
        ("/maxContextChars", tr!("tui.field.maxContextChars"), 1),
    ] {
        if let Some(edit) = index_editor(app, overview, pointer) {
            rows.push(number(
                label,
                n(v(overview, "/settings"), pointer),
                (min, i32::MAX.into(), 1),
                edit,
            ));
        }
    }
    if let Some(revision) = v(overview, "/settings/revision").as_i64() {
        // The upstream status type permits an absent checkpoint before initialization.
        // 上游状态类型允许初始化前缺少检查点。
        {
            // Checkpoint.
            // 已观察检查点版本。
            let checkpoint = v(overview, "/status/checkpointVersion");
            for (name, label) in [
                ("initialize", tr!("tui.field.initialize")),
                ("synchronize", tr!("tui.field.synchronize")),
            ] {
                rows.push(button(
                    label,
                    call(vce::submit_vce_job(
                        json!({"projectId":project_id,"action":name,
                    "idempotencyKey":api::fresh_command_id(),"expectedSettingsRevision":revision,
                    "expectedCheckpointVersion":checkpoint}),
                    )),
                    Tone::Accent,
                ));
            }
        }
        for (mode, label) in [
            ("repair", tr!("tui.field.repair")),
            ("rebuild", tr!("tui.field.rebuild")),
            ("cleanup_staging", tr!("tui.field.cleanupStaging")),
            ("compact", tr!("tui.field.compact")),
        ] {
            rows.push(button(
                label,
                call(vce::plan_vce_maintenance(project_id, mode, revision)),
                Tone::Warn,
            ));
        }
    }
    if let Some(Edit::Call(Setter::Build { ctx, .. })) =
        index_editor(app, overview, "/extraIgnorePatterns")
    {
        rows.push(text(
            tr!("tui.field.extraIgnores"),
            a(overview, "/settings/extraIgnorePatterns")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n"),
            Edit::Call(Setter::Build {
                ctx,
                build: |ctx, value| {
                    // Settings.
                    // 权威设置文档。
                    let mut settings = ctx.get("settings")?.clone();
                    settings["extraIgnorePatterns"] = parse_lines(value.as_str())?;
                    Some(vce::update_vce_project_settings(
                        ctx.get("projectId")?.as_str()?,
                        ctx.get("revision")?.as_i64()?,
                        settings,
                    ))
                },
            }),
        ));
    }
    // Job.
    // 持久任务回执。
    let job = app.slots.value(Slot::VceJob);
    if let Some(id) = v(job, "/jobId").as_str() {
        // Actions.
        // 声明的操作集合。
        let mut actions = vec![action(
            tr!("tui.field.refresh"),
            call(vce::load_vce_job(project_id, None)),
            Tone::Normal,
        )];
        if matches!(
            v(job, "/state").as_str(),
            Some("queued" | "running" | "cancelling")
        ) {
            actions.push(action(
                tr!("tui.field.cancel"),
                call(vce::cancel_vce_job(project_id, id)),
                Tone::Warn,
            ));
        }
        rows.push(
            item(id, tr!("tui.field.job"), s(job, "/state"))
                .lines(details(job))
                .actions(actions),
        );
    }
    rows
}

/// Presents the service's global VCE configuration and compatible inference routes.
/// 显示服务的全局 VCE 配置与兼容推理路由。
pub(super) fn engine(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![
        item("configuration", tr!("tui.field.status"), "")
            .lines(details(app.slots.value(Slot::VceStatus))),
    ];
    rows.extend(super::inference::rows(app, "vce"));
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
