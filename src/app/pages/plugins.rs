// Plugins area: LuaSkills packages and settings, the semantic code engine, and Vulcan plugins.
// 插件区域：LuaSkills 技能包与设置、语义代码引擎，以及 Vulcan 插件。
//
// Plugin UI mounts are listed but never rendered here; the TUI only drives their core capabilities.
// 插件 UI 挂载点仅列出、不在此渲染；TUI 只驱动其核心能力。

use super::*;
use crate::ui::text::compact_count;

fn tool_mode_label(mode: ToolMode) -> (String, Tone) {
    match mode {
        ToolMode::Explicit => (tr!("skills.lua.toolExplicit"), Tone::Accent),
        ToolMode::Implicit => (tr!("skills.lua.toolImplicit"), Tone::Normal),
        ToolMode::Disabled => (tr!("skills.lua.toolDisabled"), Tone::Muted),
    }
}

pub(super) fn lua_skills(system: &SystemSnapshot) -> Vec<Row> {
    let skills = &system.lua_skills;
    let updates = skills.iter().filter(|s| s.update.is_some()).count();
    let mut rows = vec![stats(vec![
        (
            tr!("tui.lua.packages"),
            skills.len().to_string(),
            Tone::Normal,
        ),
        (
            tr!("agentSkills.enabled"),
            skills.iter().filter(|s| s.enabled).count().to_string(),
            Tone::Good,
        ),
        (
            tr!("skills.plugins.tools"),
            skills
                .iter()
                .map(|s| s.tools.len())
                .sum::<usize>()
                .to_string(),
            Tone::Normal,
        ),
        (
            tr!("tui.lua.updates"),
            updates.to_string(),
            if updates > 0 {
                Tone::Warn
            } else {
                Tone::Normal
            },
        ),
    ])];
    if skills.is_empty() {
        rows.push(note(tr!("skills.lua.emptyTitle"), Tone::Muted));
        rows.push(note(tr!("skills.lua.emptyDetail"), Tone::Muted));
    }
    for layer in ["system", "user"] {
        let list: Vec<&LuaSkill> = skills.iter().filter(|s| s.layer == layer).collect();
        if list.is_empty() {
            continue;
        }
        let title = if layer == "system" {
            tr!("agentSkills.system")
        } else {
            tr!("agentSkills.user")
        };
        rows.push(section(format!("{title} · {}", list.len())));
        rows.extend(list.into_iter().map(lua_row));
    }
    rows.push(button(
        format!("+ {}", tr!("skills.lua.installTitle")),
        prompt(
            tr!("skills.lua.installTitle"),
            format!(
                "{}\n{} · {}",
                tr!("skills.lua.installDescription"),
                tr!("skills.lua.placeholderGithub"),
                tr!("skills.lua.placeholderUrl")
            ),
            PromptThen::LuaInstall,
            false,
        ),
        Tone::Accent,
    ));
    rows
}

fn lua_row(skill: &LuaSkill) -> Row {
    let catalog = |op: CatalogOp| sys_action(SystemCommand::LuaSkill(op));
    let mut actions = vec![switch(
        tr!("agentSkills.enabled"),
        skill.enabled,
        Edit::Enabled(EntityKind::LuaSkill, skill.id.clone()),
    )];
    // One chip per tool; activating it cycles explicit → implicit → disabled.
    // 每个工具一个按钮；激活后按 显式 → 隐式 → 禁用 循环。
    for (tool, mode) in &skill.tools {
        let (label, tone) = tool_mode_label(*mode);
        actions.push(action(
            format!("{tool}: {label}"),
            UiAction::Edit(
                Edit::Field(
                    EntityKind::LuaSkill,
                    skill.id.clone(),
                    format!("tool:{tool}"),
                ),
                SettingValue::Choice(mode.next().id().to_string()),
            ),
            tone,
        ));
    }
    if skill.job.is_none() {
        if let Some(version) = &skill.update {
            actions.push(action(
                tr!("tui.lua.updateTo", version = version),
                catalog(CatalogOp::Update(skill.id.clone())),
                Tone::Accent,
            ));
        }
        actions.push(action(
            tr!("skills.lua.uninstallSkill"),
            confirm(
                tr!("skills.lua.uninstallSkill"),
                tr!("skills.lua.removeSkillPackage", displayName = skill.name),
                tr!("skills.plugins.uninstall"),
                catalog(CatalogOp::Delete(skill.id.clone())),
            ),
            Tone::Bad,
        ));
    }
    let mut lines = Vec::new();
    if !skill.description.is_empty() {
        lines.push(skill.description.clone());
    }
    lines.push(format!(
        "{}  {}",
        tr!("skills.lua.installSource"),
        skill.source
    ));
    if skill.tools.is_empty() {
        lines.push(tr!("skills.lua.toolsEmpty"));
    }
    let (badge, tone) = if skill.job.is_some() {
        (tr!("skills.lua.working"), Tone::Warn)
    } else if let Some(version) = &skill.update {
        (
            tr!("tui.lua.updateAvailable", version = version),
            Tone::Warn,
        )
    } else if skill.enabled {
        (tr!("agentSkills.enabled"), Tone::Good)
    } else {
        (tr!("agentSkills.disabled"), Tone::Muted)
    };
    item(
        &skill.id,
        skill.name.clone(),
        format!("v{} · {}", skill.version, skill.source),
    )
    .badge(badge, tone)
    .lines(lines)
    .actions(actions)
    .job(skill.job.clone())
}

pub(super) fn lua_settings(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let number_draft = |label: String, key: &str, range: (i64, i64, i64)| {
        number(label, count(app, key), range, draft(key))
    };
    let toggle_draft = |label: String, key: &str| toggle(label, flag(app, key), draft(key));
    let pool = |rows: &mut Vec<Row>, title: String, prefix: &str| {
        rows.push(section(title));
        rows.push(number_draft(
            tr!("skills.luaSettings.minSize"),
            &format!("{prefix}.min"),
            (0, 64, 1),
        ));
        rows.push(number_draft(
            tr!("skills.luaSettings.maxSize"),
            &format!("{prefix}.max"),
            (1, 64, 1),
        ));
        rows.push(number_draft(
            tr!("skills.luaSettings.idleTtl"),
            &format!("{prefix}.idleTtl"),
            (10, 86_400, 30),
        ));
    };
    let mut rows = vec![
        note(tr!("skills.luaSettings.description"), Tone::Muted),
        section(tr!("skills.luaSettings.aiTools")),
        toggle(
            tr!("skills.luaSettings.enableRunLua"),
            flag(app, "lua.runlua"),
            immediate("lua.runlua"),
        )
        .detail(tr!("skills.luaSettings.runLuaTooltip")),
        toggle(
            tr!("skills.luaSettings.enableRunLuaSession"),
            flag(app, "lua.runluaSession"),
            immediate("lua.runluaSession"),
        )
        .detail(tr!("skills.luaSettings.runLuaSessionTooltip")),
        note(tr!("skills.luaSettings.aiToolsHint"), Tone::Muted),
    ];
    pool(
        &mut rows,
        tr!("skills.luaSettings.mainPool"),
        "lua.mainPool",
    );
    pool(
        &mut rows,
        tr!("skills.luaSettings.runLuaPool"),
        "lua.runluaPool",
    );
    rows.push(note(tr!("skills.luaSettings.poolHint"), Tone::Muted));

    rows.push(section(tr!("skills.luaSettings.toolCache")));
    rows.push(number_draft(
        tr!("skills.luaSettings.maxEntries"),
        "lua.cache.maxEntries",
        (0, 100_000, 64),
    ));
    rows.push(number_draft(
        tr!("skills.luaSettings.defaultTtl"),
        "lua.cache.defaultTtl",
        (1, 86_400, 30),
    ));
    rows.push(number_draft(
        tr!("skills.luaSettings.maxTtl"),
        "lua.cache.maxTtl",
        (1, 604_800, 60),
    ));

    rows.push(section(tr!("skills.luaSettings.network")));
    rows.push(toggle_draft(
        tr!("skills.luaSettings.allowDownloads"),
        "lua.allowDownloads",
    ));
    for (label, key) in [
        (tr!("skills.luaSettings.githubBaseUrl"), "lua.githubBaseUrl"),
        (
            tr!("skills.luaSettings.githubApiBaseUrl"),
            "lua.githubApiBaseUrl",
        ),
        (
            tr!("skills.luaSettings.skillHubBaseUrl"),
            "lua.skillHubBaseUrl",
        ),
    ] {
        rows.push(text(label, string(app, key), draft(key)));
    }
    rows.push(toggle_draft(
        tr!("skills.luaSettings.privateUrlInstall"),
        "lua.privateUrlInstall",
    ));

    rows.push(section(tr!("skills.luaSettings.capabilities")));
    rows.push(choice(
        tr!("skills.luaSettings.defaultEncoding"),
        vec![
            (String::new(), tr!("skills.luaSettings.autoEncoding")),
            ("utf-8".into(), "UTF-8".into()),
            ("gbk".into(), "GBK".into()),
            ("big5".into(), "Big5".into()),
            ("shift_jis".into(), "Shift_JIS".into()),
        ],
        string(app, "lua.defaultEncoding"),
        draft("lua.defaultEncoding"),
    ));
    rows.push(toggle_draft(
        tr!("skills.luaSettings.skillManagementBridge"),
        "lua.skillManagementBridge",
    ));
    rows.push(toggle_draft(
        tr!("skills.luaSettings.managedIoCompat"),
        "lua.managedIoCompat",
    ));

    rows.push(section(tr!("skills.luaSettings.managedRuntime")));
    for runtime in &system.managed_runtimes {
        rows.push(info(
            runtime.label.clone(),
            runtime.detail.clone(),
            Tone::of(runtime.severity),
        ));
    }
    rows.push(number_draft(
        tr!("skills.luaSettings.managedWorkerPoolMax"),
        "lua.managed.workerPoolMax",
        (1, 32, 1),
    ));
    rows.push(number_draft(
        tr!("skills.luaSettings.managedInvokeTimeout"),
        "lua.managed.invokeTimeout",
        (1_000, 600_000, 1_000),
    ));
    rows.push(note(tr!("skills.luaSettings.restartHint"), Tone::Muted));

    // Cross-field rules are checked before saving, as the WebUI does.
    // 与 WebUI 一致，保存前校验跨字段规则。
    let below = |min: &str, max: &str| count(app, max) < count(app, min);
    let problem = if below("lua.mainPool.min", "lua.mainPool.max")
        || below("lua.runluaPool.min", "lua.runluaPool.max")
    {
        Some(tr!("skills.luaSettings.validationPoolOrder"))
    } else if below("lua.cache.defaultTtl", "lua.cache.maxTtl") {
        Some(tr!("skills.luaSettings.validationCacheOrder"))
    } else {
        None
    };
    draft_footer(app, PageId::LuaSettings, &mut rows, problem);
    rows
}

fn index_status(entry: &IndexedProject) -> (String, Tone) {
    match entry.status.as_str() {
        _ if entry.job.is_some() => (tr!("codeContext.project.jobInProgress"), Tone::Warn),
        "ready" if entry.pending > 0 => {
            (tr!("codeContext.project.changesAwaitingSync"), Tone::Warn)
        }
        "ready" => (tr!("codeContext.project.gate.ready"), Tone::Good),
        "indexing" => (tr!("codeContext.project.jobInProgress"), Tone::Warn),
        "rebuildRequired" => (tr!("codeContext.project.gate.rebuildRequired"), Tone::Bad),
        "modelsRequired" => (tr!("codeContext.project.gate.engineOff"), Tone::Warn),
        "disabled" => (tr!("codeContext.project.gate.projectOff"), Tone::Muted),
        other => (other.to_string(), Tone::Muted),
    }
}

pub(super) fn index(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let index = &system.index;
    let mut rows = vec![
        note(tr!("area.contextEngine.description"), Tone::Muted),
        stats(vec![
            (
                tr!("tui.index.projects"),
                index.len().to_string(),
                Tone::Normal,
            ),
            (
                tr!("codeContext.job.files"),
                compact_count(index.iter().map(|p| p.files as u64).sum()),
                Tone::Normal,
            ),
            (
                tr!("tui.knowledge.chunks"),
                compact_count(index.iter().map(|p| p.chunks as u64).sum()),
                Tone::Normal,
            ),
            (
                tr!("tui.index.attention"),
                index
                    .iter()
                    .filter(|p| p.status != "ready" || p.pending > 0)
                    .count()
                    .to_string(),
                Tone::Warn,
            ),
        ]),
    ];
    for entry in index {
        let id = entry.project_id.clone();
        let mut actions = vec![action(
            format!(
                "{}: {}",
                tr!("codeContext.project.indexUpdateMode"),
                if entry.watching {
                    tr!("codeContext.project.automatic")
                } else {
                    tr!("codeContext.project.manual")
                }
            ),
            UiAction::Edit(
                Edit::Enabled(EntityKind::IndexWatch, id.clone()),
                SettingValue::Bool(!entry.watching),
            ),
            if entry.watching {
                Tone::Good
            } else {
                Tone::Muted
            },
        )];
        if entry.job.is_none() {
            if entry.status == "rebuildRequired" {
                actions.push(action(
                    tr!("tui.index.rebuild"),
                    sys_action(SystemCommand::IndexRebuild(id.clone())),
                    Tone::Accent,
                ));
            } else {
                actions.push(action(
                    tr!("codeContext.job.synchronizeIndexNow"),
                    sys_action(SystemCommand::IndexRebuild(id.clone())),
                    Tone::Accent,
                ));
                actions.push(action(
                    tr!("tui.index.rebuild"),
                    confirm(
                        tr!("codeContext.job.rebuildingSemanticIndex"),
                        tr!("tui.index.rebuildBody", name = entry.name),
                        tr!("tui.index.rebuild"),
                        sys_action(SystemCommand::IndexRebuild(id.clone())),
                    ),
                    Tone::Normal,
                ));
            }
            if entry.fragments >= COMPACTION_FRAGMENTS {
                actions.push(action(
                    tr!("tui.index.compact"),
                    sys_action(SystemCommand::IndexCompact(id.clone())),
                    Tone::Warn,
                ));
            }
        }
        let path = app
            .projects
            .iter()
            .find(|p| p.id == entry.project_id)
            .map(|p| p.path.clone())
            .unwrap_or_default();
        let mut lines = Vec::new();
        if !path.is_empty() {
            lines.push(path);
        }
        lines.push(tr!(
            "tui.index.detail",
            chunks = compact_count(entry.chunks as u64),
            vectors = compact_count(entry.vectors as u64),
            checkpoint = entry.checkpoint
        ));
        lines.push(format!(
            "{}{}",
            tr!("codeContext.project.lastSuccessfulUpdate"),
            if entry.last_update.is_empty() {
                tr!("codeContext.project.noVerifiedRecord")
            } else {
                entry.last_update.clone()
            }
        ));
        if entry.pending > 0 {
            lines.push(tr!("tui.index.pending", count = entry.pending));
        }
        lines.push(tr!(
            "tui.index.fragments",
            count = entry.fragments,
            limit = COMPACTION_FRAGMENTS
        ));
        let (badge, tone) = index_status(entry);
        rows.push(
            item(
                &entry.project_id,
                entry.name.clone(),
                tr!(
                    "tui.index.stats",
                    files = entry.files,
                    symbols = compact_count(entry.symbols as u64)
                ),
            )
            .badge(badge, tone)
            .actions(actions)
            .lines(lines)
            .job(entry.job.clone()),
        );
    }
    rows
}

pub(super) fn engine(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let number_draft = |label: String, key: &str, range: (i64, i64, i64)| {
        number(label, count(app, key), range, draft(key))
    };
    let model = string(app, "engine.embeddingModel");
    let dimensions: Vec<(String, String)> = system
        .embedding_models
        .iter()
        .find(|(id, ..)| *id == model)
        .map(|(_, _, sizes)| {
            sizes
                .iter()
                .map(|d| (d.to_string(), d.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let current_dimensions = string(app, "engine.dimensions");
    let mut rows = vec![
        note(tr!("tui.index.engineNote"), Tone::Muted),
        section(tr!("settings.inference.embedding")),
        choice(
            tr!("settings.inference.embedding"),
            system
                .embedding_models
                .iter()
                .map(|(id, label, _)| (id.clone(), label.clone()))
                .collect(),
            model,
            draft("engine.embeddingModel"),
        ),
        choice(
            tr!("knowledge.experience.dimensions"),
            dimensions.clone(),
            current_dimensions.clone(),
            draft("engine.dimensions"),
        ),
    ];
    let dimensions_valid = dimensions.iter().any(|(d, _)| *d == current_dimensions);
    if !dimensions_valid {
        rows.push(note(
            tr!("knowledge.experience.dimensionsInvalid"),
            Tone::Bad,
        ));
    }
    let mut test = button(
        tr!("settings.inference.testConnection"),
        sys_action(SystemCommand::TestRoute("engine.embedding".into())),
        Tone::Normal,
    );
    if let Some((severity, message)) = system.route_tests.get("engine.embedding") {
        test = test.detail(format!(
            "{} {message}",
            if *severity == Severity::Info {
                "✓"
            } else {
                "✗"
            }
        ));
    }
    rows.push(test);
    let pending =
        super::super::action::draft_keys(app, &["engine.embeddingModel", "engine.dimensions"]);
    if !pending.is_empty() {
        rows.push(note(tr!("tui.index.rebuildNote"), Tone::Warn));
    }

    rows.push(section(tr!("codeContext.project.indexUpdateMode")));
    rows.push(choice(
        tr!("codeContext.project.indexUpdateMode"),
        vec![
            ("automatic".into(), tr!("codeContext.project.automatic")),
            ("manual".into(), tr!("codeContext.project.manual")),
        ],
        string(app, "engine.updateMode"),
        draft("engine.updateMode"),
    ));
    rows.push(number_draft(
        tr!("tui.index.minInterval"),
        "engine.minInterval",
        (5, 3_600, 5),
    ));
    rows.push(number_draft(
        tr!("tui.index.maxFileBytes"),
        "engine.maxFileBytes",
        (65_536, 16_777_216, 65_536),
    ));
    rows.push(text(
        tr!("tui.index.ignore"),
        string(app, "engine.ignore"),
        draft("engine.ignore"),
    ));

    rows.push(section(tr!("tui.index.query")));
    rows.push(number_draft(
        tr!("tui.index.maxResults"),
        "engine.maxResults",
        (1, 100, 1),
    ));
    rows.push(number_draft(
        tr!("tui.index.contextBudget"),
        "engine.contextBudget",
        (1_000, 200_000, 1_000),
    ));
    rows.push(number_draft(
        tr!("tui.index.embeddingConcurrency"),
        "engine.embeddingConcurrency",
        (1, 32, 1),
    ));
    rows.push(number_draft(
        tr!("tui.index.queryConcurrency"),
        "engine.queryConcurrency",
        (1, 16, 1),
    ));
    rows.push(toggle(
        tr!("tui.index.queryRewrite"),
        flag(app, "engine.queryRewrite"),
        draft("engine.queryRewrite"),
    ));
    rows.push(toggle(
        tr!("settings.inference.rerankEnabled"),
        flag(app, "engine.rerank"),
        draft("engine.rerank"),
    ));
    let problem = (!dimensions_valid).then(|| tr!("knowledge.experience.dimensionsInvalid"));
    draft_footer(app, PageId::EngineSettings, &mut rows, problem);
    rows
}

pub(super) fn plugins(system: &SystemSnapshot) -> Vec<Row> {
    let plugins = &system.plugins;
    let mut rows = vec![
        note(tr!("tui.plugins.note"), Tone::Muted),
        stats(vec![
            (
                tr!("tui.plugins.installed"),
                plugins.len().to_string(),
                Tone::Normal,
            ),
            (
                tr!("agentSkills.enabled"),
                plugins.iter().filter(|p| p.enabled).count().to_string(),
                Tone::Good,
            ),
            (
                tr!("skills.plugins.tools"),
                plugins.iter().map(|p| p.tools).sum::<u32>().to_string(),
                Tone::Normal,
            ),
            (
                tr!("skills.plugins.sessions"),
                plugins.iter().map(|p| p.sessions).sum::<u32>().to_string(),
                Tone::Normal,
            ),
        ]),
    ];
    if plugins.is_empty() {
        rows.push(note(tr!("skills.plugins.emptyTitle"), Tone::Muted));
    }
    let kinds = [
        ("builtin", tr!("skills.navigation.builtins")),
        ("system", tr!("skills.plugins.systemKind")),
        ("user", tr!("skills.plugins.userPlugin")),
    ];
    for (kind, title) in kinds {
        let list: Vec<&Plugin> = plugins.iter().filter(|p| p.kind == kind).collect();
        if list.is_empty() {
            continue;
        }
        rows.push(section(format!("{title} · {}", list.len())));
        rows.extend(list.into_iter().map(plugin_row));
    }
    rows.push(button(
        format!("+ {}", tr!("skills.plugins.installLocalPlugin")),
        UiAction::InstallPlugin,
        Tone::Accent,
    ));
    rows
}

fn plugin_row(plugin: &Plugin) -> Row {
    let (status, tone) = service_status(plugin.status);
    let figures = [
        (tr!("skills.plugins.tools"), plugin.tools),
        (tr!("skills.plugins.hooks"), plugin.hooks),
        (tr!("skills.plugins.sessions"), plugin.sessions),
        (tr!("skills.plugins.skills"), plugin.skills),
        (tr!("skills.plugins.agents"), plugin.agents),
    ];
    let mut lines = vec![
        figures
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(label, n)| format!("{label} {n}"))
            .collect::<Vec<_>>()
            .join(" · "),
    ];
    let list = |items: &[String]| {
        if items.is_empty() {
            "—".to_string()
        } else {
            items.join(", ")
        }
    };
    lines.push(format!(
        "{}  {}",
        tr!("skills.plugins.permissions"),
        list(&plugin.permissions)
    ));
    if !plugin.mounts.is_empty() {
        lines.push(format!(
            "{}  {}  ({})",
            tr!("skills.plugins.uiMounts"),
            list(&plugin.mounts),
            tr!("tui.plugins.mountSkipped")
        ));
    }
    for line in &plugin.diagnostics {
        lines.push(format!("{}  {line}", tr!("skills.plugins.diagnostics")));
    }
    lines.retain(|l| !l.is_empty());
    let mut actions = vec![switch(
        tr!("agentSkills.enabled"),
        plugin.enabled,
        Edit::Enabled(EntityKind::Plugin, plugin.id.clone()),
    )];
    if plugin.kind == "user" {
        actions.push(action(
            tr!("skills.plugins.uninstall"),
            confirm(
                tr!("skills.plugins.uninstallPlugin"),
                tr!(
                    "skills.plugins.uninstallingRemovesItsPackage",
                    name = plugin.name
                ),
                tr!("skills.plugins.uninstall"),
                sys_action(SystemCommand::Plugin(CatalogOp::Delete(plugin.id.clone()))),
            ),
            Tone::Bad,
        ));
    }
    item(&plugin.id, plugin.name.clone(), plugin.description.clone())
        .badge(status, tone)
        .lines(lines)
        .actions(actions)
}
