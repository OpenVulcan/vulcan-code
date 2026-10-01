// Memory pages: runtime overview, stored items, profile, projects, maintenance, and the rules,
// inference, resource and advanced configuration pages.
// 记忆页面：运行概览、记忆条目、画像、项目、维护，以及规则、推理、资源与高级配置页面。

use super::*;
use crate::i18n;

pub(super) fn overview(system: &SystemSnapshot) -> Vec<Row> {
    let memory = &system.memory;
    let (status, tone) = service_status(memory.status);
    let busy = memory.status.is_transition();
    let service = |op: ServiceOp| sys_action(SystemCommand::MemoryService(op));
    let health = match memory.health {
        Severity::Info => tr!("vmm.experience.health.healthy"),
        Severity::Warning => tr!("tui.status.degraded"),
        Severity::Error => tr!("tui.status.failed"),
    };
    let mut rows = vec![
        note(tr!("area.memory.description"), Tone::Muted),
        section(tr!("vmm.runtime.title")),
        stats(vec![
            (tr!("tui.memory.status"), status, tone),
            (tr!("tui.memory.health"), health, Tone::of(memory.health)),
            (
                tr!("tui.memory.pendingWritebacks"),
                memory.pending_writebacks.to_string(),
                if memory.pending_writebacks > 0 {
                    Tone::Warn
                } else {
                    Tone::Normal
                },
            ),
            (
                tr!("tui.memory.completedTurns"),
                memory.completed_turns.to_string(),
                Tone::Normal,
            ),
        ]),
        info(
            tr!("tui.memory.deployment"),
            memory.deployment.clone(),
            Tone::Normal,
        ),
        info(
            tr!("tui.memory.lastProfileUpdate"),
            memory.last_profile_update.clone(),
            Tone::Normal,
        ),
        info(
            tr!("tui.memory.total"),
            tr!(
                "tui.memory.itemCount",
                count = memory.projects.iter().map(|p| p.items).sum::<u32>()
            ),
            Tone::Normal,
        ),
        spacer(),
    ];
    if !busy {
        if memory.status == ServiceStatus::Running {
            rows.push(button(
                tr!("vmm.runtime.restart"),
                service(ServiceOp::Restart),
                Tone::Accent,
            ));
            rows.push(button(
                tr!("vmm.runtime.stop"),
                confirm(
                    tr!("vmm.runtime.stop"),
                    tr!("tui.confirm.stopMemoryBody"),
                    tr!("vmm.runtime.stop"),
                    service(ServiceOp::Stop),
                ),
                Tone::Bad,
            ));
        } else {
            rows.push(button(
                tr!("vmm.runtime.start"),
                service(ServiceOp::Start),
                Tone::Accent,
            ));
        }
    }
    job_rows(&mut rows, &memory.jobs);
    rows
}

fn memory_item(entry: &MemoryItem) -> Row {
    let mut detail = format!("{} · {}", entry.project, entry.source);
    if let Some(score) = entry.score {
        detail = format!("{detail} · {score:.2}");
    }
    item(&entry.id, entry.content.clone(), detail)
        .lines(vec![entry.content.clone()])
        .actions(vec![action(
            tr!("knowledge.delete"),
            confirm(
                tr!("memory.content.deleteTitle"),
                tr!("memory.content.deleteDescription"),
                tr!("knowledge.delete"),
                sys_action(SystemCommand::MemoryDelete(entry.id.clone())),
            ),
            Tone::Bad,
        )])
}

pub(super) fn content(system: &SystemSnapshot) -> Vec<Row> {
    let memory = &system.memory;
    let mut rows = vec![
        note(tr!("memory.content.description"), Tone::Muted),
        button(
            format!("⌕ {}", tr!("memory.content.search")),
            PageId::MemoryContent
                .search()
                .expect("memory content has a search"),
            Tone::Accent,
        ),
    ];
    if memory.searching {
        rows.push(note(tr!("tui.page.searching"), Tone::Warn));
    }
    let list: &[MemoryItem] = match &memory.search {
        Some((query, hits)) => {
            rows.push(info(
                tr!("tui.page.query"),
                format!("“{query}” · {}", hits.len()),
                Tone::Accent,
            ));
            rows.push(button(
                tr!("tui.page.clearSearch"),
                sys_action(SystemCommand::MemorySearch(String::new())),
                Tone::Muted,
            ));
            hits
        }
        None => &memory.items,
    };
    rows.push(section(tr!("tui.memory.items", count = list.len())));
    if list.is_empty() {
        rows.push(note(tr!("memory.content.empty"), Tone::Muted));
    }
    rows.extend(list.iter().map(memory_item));
    rows
}

pub(super) fn profile(system: &SystemSnapshot) -> Vec<Row> {
    let memory = &system.memory;
    let mut rows = vec![note(tr!("memory.profile.description"), Tone::Muted)];
    if memory.profile.is_empty() {
        rows.push(note(tr!("memory.profile.empty"), Tone::Muted));
    }
    rows.extend(
        memory
            .profile
            .iter()
            .map(|line| note(format!("• {line}"), Tone::Normal)),
    );
    rows.push(spacer());
    rows.push(button(
        tr!("tui.memory.correctProfile"),
        prompt(
            tr!("tui.memory.correctProfile"),
            tr!("tui.memory.correctProfileNote"),
            PromptThen::MemoryCorrect,
            false,
        ),
        Tone::Accent,
    ));
    rows
}

pub(super) fn maintenance(system: &SystemSnapshot) -> Vec<Row> {
    let run = |task: MaintenanceTask| sys_action(SystemCommand::MemoryMaintenance(task));
    let mut rows = vec![
        note(tr!("memory.maintenance.description"), Tone::Muted),
        button(
            tr!("tui.memory.organize"),
            run(MaintenanceTask::Organize),
            Tone::Accent,
        )
        .detail(tr!("tui.memory.organizeNote")),
        button(
            tr!("tui.memory.refreshProfile"),
            run(MaintenanceTask::RefreshProfile),
            Tone::Accent,
        )
        .detail(tr!("tui.memory.refreshProfileNote")),
        button(
            tr!("tui.memory.cleanup"),
            confirm(
                tr!("tui.memory.cleanup"),
                tr!("tui.memory.cleanupNote"),
                tr!("projectDialog.confirm"),
                run(MaintenanceTask::Cleanup),
            ),
            Tone::Bad,
        )
        .detail(tr!("tui.memory.cleanupNote")),
    ];
    job_rows(&mut rows, &system.memory.jobs);
    rows
}

pub(super) fn projects(system: &SystemSnapshot) -> Vec<Row> {
    let projects = &system.memory.projects;
    let total: u32 = projects.iter().map(|p| p.items).sum();
    let default = projects
        .iter()
        .find(|p| p.default)
        .map_or_else(|| "—".to_string(), |p| p.name.clone());
    let op = |op: MemoryProjectOp| sys_action(SystemCommand::MemoryProject(op));
    let mut rows = vec![
        note(tr!("memory.identity.nativeDescription"), Tone::Muted),
        stats(vec![
            (
                tr!("memory.identity.projects"),
                projects.len().to_string(),
                Tone::Normal,
            ),
            (
                tr!("tui.memory.total"),
                tr!("tui.memory.itemCount", count = total),
                Tone::Normal,
            ),
            (tr!("memory.identity.default"), default, Tone::Accent),
        ]),
        section(tr!("memory.identity.projects")),
    ];
    if projects.is_empty() {
        rows.push(note(tr!("memory.identity.emptyProjects"), Tone::Muted));
    }
    for project in projects {
        let mut actions = Vec::new();
        if !project.default {
            actions.push(action(
                tr!("memory.identity.setDefault"),
                op(MemoryProjectOp::SetDefault(project.id.clone())),
                Tone::Accent,
            ));
        }
        if project.items > 0 {
            actions.push(action(
                tr!("memory.identity.migrate"),
                prompt(
                    tr!("memory.identity.migrateTitle"),
                    format!(
                        "{}\n{}",
                        tr!("memory.identity.migrateDescription"),
                        tr!("memory.identity.targetProjectName")
                    ),
                    PromptThen::MemoryMigrate(project.id.clone()),
                    false,
                ),
                Tone::Normal,
            ));
        }
        if !project.protected {
            actions.push(action(
                tr!("memory.identity.delete"),
                confirm(
                    tr!("memory.identity.confirmDeleteTitle"),
                    format!(
                        "{}\n{}",
                        project.name,
                        tr!("memory.identity.confirmDeleteDescription")
                    ),
                    tr!("memory.identity.delete"),
                    op(MemoryProjectOp::Delete(project.id.clone())),
                ),
                Tone::Bad,
            ));
        }
        let mut row = item(
            &project.id,
            project.name.clone(),
            tr!("tui.memory.itemCount", count = project.items),
        )
        .actions(actions);
        if project.default {
            row = row.badge(tr!("memory.identity.default"), Tone::Good);
        } else if project.protected {
            row = row.badge(tr!("memory.identity.protected"), Tone::Muted);
        }
        rows.push(row);
    }
    rows.push(button(
        format!("+ {}", tr!("memory.identity.addProject")),
        prompt(
            tr!("memory.identity.addProject"),
            tr!("memory.identity.projectPlaceholder"),
            PromptThen::MemoryProjectCreate,
            false,
        ),
        Tone::Accent,
    ));
    rows
}

pub(super) fn rules(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let toggle_draft = |label: String, key: &str| toggle(label, flag(app, key), draft(key));
    let number_draft =
        |label: String, key: &str, range| number(label, count(app, key), range, draft(key));
    let mut scopes = vec![(String::new(), tr!("memory.rules.allProjects"))];
    scopes.extend(
        system
            .memory
            .projects
            .iter()
            .map(|p| (p.id.clone(), p.name.clone())),
    );
    let mut rows = vec![
        note(tr!("memory.rules.description"), Tone::Muted),
        choice(
            tr!("memory.rules.applyTitle"),
            scopes,
            string(app, "memory.rules.scope"),
            immediate("memory.rules.scope"),
        )
        .detail(tr!("memory.rules.applyDescription")),
        section(tr!("memory.rules.features")),
        toggle_draft(tr!("memory.rules.enable"), "memory.rules.enable")
            .detail(tr!("memory.rules.enableDescription")),
        toggle_draft(tr!("memory.rules.recall"), "memory.rules.recall")
            .detail(tr!("memory.rules.recallDescription")),
        toggle_draft(tr!("memory.rules.organize"), "memory.rules.organize")
            .detail(tr!("memory.rules.organizeDescription")),
        toggle_draft(tr!("memory.rules.aiTools"), "memory.rules.aiTools")
            .detail(tr!("memory.rules.aiToolsDescription")),
        section(tr!("memory.rules.retentionProfiles")),
        number_draft(
            tr!("memory.rules.recallTurns"),
            "memory.rules.recallTurns",
            (0, 50, 1),
        ),
        number_draft(
            tr!("memory.rules.profileRefresh"),
            "memory.rules.profileRefresh",
            (1, 500, 5),
        ),
        number_draft(
            tr!("memory.rules.profileLimit"),
            "memory.rules.profileLimit",
            (5, 500, 5),
        ),
        section(tr!("memory.policy.title")),
        number_draft(
            tr!("memory.policy.precheckHistory"),
            "memory.policy.precheckHistory",
            (0, 50, 1),
        ),
        number_draft(
            tr!("memory.policy.precheckBudget"),
            "memory.policy.precheckBudget",
            (0, 200_000, 1_000),
        ),
        number_draft(
            tr!("memory.policy.recallTopK"),
            "memory.policy.recallTopK",
            (1, 100, 1),
        ),
        number_draft(
            tr!("memory.policy.reviewLimit"),
            "memory.policy.reviewLimit",
            (1, 200, 1),
        ),
        number_draft(
            tr!("memory.policy.postactionHistory"),
            "memory.policy.postactionHistory",
            (0, 50, 1),
        ),
        number_draft(
            tr!("memory.policy.postactionBudget"),
            "memory.policy.postactionBudget",
            (0, 400_000, 1_000),
        ),
        choice(
            tr!("memory.policy.extractedLevel"),
            vec![
                ("working".into(), tr!("memory.policy.working")),
                ("stable".into(), tr!("memory.policy.stable")),
                ("archival".into(), tr!("memory.policy.archival")),
            ],
            string(app, "memory.policy.extractedLevel"),
            draft("memory.policy.extractedLevel"),
        )
        .detail(tr!("memory.policy.extractedHint")),
        section(tr!("memory.ranking.title")),
        toggle_draft(tr!("memory.ranking.hybrid"), "memory.ranking.hybrid"),
        toggle_draft(tr!("memory.ranking.mmr"), "memory.ranking.mmr"),
        toggle_draft(tr!("memory.ranking.decay"), "memory.ranking.decay"),
        number_draft(
            tr!("memory.ranking.rerank"),
            "memory.ranking.rerank",
            (0, 200, 5),
        ),
    ];
    draft_footer(app, PageId::MemoryRules, &mut rows, None);
    rows
}

/// One model route of the memory runtime: pick a model and run a real connection test.
/// 记忆运行时的一条模型路由：选择模型并执行真实连接测试。
fn route(
    app: &App,
    system: &SystemSnapshot,
    key: &str,
    label: String,
    description: String,
    options: Vec<(String, String)>,
) -> Row {
    let current = string(app, key);
    let model = options
        .iter()
        .find(|(id, _)| *id == current)
        .map_or_else(|| current.clone(), |(_, label)| label.clone());
    let mut row = item(key, label.clone(), model)
        .lines(vec![description])
        .actions(vec![
            action(
                tr!("settings.inference.selectModel"),
                UiAction::Pick {
                    title: label,
                    edit: immediate(key),
                    options,
                    current,
                },
                Tone::Accent,
            ),
            action(
                tr!("settings.inference.testConnection"),
                sys_action(SystemCommand::TestRoute(key.to_string())),
                Tone::Normal,
            ),
        ]);
    if let Some((severity, message)) = system.route_tests.get(key) {
        row = row.badge(message.clone(), Tone::of(*severity));
    }
    row
}

pub(super) fn inference(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let not_configured = tr!("settings.inference.noRoute");
    let llm = internal_models(system, not_configured.clone());
    let mut rows = vec![
        note(tr!("settings.inference.subtitle"), Tone::Muted),
        section(tr!("settings.inference.llm")),
    ];
    let purposes = [
        (
            "memory.inference.precheckL1",
            tr!("settings.inference.purpose.precheckL1"),
            tr!("settings.inference.purpose.precheckL1Description"),
        ),
        (
            "memory.inference.precheckL2",
            tr!("settings.inference.purpose.precheckL2"),
            tr!("settings.inference.purpose.precheckL2Description"),
        ),
        (
            "memory.inference.postactionL1",
            tr!("settings.inference.purpose.postactionL1"),
            tr!("settings.inference.purpose.postactionL1Description"),
        ),
        (
            "memory.inference.postactionL2",
            tr!("settings.inference.purpose.postactionL2"),
            tr!("settings.inference.purpose.postactionL2Description"),
        ),
    ];
    for (key, label, description) in purposes {
        rows.push(route(app, system, key, label, description, llm.clone()));
    }

    rows.push(section(tr!("settings.inference.embedding")));
    let mut embeddings = vec![(String::new(), not_configured.clone())];
    embeddings.extend(
        system
            .embedding_models
            .iter()
            .map(|(id, label, _)| (id.clone(), label.clone())),
    );
    rows.push(route(
        app,
        system,
        "memory.inference.embedding",
        tr!("settings.inference.embedding"),
        tr!("tui.memory.embeddingNote"),
        embeddings,
    ));
    let model = string(app, "memory.inference.embedding");
    let mut dimensions = vec![(String::new(), tr!("settings.inference.inheritModelDefault"))];
    if let Some((_, _, sizes)) = system.embedding_models.iter().find(|(id, ..)| *id == model) {
        dimensions.extend(sizes.iter().map(|d| (d.to_string(), d.to_string())));
    }
    rows.push(choice(
        tr!("settings.inference.embeddingDimensions"),
        dimensions,
        string(app, "memory.inference.embeddingDimensions"),
        immediate("memory.inference.embeddingDimensions"),
    ));

    rows.push(section(tr!("settings.inference.rerank")));
    let rerank_on = flag(app, "memory.inference.rerankEnabled");
    rows.push(toggle(
        tr!("settings.inference.rerankEnabled"),
        rerank_on,
        immediate("memory.inference.rerankEnabled"),
    ));
    if rerank_on {
        let mut reranks = vec![(String::new(), not_configured)];
        reranks.extend(system.rerank_models.iter().cloned());
        rows.push(route(
            app,
            system,
            "memory.inference.rerank",
            tr!("settings.inference.rerank"),
            tr!("tui.memory.rerankNote"),
            reranks,
        ));
    } else {
        rows.push(note(tr!("settings.inference.rerankDisabled"), Tone::Muted));
    }
    rows
}

/// Long purpose text of a built-in memory prompt file.
/// 内置记忆提示词文件的用途说明。
fn prompt_purpose(purpose: &str) -> (String, String) {
    match purpose {
        "precheckL1" => (
            tr!("settings.inference.purpose.precheckL1"),
            tr!("memory.resources.promptPurpose.precheckL1"),
        ),
        "precheckL2" => (
            tr!("settings.inference.purpose.precheckL2"),
            tr!("memory.resources.promptPurpose.precheckL2"),
        ),
        "postactionL1" => (
            tr!("settings.inference.purpose.postactionL1"),
            tr!("memory.resources.promptPurpose.postactionL1"),
        ),
        "postactionL2" => (
            tr!("settings.inference.purpose.postactionL2"),
            tr!("memory.resources.promptPurpose.postactionL2"),
        ),
        "profileInstruction" => (
            tr!("settings.inference.purpose.profileInstruction"),
            tr!("memory.resources.promptPurpose.profileInstruction"),
        ),
        _ => (String::new(), tr!("memory.resources.promptPurpose.unknown")),
    }
}

pub(super) fn resources(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let locales: Vec<(String, String)> = i18n::LOCALES
        .iter()
        .map(|l| (l.code.to_string(), format!("{}  {}", l.native_name, l.code)))
        .collect();
    let regions: Vec<(String, String)> = ["CN", "TW", "HK", "US", "JP"]
        .iter()
        .map(|r| (r.to_string(), r.to_string()))
        .collect();
    let mut rows = vec![
        note(tr!("memory.resources.description"), Tone::Muted),
        section(tr!("memory.resources.composition")),
        choice(
            tr!("memory.resources.promptBundle"),
            locales,
            string(app, "memory.resources.promptBundle"),
            draft("memory.resources.promptBundle"),
        ),
        choice(
            tr!("memory.resources.piiLocale"),
            regions.clone(),
            string(app, "memory.resources.piiLocale"),
            draft("memory.resources.piiLocale"),
        ),
        choice(
            tr!("memory.resources.noiseLocale"),
            regions,
            string(app, "memory.resources.noiseLocale"),
            draft("memory.resources.noiseLocale"),
        ),
        toggle(
            tr!("memory.resources.noiseEnabled"),
            flag(app, "memory.resources.noiseEnabled"),
            draft("memory.resources.noiseEnabled"),
        ),
        toggle(
            tr!("memory.resources.noiseSemanticEnabled"),
            flag(app, "memory.resources.noiseSemanticEnabled"),
            draft("memory.resources.noiseSemanticEnabled"),
        ),
        number(
            tr!("memory.resources.noiseThreshold"),
            count(app, "memory.resources.noiseThreshold"),
            (0, 100, 1),
            draft("memory.resources.noiseThreshold"),
        ),
    ];
    draft_footer(app, PageId::MemoryResources, &mut rows, None);

    let categories = [
        ("prompt", tr!("memory.resources.promptTab"), None),
        (
            "pii",
            tr!("memory.resources.piiTab"),
            Some(tr!("memory.resources.piiPurpose")),
        ),
        (
            "noise",
            tr!("memory.resources.noiseTab"),
            Some(tr!("memory.resources.noisePurpose")),
        ),
    ];
    for (category, title, purpose) in categories {
        rows.push(section(title));
        if let Some(purpose) = purpose {
            rows.push(note(purpose, Tone::Muted));
        }
        for resource in system
            .memory
            .resources
            .iter()
            .filter(|r| r.category == category)
        {
            let (name, long) = if category == "prompt" {
                prompt_purpose(&resource.purpose)
            } else {
                (String::new(), String::new())
            };
            let (badge, tone) = if resource.overridden {
                (tr!("memory.resources.userOverride"), Tone::Warn)
            } else {
                (tr!("memory.resources.builtIn"), Tone::Muted)
            };
            let mut row = item(&resource.id, resource.file.clone(), name).badge(badge, tone);
            if !long.is_empty() {
                row = row.lines(vec![long]);
            }
            if resource.overridden {
                row = row.actions(vec![action(
                    tr!("memory.resources.restoreBuiltIn"),
                    confirm(
                        tr!("memory.resources.resetConfirmTitle"),
                        format!(
                            "{}\n{}",
                            resource.file,
                            tr!("memory.resources.resetConfirmDescription")
                        ),
                        tr!("memory.resources.resetConfirmAction"),
                        sys_action(SystemCommand::MemoryResourceRestore(resource.id.clone())),
                    ),
                    Tone::Bad,
                )]);
            }
            rows.push(row);
        }
    }
    rows.push(spacer());
    rows.push(note(tr!("tui.memory.resourcesEditNote"), Tone::Muted));
    rows
}

pub(super) fn advanced(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let external = string(app, "memory.advanced.deployment") == "external";
    let secret = |label: String, key: &str| {
        // The masked value comes from the service; the typed secret is never kept locally.
        // 打码值来自服务；输入的密钥从不在本地保存。
        let stored = system
            .setting(key)
            .map(|v| v.as_str().to_string())
            .unwrap_or_default();
        Row::new(
            RowKind::Text {
                value: stored,
                edit: Edit::Secret(key.to_string()),
                secret: true,
            },
            label,
        )
        .detail(tr!("tui.settings.secretKeep"))
    };
    let mut rows = vec![
        note(tr!("memory.advanced.description"), Tone::Muted),
        choice(
            tr!("memory.advanced.deployment"),
            vec![
                ("native".into(), tr!("memory.advanced.native")),
                ("external".into(), tr!("memory.advanced.external")),
            ],
            string(app, "memory.advanced.deployment"),
            draft("memory.advanced.deployment"),
        )
        .detail(if external {
            tr!("memory.advanced.externalDescription")
        } else {
            tr!("memory.advanced.nativeDescription")
        }),
    ];
    if external {
        rows.push(section(tr!("memory.advanced.connection")));
        rows.push(text(
            tr!("memory.advanced.endpoint"),
            string(app, "memory.advanced.endpoint"),
            draft("memory.advanced.endpoint"),
        ));
        rows.push(secret(
            tr!("memory.advanced.apiKey"),
            "memory.advanced.apiKey",
        ));
        rows.push(text(
            tr!("memory.advanced.managementEndpoint"),
            string(app, "memory.advanced.managementEndpoint"),
            draft("memory.advanced.managementEndpoint"),
        ));
        rows.push(secret(
            tr!("memory.advanced.managementToken"),
            "memory.advanced.managementToken",
        ));
    } else {
        rows.push(note(
            tr!("memory.advanced.nativeStorageDescription"),
            Tone::Muted,
        ));
    }
    rows.push(section(tr!("memory.advanced.runtime")));
    rows.push(toggle(
        tr!("memory.advanced.autoStart"),
        flag(app, "memory.advanced.autoStart"),
        draft("memory.advanced.autoStart"),
    ));
    rows.push(section(tr!("memory.advanced.postactionReliability")));
    if external {
        rows.push(note(
            tr!("memory.advanced.externalReliabilityDescription"),
            Tone::Muted,
        ));
    } else {
        rows.push(number(
            tr!("memory.advanced.postactionTimeout"),
            count(app, "memory.advanced.postactionTimeout"),
            (10, 3_600, 10),
            draft("memory.advanced.postactionTimeout"),
        ));
        rows.push(number(
            tr!("memory.advanced.failurePassThreshold"),
            count(app, "memory.advanced.failurePassThreshold"),
            (1, 20, 1),
            draft("memory.advanced.failurePassThreshold"),
        ));
        rows.push(number(
            tr!("memory.advanced.maxConnectionsPerHost"),
            count(app, "memory.advanced.maxConnectionsPerHost"),
            (1, 64, 1),
            draft("memory.advanced.maxConnectionsPerHost"),
        ));
    }
    draft_footer(app, PageId::MemoryAdvanced, &mut rows, None);
    rows.push(section(tr!("memory.advanced.diagnostics")));
    for (key, value) in &system.memory.diagnostics {
        let label = match key.as_str() {
            "runtime" => tr!("memory.advanced.diagnosticRuntime"),
            "backend" => tr!("memory.advanced.diagnosticBackend"),
            "grpc" => tr!("memory.advanced.diagnosticGrpc"),
            "controller" => tr!("memory.advanced.diagnosticController"),
            "data" => tr!("memory.advanced.diagnosticData"),
            "log" => tr!("memory.advanced.diagnosticLog"),
            other => other.to_string(),
        };
        rows.push(info(label, value.clone(), Tone::Muted));
    }
    rows
}
