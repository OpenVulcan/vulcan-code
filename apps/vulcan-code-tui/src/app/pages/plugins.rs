// LuaSkills and declarative plugin management pages.
// LuaSkills 与声明式插件管理页面。

use super::*;
use crate::service_api::skills_actions as skills;

/// Builds the GitHub install request accepted by LuaSkillsPanel; no unsupported URL mode.
/// 构建 LuaSkillsPanel 接受的 GitHub 安装请求，不提供不支持的 URL 模式。
fn install_lua(request: Value) -> ServiceCall {
    skills::install_luaskill(&s(&request, "/skillId"), &s(&request, "/source"), "github")
}

/// Builds the first-page LuaSkills Hub search mirrored from the WebUI install dialog.
/// 构建镜像 WebUI 安装弹窗的 LuaSkills Hub 首页搜索。
fn search_hub(request: Value) -> ServiceCall {
    skills::search_luaskills_hub(&s(&request, "/query"), None, crate::i18n::active_locale())
}

/// Shows the loaded Hub skill detail: language note, AI overview and README, in the translation
/// unless the user switched this skill to the original.
/// 显示已载入的 Hub 技能详情：语言说明、AI 概览与 README；除非用户将该技能切换为原文，否则显示译文。
fn hub_detail(app: &App, rows: &mut Vec<Row>) {
    if !app.slots.loaded(Slot::LuaSkillsHubSkill) {
        return;
    }
    // Detail.
    // 当前 Hub 技能详情。
    let detail = app.slots.value(Slot::LuaSkillsHubSkill);
    let skill = v(detail, "/skill");
    let id = s(skill, "/skillId");
    let content = v(skill, "/content");
    let kind = s(content, "/kind");
    let original = app.hub_original_skill.as_deref() == Some(id.as_str());
    rows.push(section(s(skill, "/displayName")));
    match kind.as_str() {
        "translation" => {
            // Native name of the source language, as the WebUI drawer shows it.
            // 源语言的本地名称，与 WebUI 抽屉显示一致。
            let source = s(content, "/sourceLocale");
            let language = crate::i18n::LOCALES
                .iter()
                .find(|locale| locale.code == source)
                .map_or_else(
                    || tr!("skills.lua.hubLanguageOther"),
                    |locale| locale.native_name.to_string(),
                );
            let toggle = if original {
                action(
                    tr!("skills.lua.hubShowTranslation"),
                    UiAction::HubOriginal(None),
                    Tone::Normal,
                )
            } else {
                action(
                    tr!("skills.lua.hubShowOriginal"),
                    UiAction::HubOriginal(Some(id.clone())),
                    Tone::Normal,
                )
            };
            rows.push(
                item(
                    "hub-detail-language",
                    tr!("skills.lua.hubMachineTranslated", language = language),
                    "",
                )
                .actions(vec![toggle]),
            );
        }
        "original_fallback" => rows.push(note(tr!("skills.lua.hubNoTranslation"), Tone::Muted)),
        _ => {}
    }
    // Shown content: the original node when toggled, else the requested one.
    // 显示的内容：切换时为原文节点，否则为请求的节点。
    let shown = if original && v(detail, "/original").is_object() {
        v(detail, "/original")
    } else {
        content
    };
    if shown.is_object() {
        let mut overview = vec![s(shown, "/intro")];
        overview.extend(
            a(shown, "/features")
                .iter()
                .map(|feature| format!("· {}", feature.as_str().unwrap_or_default())),
        );
        rows.push(
            item("hub-detail-overview", tr!("skills.lua.hubAiOverview"), "").lines(
                overview
                    .into_iter()
                    .filter(|line| !line.is_empty())
                    .collect(),
            ),
        );
    }
    // Only a translation node carries its own README; everything else shows the published one.
    // 只有译文节点自带 README；其余情况显示已发布的 README。
    let translated = s(shown, "/readme");
    let readme = if translated.is_empty() {
        s(detail, "/latest/readme")
    } else {
        translated
    };
    rows.push(section(tr!("skills.lua.hubReadme")));
    rows.push(Row::new(RowKind::Markdown { content: readme }, ""));
}

/// Lists the loaded Hub page; each skill installs through the `official_hub` source.
/// 列出已载入的 Hub 页面；每个技能通过 `official_hub` 来源安装。
fn hub_results(app: &App, rows: &mut Vec<Row>) {
    if !app.slots.loaded(Slot::LuaSkillsHub) {
        return;
    }
    // Page.
    // 当前 Hub 结果页。
    let page = app.slots.value(Slot::LuaSkillsHub);
    rows.push(info(
        tr!("tui.field.hubResults"),
        s(page, "/hubBaseUrl"),
        Tone::Muted,
    ));
    for skill in a(page, "/items") {
        // Exact Hub skill identity.
        // 准确的 Hub 技能身份。
        let id = s(skill, "/skillId");
        let mut lines = vec![
            [
                s(skill, "/skillId"),
                s(skill, "/publisher"),
                s(skill, "/license"),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
        ];
        // The localized summary when the Hub returned one, else the published summary.
        // Hub 返回本地化摘要时使用它，否则使用已发布的摘要。
        let summary = [s(skill, "/content/summary"), s(skill, "/summary")]
            .into_iter()
            .find(|summary| !summary.is_empty());
        lines.extend(summary);
        let mut row = item(
            &format!("hub:{id}"),
            s(skill, "/displayName"),
            s(skill, "/latestVersion"),
        )
        .lines(lines)
        .actions(vec![
            action(
                tr!("tui.field.install"),
                call(skills::install_luaskill("", &id, "official_hub")),
                Tone::Accent,
            ),
            action(
                tr!("skills.lua.hubDetails"),
                call(skills::get_luaskills_hub_skill(
                    &id,
                    crate::i18n::active_locale(),
                )),
                Tone::Normal,
            ),
        ]);
        if s(skill, "/content/kind") == "translation" {
            row = row.badge(tr!("skills.lua.hubTranslatedBadge"), Tone::Muted);
        }
        rows.push(row);
    }
    if a(page, "/items").is_empty() {
        rows.push(info(tr!("tui.field.hubEmpty"), String::new(), Tone::Muted));
    }
    // The next page reuses the exact query of the request that produced this page.
    // 下一页沿用产生本页的请求的准确查询。
    let cursor = s(page, "/nextCursor");
    if let Some(previous) = app.slots.loader(Slot::LuaSkillsHub)
        && !cursor.is_empty()
    {
        rows.push(button(
            tr!("tui.field.hubNextPage"),
            call(skills::search_luaskills_hub(
                &s(&previous.params, "/query"),
                Some(&cursor),
                crate::i18n::active_locale(),
            )),
            Tone::Normal,
        ));
    }
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
        button(
            tr!("tui.field.hubSearch"),
            form(
                json!({"query":""}),
                vec![field(tr!("tui.field.hubQuery"), "/query", false, true)],
                search_hub,
            ),
            Tone::Normal,
        ),
    ];
    hub_results(app, &mut rows);
    hub_detail(app, &mut rows);
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
