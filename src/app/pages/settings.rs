// Settings pages: appearance, context, notifications, models, capabilities, MCP and diagnostics.
// 设置页面：外观、上下文、通知、模型、能力、MCP 与诊断。

use super::*;
use crate::app::action::LocalSetting;
use crate::i18n;
use crate::ui::text::compact_count;

pub(super) fn appearance(app: &App) -> Vec<Row> {
    let locales = i18n::LOCALES
        .iter()
        .map(|l| (l.code.to_string(), format!("{}  {}", l.native_name, l.code)))
        .collect();
    let theme = match app.theme.mode {
        crate::ui::theme::ThemeMode::Dark => "dark",
        crate::ui::theme::ThemeMode::Light => "light",
    };
    let thinking = match app.thinking_placement {
        crate::app::ThinkingPlacement::Float => "float",
        crate::app::ThinkingPlacement::Docked => "docked",
    };
    vec![
        Row::new(
            RowKind::Choice {
                options: locales,
                current: i18n::active_locale().to_string(),
                edit: Edit::Local(LocalSetting::Locale),
            },
            tr!("global.language"),
        )
        .detail(tr!("tui.settings.localeNote")),
        Row::new(
            RowKind::Choice {
                options: vec![
                    ("dark".into(), tr!("tui.theme.dark")),
                    ("light".into(), tr!("tui.theme.light")),
                ],
                current: theme.into(),
                edit: Edit::Local(LocalSetting::Theme),
            },
            tr!("appChrome.theme"),
        ),
        Row::new(
            RowKind::Choice {
                options: vec![
                    ("float".into(), tr!("tui.thinking.float")),
                    ("docked".into(), tr!("tui.thinking.docked")),
                ],
                current: thinking.into(),
                edit: Edit::Local(LocalSetting::Thinking),
            },
            tr!("composer.thinkingProcess"),
        ),
        toggle(
            tr!("sidebar.toggle"),
            app.sidebar == crate::app::SidebarMode::Expanded,
            Edit::Local(LocalSetting::Sidebar),
        ),
        toggle(
            tr!("tui.command.togglePanel"),
            app.panel_visible,
            Edit::Local(LocalSetting::Panel),
        ),
    ]
}

pub(super) fn context(app: &App) -> Vec<Row> {
    let number = |key: &str| count(app, key);
    let mut rows = vec![
        note(tr!("tui.settings.contextNote"), Tone::Muted),
        Row::new(
            RowKind::Number {
                value: number("context.compressionThreshold"),
                min: 50,
                max: 95,
                step: 5,
                edit: draft("context.compressionThreshold"),
            },
            tr!("tui.settings.compressionThreshold"),
        ),
        Row::new(
            RowKind::Number {
                value: number("context.maxOutputTokens"),
                min: 4_000,
                max: 128_000,
                step: 4_000,
                edit: draft("context.maxOutputTokens"),
            },
            tr!("tui.settings.maxOutputTokens"),
        ),
        toggle(
            tr!("tui.settings.autoCompact"),
            setting_value(app, "context.autoCompact").is_some_and(|v| v.as_bool()),
            draft("context.autoCompact"),
        ),
        Row::new(
            RowKind::Text {
                value: setting_value(app, "context.systemPromptExtra")
                    .map(|v| v.as_str().to_string())
                    .unwrap_or_default(),
                edit: draft("context.systemPromptExtra"),
                secret: false,
            },
            tr!("tui.settings.systemPromptExtra"),
        ),
    ];
    draft_footer(app, PageId::Context, &mut rows, None);
    rows
}

pub(super) fn notifications(app: &App) -> Vec<Row> {
    let flag = |key: &str| setting_value(app, key).is_some_and(|v| v.as_bool());
    let edit = |key: &str| Edit::Setting(key.to_string());
    let enabled = flag("notifications.enabled");
    let mut rows = vec![
        note(tr!("settings.notifications.description"), Tone::Muted),
        toggle(
            tr!("settings.notifications.enabled"),
            enabled,
            edit("notifications.enabled"),
        ),
    ];
    if enabled {
        rows.push(toggle(
            tr!("settings.notifications.completed"),
            flag("notifications.completed"),
            edit("notifications.completed"),
        ));
        rows.push(toggle(
            tr!("settings.notifications.failed"),
            flag("notifications.failed"),
            edit("notifications.failed"),
        ));
        rows.push(toggle(
            tr!("settings.notifications.showTitle"),
            flag("notifications.showTitle"),
            edit("notifications.showTitle"),
        ));
    }
    rows.push(Row::new(RowKind::Spacer, ""));
    rows.push(button(
        tr!("settings.notifications.sendATest"),
        sys_action(SystemCommand::SendTestNotification),
        Tone::Accent,
    ));
    rows
}

pub(super) fn git_commit(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let edit = |key: &str| Edit::Setting(key.to_string());
    let mut options = vec![(String::new(), tr!("settings.gitCommit.followSessionModel"))];
    options.extend(
        system
            .models
            .iter()
            .filter(|m| m.internal_enabled)
            .map(|m| (m.id.clone(), m.label.clone())),
    );
    vec![
        note(tr!("settings.gitCommit.description"), Tone::Muted),
        toggle(
            tr!("settings.gitCommit.autoSummary"),
            setting_value(app, "gitCommit.autoSummary").is_some_and(|v| v.as_bool()),
            edit("gitCommit.autoSummary"),
        ),
        Row::new(
            RowKind::Choice {
                options,
                current: setting_value(app, "gitCommit.summaryModel")
                    .map(|v| v.as_str().to_string())
                    .unwrap_or_default(),
                edit: edit("gitCommit.summaryModel"),
            },
            tr!("settings.gitCommit.summaryModel"),
        ),
        Row::new(
            RowKind::Text {
                value: setting_value(app, "gitCommit.summaryExtraPrompt")
                    .map(|v| v.as_str().to_string())
                    .unwrap_or_default(),
                edit: edit("gitCommit.summaryExtraPrompt"),
                secret: false,
            },
            tr!("settings.gitCommit.summaryExtraPrompt"),
        ),
    ]
}

pub(super) fn providers(system: &SystemSnapshot) -> Vec<Row> {
    let mut rows = vec![note(tr!("settings.providers.description"), Tone::Muted)];
    for provider in &system.providers {
        let count = provider.credentials.len();
        let badge = if count == 0 {
            (tr!("tui.settings.noCredential"), Tone::Warn)
        } else {
            (
                tr!("tui.settings.credentialCount", count = count),
                Tone::Good,
            )
        };
        rows.push(
            item(
                &provider.id,
                provider.name.clone(),
                provider.category.clone(),
            )
            .badge(badge.0, badge.1)
            .actions(vec![action(
                tr!("tui.settings.manageCredentials"),
                UiAction::GoPage(PageId::Credentials),
                Tone::Accent,
            )])
            .lines(
                provider
                    .protocols
                    .iter()
                    .map(|p| format!("{}  {p}", tr!("tui.settings.protocol")))
                    .chain(std::iter::once(tr!(
                        "tui.settings.endpoints",
                        count = provider.endpoints
                    )))
                    .collect(),
            ),
        );
    }
    rows
}

pub(super) fn credentials(system: &SystemSnapshot) -> Vec<Row> {
    let mut rows = vec![note(tr!("tui.settings.credentialNote"), Tone::Muted)];
    for provider in &system.providers {
        rows.push(section(provider.name.clone()));
        let credential_command = |op: CredentialOp| {
            sys_action(SystemCommand::Credential {
                provider: provider.id.clone(),
                op,
            })
        };
        for credential in &provider.credentials {
            let mut actions = vec![action(
                tr!("settings.providerTools.test"),
                credential_command(CredentialOp::Test(credential.id.clone())),
                Tone::Accent,
            )];
            if !credential.is_default {
                actions.push(action(
                    tr!("settings.credentials.setDefault"),
                    credential_command(CredentialOp::MakeDefault(credential.id.clone())),
                    Tone::Normal,
                ));
            }
            actions.push(action(
                tr!("knowledge.delete"),
                confirm(
                    tr!("tui.confirm.deleteCredential"),
                    tr!("tui.confirm.deleteCredentialBody", name = credential.label),
                    tr!("knowledge.delete"),
                    credential_command(CredentialOp::Delete(credential.id.clone())),
                ),
                Tone::Bad,
            ));
            let mut detail = credential.masked.clone();
            if credential.is_default {
                detail = format!("{detail} · {}", tr!("tui.settings.default"));
            }
            let mut row = item(&credential.id, credential.label.clone(), detail).actions(actions);
            if let Some((severity, message)) = &credential.last_test {
                row = row.badge(message.clone(), Tone::of(*severity));
            }
            rows.push(row);
        }
        rows.push(button(
            format!("+ {}", tr!("settings.providerOnboarding.addCredential")),
            prompt(
                tr!("settings.providerOnboarding.addCredential"),
                tr!("tui.settings.secretNote", provider = provider.name),
                PromptThen::AddCredential(provider.id.clone()),
                true,
            ),
            Tone::Accent,
        ));
    }
    rows
}

pub(super) fn models(system: &SystemSnapshot) -> Vec<Row> {
    system
        .models
        .iter()
        .map(|model| {
            item(
                &model.id,
                model.label.clone(),
                format!(
                    "{} · {} · {}",
                    model.provider,
                    model.id,
                    compact_count(model.context_window)
                ),
            )
            .actions(vec![
                switch(
                    tr!("settings.models.conversationUse"),
                    model.conversation_enabled,
                    Edit::Enabled(EntityKind::ConversationModel, model.id.clone()),
                ),
                switch(
                    tr!("settings.models.internalUse"),
                    model.internal_enabled,
                    Edit::Enabled(EntityKind::InternalModel, model.id.clone()),
                ),
            ])
        })
        .collect()
}

pub(super) fn capabilities(system: &SystemSnapshot) -> Vec<Row> {
    let mut rows = vec![note(
        tr!("settings.aiCapabilities.description"),
        Tone::Muted,
    )];
    for capability in &system.capabilities {
        let mut options = vec![(String::new(), tr!("tui.settings.off"))];
        options.extend(capability.candidates.iter().map(|c| (c.clone(), c.clone())));
        rows.push(Row::new(
            RowKind::Choice {
                options,
                current: if capability.enabled {
                    capability.model.clone().unwrap_or_default()
                } else {
                    String::new()
                },
                edit: Edit::Route(EntityKind::Capability, capability.key.clone()),
            },
            capability_label(&capability.key),
        ));
    }
    rows
}

pub(super) fn mcp(system: &SystemSnapshot) -> Vec<Row> {
    system
        .mcp_servers
        .iter()
        .map(|server| {
            let (status, tone) = service_status(server.status);
            item(
                &server.id,
                server.name.clone(),
                format!(
                    "{} · {}",
                    server.transport,
                    tr!("settings.mcp.tools", length = server.tools)
                ),
            )
            .badge(status, tone)
            .actions(vec![
                switch(
                    tr!("settings.mcp.enabled"),
                    server.enabled,
                    Edit::Enabled(EntityKind::Mcp, server.id.clone()),
                ),
                action(
                    tr!("vmm.runtime.restart"),
                    sys_action(SystemCommand::RestartMcp(server.id.clone())),
                    Tone::Accent,
                ),
            ])
        })
        .collect()
}

pub(super) fn archived(system: &SystemSnapshot) -> Vec<Row> {
    if system.archived.is_empty() {
        return vec![note(
            tr!("settings.misc.noArchivedConversations"),
            Tone::Muted,
        )];
    }
    system
        .archived
        .iter()
        .map(|entry| {
            item(
                &entry.id,
                entry.title.clone(),
                format!("{} · {}", entry.project, entry.archived_label),
            )
            .actions(vec![
                action(
                    tr!("settings.misc.restore"),
                    sys_action(SystemCommand::RestoreArchived(entry.id.clone())),
                    Tone::Accent,
                ),
                action(
                    tr!("settings.misc.delete"),
                    confirm(
                        tr!("settings.misc.delete"),
                        tr!("tui.confirm.deleteArchivedBody", name = entry.title),
                        tr!("settings.misc.delete"),
                        sys_action(SystemCommand::DeleteArchived(entry.id.clone())),
                    ),
                    Tone::Bad,
                ),
            ])
        })
        .collect()
}

pub(super) fn diagnostics(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let mut rows = vec![
        note(tr!("settings.diagnostics.description"), Tone::Muted),
        section(tr!("settings.diagnostics.startupStatus")),
    ];
    for check in &system.startup {
        rows.push(info(
            check.label.clone(),
            check.detail.clone(),
            Tone::of(check.severity),
        ));
    }
    rows.push(section(tr!("settings.diagnostics.providerRequestTrace")));
    rows.push(toggle(
        tr!("settings.diagnostics.providerRequestTrace"),
        setting_value(app, "diagnostics.providerRequestTrace").is_some_and(|v| v.as_bool()),
        Edit::Setting("diagnostics.providerRequestTrace".into()),
    ));
    rows.push(section(tr!("settings.diagnostics.logs")));
    if system.logs.is_empty() {
        rows.push(note(tr!("vmm.common.none"), Tone::Muted));
    }
    for line in &system.logs {
        let tone = if line.contains("ERROR") {
            Tone::Bad
        } else if line.contains("WARN") {
            Tone::Warn
        } else {
            Tone::Muted
        };
        rows.push(note(line.clone(), tone));
    }
    rows.push(button(
        tr!("tui.settings.cleanupLogs"),
        confirm(
            tr!("tui.settings.cleanupLogs"),
            tr!("settings.diagnostics.confirmCleanup"),
            tr!("projectDialog.confirm"),
            sys_action(SystemCommand::CleanupLogs),
        ),
        Tone::Bad,
    ));
    rows
}

pub(super) fn about(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let locale = i18n::LOCALES
        .iter()
        .find(|l| l.code == i18n::active_locale())
        .map_or_else(String::new, |l| format!("{} ({})", l.native_name, l.code));
    vec![
        Row::new(RowKind::Logo, ""),
        info(
            tr!("tui.about.version"),
            system.version.clone(),
            Tone::Normal,
        ),
        info(
            tr!("tui.about.backend"),
            app.backend.label.clone(),
            Tone::Normal,
        ),
        info(tr!("tui.about.service"), app.base_url.clone(), Tone::Muted),
        info(tr!("global.language"), locale, Tone::Normal),
        note(tr!("tui.about.note"), Tone::Muted),
    ]
}
