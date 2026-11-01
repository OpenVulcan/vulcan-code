// Settings pages reading the WebUI settings and workbench DTOs.
// 读取 WebUI 设置与工作台 DTO 的设置页面。

use super::*;
use crate::app::action::LocalSetting;
use crate::i18n;
use api::settings_actions as settings;
use vulcan_service_api::SystemPromptScope;

pub(super) fn appearance(app: &App) -> Vec<Row> {
    // Locales.
    // 支持的语言标签。
    let locales = i18n::LOCALES
        .iter()
        .map(|l| (l.code.to_string(), format!("{}  {}", l.native_name, l.code)))
        .collect();
    // Theme.
    // 当前主题。
    let theme = match app.theme.mode {
        crate::ui::theme::ThemeMode::Dark => "dark",
        crate::ui::theme::ThemeMode::Light => "light",
    };
    // Thinking.
    // 思考显示模式。
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

/// Builds context controls from the public AI settings and output-style catalogs.
/// 根据公共 AI 设置与输出风格目录构建上下文控件。
pub(super) fn context(app: &App) -> Vec<Row> {
    // Public AI settings.
    // 公开 AI 设置。
    let ai = app.slots.value(Slot::PublicAiSettings);
    // Nodes.
    // 上下文窗口节点配置。
    let nodes = v(ai, "/contextCompactionWindowNodes");
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    for (index, node) in a(nodes, "/nodes").iter().enumerate() {
        rows.push(number(
            s(node, "/id"),
            n(node, "/tokens"),
            (1, i32::MAX.into(), 1000),
            dto(
                nodes.clone(),
                &format!("/nodes/{index}/tokens"),
                settings::save_public_ai_context_compaction_window_nodes,
            ),
        ));
    }
    // Null is the upstream default; the fixed node ids always come from the service.
    // 空值是上游默认值；固定节点标识始终来自服务。
    let mut defaults = vec![(String::new(), tr!("tui.field.default"))];
    defaults.extend(
        a(nodes, "/nodes")
            .iter()
            .map(|node| (s(node, "/id"), s(node, "/id"))),
    );
    rows.push(choice(
        tr!("tui.field.newSessionWindow"),
        defaults,
        s(ai, "/newSessionContextCompactionWindowNodeId"),
        Edit::Call(Setter::Build {
            ctx: Value::Null,
            build: |_, value| {
                Some(settings::save_public_ai_new_session_context_window_default(
                    (!value.as_str().is_empty()).then_some(value.as_str()),
                ))
            },
        }),
    ));
    rows.push(number(
        tr!("tui.field.usageRefresh"),
        n(ai, "/usageRefreshIntervalSeconds"),
        (0, 86400, 1),
        Edit::Call(Setter::Build {
            ctx: Value::Null,
            build: |_, value| {
                Some(settings::save_public_ai_usage_refresh_settings(
                    value.as_number(),
                ))
            },
        }),
    ));
    // Styles.
    // 输出风格目录。
    let styles = app.slots.value(Slot::OutputStyleCatalog);
    rows.push(choice(
        tr!("tui.field.outputStyle"),
        a(styles, "/entries")
            .iter()
            .map(|style| {
                // Label.
                // 当前显示文案。
                let label = if i18n::active_locale().starts_with("zh") {
                    s(style, "/labelZh")
                } else {
                    s(style, "/labelEn")
                };
                (s(style, "/id"), label)
            })
            .collect(),
        s(styles, "/selectedId"),
        Edit::Call(Setter::Build {
            ctx: Value::Null,
            build: |_, value| Some(settings::save_prompt_output_style(value.as_str())),
        }),
    ));
    // Authoritative workbench.
    // 工作台权威状态。
    let wb = app.slots.value(Slot::Workbench);
    // The global default is copied into conversations created afterwards; before the workbench
    // loads, the row shows the session contract's system default.
    // 全局默认会复制进之后新建的会话；工作台载入前，该行显示会话契约的系统默认值。
    use vulcan_session_core::v1::config::AnswerMode;
    let answer_mode = s(wb, "/defaultAnswerMode");
    rows.push(choice(
        tr!("answerMode.globalDefault"),
        vec![
            (
                AnswerMode::Standard.wire_name().into(),
                tr!("answerMode.standard"),
            ),
            (
                AnswerMode::Interactive.wire_name().into(),
                tr!("answerMode.interactive"),
            ),
        ],
        if answer_mode.is_empty() {
            AnswerMode::default().wire_name().into()
        } else {
            answer_mode
        },
        Edit::Call(Setter::Build {
            ctx: Value::Null,
            build: |_, value| Some(settings::save_prompt_answer_mode(value.as_str())),
        }),
    ));
    rows.push(choice(
        tr!("composer.thinkingProcess"),
        vec![
            ("popup_only".into(), tr!("tui.field.thinkingPopup")),
            ("inline_output".into(), tr!("tui.field.thinkingInline")),
            (
                "first_inline_with_popup".into(),
                tr!("tui.field.thinkingBoth"),
            ),
            ("first_inline_only".into(), tr!("tui.field.thinkingFirst")),
        ],
        s(wb, "/thinkingDisplayMode"),
        Edit::Call(Setter::Build {
            ctx: Value::Null,
            build: |_, value| {
                Some(settings::select_workbench_thinking_display_mode(
                    value.as_str(),
                ))
            },
        }),
    ));
    rows.push(toggle(
        tr!("tui.field.collapseTools"),
        b(wb, "/toolCollapseDefaultEnabled"),
        Edit::Call(Setter::Build {
            ctx: Value::Null,
            build: |_, value| Some(settings::save_tool_collapse_default(value.as_bool())),
        }),
    ));
    session_title_rows(&mut rows, wb, ai);
    rows.extend(system_prompt(app));
    rows
}

/// Consumer profile and purpose of the session title route, owned by the service kernel.
/// 会话标题路由的消费者配置档与用途，由服务内核持有。
const SESSION_TITLE_PROFILE: &str = "session_title";
const SESSION_TITLE_PURPOSE: &str = "session_title_generation";

/// Effort choice that saves no explicit reasoning effort.
/// 不保存显式思考档位的选项。
const SESSION_TITLE_MODEL_DEFAULT_EFFORT: &str = "model-default";

/// Mirrors `candidateSupportsDisabledReasoning`: whether a route can run with thinking off.
/// 镜像 `candidateSupportsDisabledReasoning`：路由能否关闭思考运行。
fn supports_disabled_reasoning(candidate: &Value) -> bool {
    match s(candidate, "/capabilities/reasoning_control").as_str() {
        "fixed_off" => true,
        "toggle" | "effort_selection" | "provider_adaptive" => declares_effort(candidate, "none"),
        _ => false,
    }
}

/// Whether a candidate model declares one reasoning effort.
/// 候选模型是否声明某个思考档位。
fn declares_effort(candidate: &Value, effort: &str) -> bool {
    a(candidate, "/capabilities/reasoning_efforts")
        .iter()
        .any(|declared| declared == effort)
}

/// Mirrors `sessionTitleCandidates`: internal conversation routes, thinking-off models first.
/// 镜像 `sessionTitleCandidates`：内部对话路由，可关闭思考的模型在前。
fn session_title_candidates(ai: &Value) -> Vec<&Value> {
    let mut candidates: Vec<&Value> = a(ai, "/internalInferenceCandidates")
        .iter()
        .filter(|candidate| {
            b(candidate, "/enabled")
                && s(candidate, "/route/usage_scope") == "internal_background"
                && s(candidate, "/route/operation") == "conversation_respond"
                && s(candidate, "/route/binding/kind") == "protocol"
        })
        .collect();
    candidates.sort_by_key(|candidate| !supports_disabled_reasoning(candidate));
    candidates
}

/// Saves the picked title route; a model whose route is listed in `thinking_off` starts with
/// thinking off, as in the WebUI.
/// 保存所选标题路由；路由列在 `thinking_off` 中的模型以关闭思考开始，与 WebUI 一致。
fn select_session_title_route(thinking_off: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let route: Value = serde_json::from_str(value.as_str()).ok()?;
    let off = a(thinking_off, "")
        .iter()
        .any(|listed| listed == value.as_str());
    Some(settings::save_public_ai_inference_route(
        SESSION_TITLE_PROFILE,
        Some(SESSION_TITLE_PURPOSE),
        "conversation_respond",
        route,
        off.then_some("none"),
    ))
}

/// Saves the title route with the chosen effort; the default option saves none.
/// 以所选档位保存标题路由；默认选项不保存档位。
fn select_session_title_effort(route: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let effort = value.as_str();
    Some(settings::save_public_ai_inference_route(
        SESSION_TITLE_PROFILE,
        Some(SESSION_TITLE_PURPOSE),
        "conversation_respond",
        route.clone(),
        (effort != SESSION_TITLE_MODEL_DEFAULT_EFFORT).then_some(effort),
    ))
}

/// The session title switch, its model and its thinking level.
/// 会话标题开关、其模型与思考级别。
fn session_title_rows(rows: &mut Vec<Row>, wb: &Value, ai: &Value) {
    // Older snapshots without the field keep the enabled default.
    // 缺少该字段的旧快照保留默认开启。
    let enabled = v(wb, "/sessionTitleGenerationEnabled")
        .as_bool()
        .unwrap_or(true);
    rows.push(toggle(
        tr!("settings.sessionTitle.title"),
        enabled,
        Edit::Call(Setter::Build {
            ctx: Value::Null,
            build: |_, value| Some(settings::save_session_title_generation(value.as_bool())),
        }),
    ));
    rows.push(note(tr!("settings.sessionTitle.hint"), Tone::Muted));
    if !enabled {
        return;
    }
    let profile = v(ai, &format!("/inferenceProfiles/{SESSION_TITLE_PROFILE}"));
    let saved = v(profile, &format!("/llm_routes/{SESSION_TITLE_PURPOSE}"));
    let effort = v(
        profile,
        &format!("/llm_reasoning_efforts/{SESSION_TITLE_PURPOSE}"),
    );
    let candidates = session_title_candidates(ai);
    let selected = candidates
        .iter()
        .find(|candidate| v(candidate, "/route") == saved);
    rows.push(note(
        tr!("settings.sessionTitle.recommendation"),
        Tone::Muted,
    ));
    if !saved.is_null() && selected.is_none() {
        rows.push(note(tr!("settings.sessionTitle.unavailable"), Tone::Warn));
    }
    let mut options = vec![(
        "null".to_string(),
        tr!("settings.sessionTitle.sessionModel"),
    )];
    for candidate in &candidates {
        let mut label = format!(
            "{} / {} / {}",
            s(candidate, "/providerDisplayName"),
            s(candidate, "/connectionDisplayName"),
            s(candidate, "/modelDisplayName")
        );
        if supports_disabled_reasoning(candidate) {
            label.push_str(&format!(
                " · {}",
                tr!("settings.sessionTitle.canTurnOffThinking")
            ));
        }
        options.push((v(candidate, "/route").to_string(), label));
    }
    let thinking_off = candidates
        .iter()
        .filter(|candidate| declares_effort(candidate, "none"))
        .map(|candidate| v(candidate, "/route").to_string())
        .collect::<Vec<_>>();
    rows.push(choice(
        tr!("settings.sessionTitle.route"),
        options,
        saved.to_string(),
        Edit::Call(Setter::Build {
            ctx: json!(thinking_off),
            build: select_session_title_route,
        }),
    ));
    if selected.is_none() {
        rows.push(note(
            tr!("settings.sessionTitle.sessionModelHint"),
            Tone::Muted,
        ));
    }
    if let Some(candidate) = selected {
        let mut efforts = vec![(
            SESSION_TITLE_MODEL_DEFAULT_EFFORT.to_string(),
            tr!("settings.sessionTitle.reasoningDefault"),
        )];
        efforts.extend(
            a(candidate, "/capabilities/reasoning_efforts")
                .iter()
                .filter_map(Value::as_str)
                .map(|declared| {
                    (
                        declared.to_string(),
                        super::permission_judge::effort_label(declared),
                    )
                }),
        );
        rows.push(choice(
            tr!("settings.sessionTitle.reasoning"),
            efforts,
            effort
                .as_str()
                .unwrap_or(SESSION_TITLE_MODEL_DEFAULT_EFFORT)
                .to_string(),
            Edit::Call(Setter::Build {
                ctx: saved.clone(),
                build: select_session_title_effort,
            }),
        ));
    }
}

/// Builds the base system prompt editor for the user layer or one project.
/// 构建用户层级或单个项目的基础系统提示词编辑器。
/// The selected scope is the loaded snapshot's `projectId`; choosing another scope reloads the slot.
/// 所选作用域即已加载快照的 `projectId`；选择其他作用域会重新加载该槽位。
fn system_prompt(app: &App) -> Vec<Row> {
    // Snapshot.
    // 基础系统提示词层级快照。
    let snapshot = app.slots.value(Slot::SystemPrompt);
    // Project id; empty selects the user scope.
    // 项目标识；为空时选择用户作用域。
    let project = s(snapshot, "/projectId");
    let mut scopes = vec![(String::new(), tr!("settings.systemPrompt.scopeUser"))];
    scopes.extend(app.projects.iter().map(|p| {
        (
            p.id.clone(),
            tr!("settings.systemPrompt.scopeProject", name = p.name.as_str()),
        )
    }));
    let mut rows = vec![
        section(tr!("settings.modelExperience.systemPrompt")),
        note(tr!("settings.systemPrompt.description"), Tone::Muted),
        choice(
            tr!("settings.systemPrompt.scope"),
            scopes,
            project.clone(),
            Edit::Call(Setter::Build {
                ctx: Value::Null,
                build: |_, value| {
                    let id = value.as_str();
                    Some(settings::load_system_prompt_settings(
                        (!id.is_empty()).then_some(id),
                    ))
                },
            }),
        ),
    ];
    if snapshot.is_null() {
        return rows;
    }
    // Override of the selected scope; the editor's baseline is the text this scope inherits.
    // 所选作用域的覆盖；编辑器基线为该作用域继承的正文。
    let override_text = v(
        snapshot,
        if project.is_empty() {
            "/userText"
        } else {
            "/projectText"
        },
    );
    let baseline = [
        override_text,
        v(snapshot, "/userText"),
        v(snapshot, "/builtInText"),
    ]
    .into_iter()
    .find_map(Value::as_str)
    .unwrap_or_default();
    let source = match s(snapshot, "/effectiveSource").as_str() {
        "project" => tr!("settings.systemPrompt.sourceProject"),
        "user" => tr!("settings.systemPrompt.sourceUser"),
        _ => tr!("settings.systemPrompt.sourceBuiltIn"),
    };
    rows.push(note(source, Tone::Info));
    let editor = text(
        tr!("settings.systemPrompt.textLabel"),
        baseline.to_string(),
        Edit::Call(Setter::Build {
            ctx: json!({ "projectId": snapshot["projectId"], "baseline": baseline }),
            build: |ctx, value| {
                let project = ctx["projectId"].as_str();
                let text = value.as_str();
                if text.trim().is_empty() {
                    return None;
                }
                // An unchanged body stays inherited, so it only rereads the snapshot.
                // 未修改的正文保持继承，因此只重新读取快照。
                if text.trim() == ctx["baseline"].as_str().unwrap_or_default().trim() {
                    return Some(settings::load_system_prompt_settings(project));
                }
                let scope = if project.is_some() {
                    SystemPromptScope::Project
                } else {
                    SystemPromptScope::User
                };
                Some(settings::save_system_prompt(scope, project, text))
            },
        }),
    );
    // The empty-text hint points at the restore action, which exists only with an override.
    // 空正文提示指向恢复操作，仅在存在覆盖时才有该操作。
    rows.push(if override_text.is_null() {
        editor
    } else {
        editor.detail(tr!("settings.systemPrompt.empty"))
    });
    let path = s(
        snapshot,
        if project.is_empty() {
            "/userPath"
        } else {
            "/projectPath"
        },
    );
    if !path.is_empty() {
        rows.push(note(
            tr!("settings.systemPrompt.savedTo", path = path.as_str()),
            Tone::Muted,
        ));
    }
    if !override_text.is_null() {
        let (scope, label) = if project.is_empty() {
            (
                SystemPromptScope::User,
                tr!("settings.systemPrompt.resetUser"),
            )
        } else {
            (
                SystemPromptScope::Project,
                tr!("settings.systemPrompt.resetProject"),
            )
        };
        rows.push(button(
            label,
            call(settings::reset_system_prompt(
                scope,
                (!project.is_empty()).then_some(project.as_str()),
            )),
            Tone::Normal,
        ));
    }
    rows
}

/// Saves the full Git DTO after each field edit, retaining the unchanged sibling fields.
/// 每次字段编辑后保存完整 Git DTO，保留未变更的同级字段。
pub(super) fn git_commit(app: &App) -> Vec<Row> {
    // Git.
    // Git 配置与状态。
    let git = app.slots.at(Slot::Workbench, "/gitCommit");
    if git.is_null() {
        return Vec::new();
    }
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![
        text(
            tr!("tui.field.branchPrefix"),
            s(git, "/newBranchPrefix"),
            dto(
                git.clone(),
                "/newBranchPrefix",
                settings::save_git_commit_settings,
            ),
        ),
        text(
            tr!("tui.field.commitPrompt"),
            s(git, "/summaryExtraPrompt"),
            dto(
                git.clone(),
                "/summaryExtraPrompt",
                settings::save_git_commit_settings,
            ),
        ),
    ];
    // Options.
    // 允许的选择项。
    let mut options = vec![(
        json!({"mode":"session"}).to_string(),
        tr!("tui.field.followSession"),
    )];
    for model in a(
        app.slots.value(Slot::PublicAiSettings),
        "/modelAccessRecords",
    ) {
        options.push((json!({"mode":"fixed","providerId":v(model,"/providerId"),"modelId":v(model,"/modelId")}).to_string(),
            format!("{} / {}", s(model,"/providerId"), s(model,"/modelId"))));
    }
    rows.push(choice(
        tr!("tui.field.summaryModel"),
        options,
        v(git, "/summaryModel").to_string(),
        Edit::Call(Setter::Build {
            ctx: git.clone(),
            build: |context, value| {
                // Exact protocol request.
                // 协议请求文档。
                let mut request = context.clone();
                request["summaryModel"] = serde_json::from_str(value.as_str()).ok()?;
                Some(settings::save_git_commit_settings(request))
            },
        }),
    ));
    rows
}

/// Lists providers and their service-entry metadata from the public AI catalog.
/// 从公共 AI 目录列出供应商及其服务入口元数据。
pub(super) fn providers(app: &App) -> Vec<Row> {
    a(app.slots.value(Slot::PublicAiSettings), "/providers")
        .iter()
        .map(|provider| {
            item(
                &s(provider, "/id"),
                s(provider, "/display_name"),
                s(provider, "/category"),
            )
            .lines(details(provider))
        })
        .collect()
}

/// Sends token credentials once using the exact PublicAiConnectionAddRequest contract.
/// 按精确的 PublicAiConnectionAddRequest 协议单次发送令牌凭据。
pub(super) fn credentials(app: &App) -> Vec<Row> {
    // Public AI settings.
    // 公开 AI 设置。
    let ai = app.slots.value(Slot::PublicAiSettings);
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    for provider in a(ai, "/providers") {
        for entry in a(provider, "/service_entries") {
            for profile in a(entry, "/credential_profiles") {
                // Exact protocol request.
                // 协议请求文档。
                let request = json!({"providerId":v(provider,"/id"),"serviceEntryId":v(entry,"/id"),
                    "credentialProfileId":v(profile,"/id"),"credentialMode":v(profile,"/credential_mode"),
                    "displayName":"","secret":"","managementKey":"","serviceOriginOverride":""});
                rows.push(button(
                    format!(
                        "{} · {} / {} / {}",
                        tr!("tui.field.addConnection"),
                        s(provider, "/display_name"),
                        s(entry, "/display_name"),
                        s(profile, "/display_name")
                    ),
                    form(
                        request,
                        vec![
                            field(tr!("tui.field.displayName"), "/displayName", false, false),
                            field(tr!("tui.field.secret"), "/secret", true, false),
                            field(tr!("tui.field.managementKey"), "/managementKey", true, true),
                            field(
                                tr!("tui.field.serviceOrigin"),
                                "/serviceOriginOverride",
                                false,
                                true,
                            ),
                        ],
                        settings::add_public_ai_connection,
                    ),
                    Tone::Accent,
                ));
            }
        }
    }
    for connection in a(ai, "/connections") {
        // Exact entity identity.
        // 准确实体身份。
        let id = s(connection, "/id");
        // Provider.
        // 提供方身份或元数据。
        let provider = s(connection, "/provider_id");
        rows.push(toggle(
            s(connection, "/display_name"),
            b(connection, "/enabled"),
            dto(
                json!({"id":id,"enabled":v(connection,"/enabled")}),
                "/enabled",
                |request| {
                    settings::save_public_ai_connection_enabled(
                        &s(&request, "/id"),
                        b(&request, "/enabled"),
                    )
                },
            ),
        ));
        rows.push(
            item(
                &id,
                s(connection, "/display_name"),
                format!("{} · {}", provider, s(connection, "/credential_profile_id")),
            )
            .lines(vec![tr!("tui.field.secretMasked")])
            .actions(vec![
                action(
                    tr!("tui.field.sync"),
                    call(settings::sync_public_ai_connection(&id)),
                    Tone::Normal,
                ),
                action(
                    tr!("tui.field.setDefault"),
                    call(settings::save_public_ai_default_connection(&provider, &id)),
                    Tone::Accent,
                ),
                action(
                    tr!("tui.field.remove"),
                    connection_removal(ai, connection),
                    Tone::Bad,
                ),
            ]),
        );
    }
    rows
}

/// Reviews a default connection's same-provider replacements before one atomic removal.
/// 在一次原子移除前审查默认连接的同供应商替代项。
fn connection_removal(ai: &Value, connection: &Value) -> UiAction {
    // Exact entity identity.
    // 准确实体身份。
    let id = s(connection, "/id");
    // Provider.
    // 提供方身份或元数据。
    let provider = s(connection, "/provider_id");
    // Replacements.
    // 符合服务准入规则的替代默认连接。
    let replacements: Vec<_> = a(ai, "/connections")
        .iter()
        .filter(|other| {
            // The service admits only enabled, validated connections as replacement defaults.
            // 服务仅准入已启用且已验证的连接作为替代默认连接。
            s(other, "/provider_id") == provider
                && s(other, "/id") != id
                && b(other, "/enabled")
                && !b(other, "/pending_validation")
        })
        .collect();
    if v(ai, "/providerDefaults")
        .get(&provider)
        .and_then(Value::as_str)
        != Some(&id)
        || replacements.is_empty()
    {
        return confirm(
            tr!("tui.field.remove"),
            s(connection, "/display_name"),
            tr!("tui.field.remove"),
            call(settings::remove_public_ai_connection(&id, None)),
        );
    }
    // Sheet.
    // 可滚动详情面板。
    let mut sheet =
        super::super::overlay::DetailSheet::new(tr!("settings.credentials.deleteTitle"))
            .field(tr!("tui.field.displayName"), s(connection, "/display_name"))
            .section(
                tr!("settings.credentials.replacementDefault"),
                tr!("settings.credentials.deleteDefaultMissingReplacement"),
            );
    for replacement in replacements {
        // Replacement id.
        // 所选替代默认连接身份。
        let replacement_id = s(replacement, "/id");
        // Label.
        // 当前显示文案。
        let label = s(replacement, "/display_name");
        sheet = sheet.action(
            label.clone(),
            true,
            confirm(
                tr!("tui.field.remove"),
                format!("{} → {label}", s(connection, "/display_name")),
                tr!("tui.field.confirm"),
                call(settings::remove_public_ai_connection(
                    &id,
                    Some(&replacement_id),
                )),
            ),
        );
    }
    UiAction::Detail(Box::new(sheet))
}

/// Presents service-published capability targets and exact connection/target selections.
/// 显示服务发布的能力目标及精确的连接与目标选择。
pub(super) fn capabilities(app: &App) -> Vec<Row> {
    // Public AI settings.
    // 公开 AI 设置。
    let ai = app.slots.value(Slot::PublicAiSettings);
    // Capabilities.
    // 已声明能力集合。
    let mut capabilities = std::collections::BTreeSet::new();
    for target in a(ai, "/providerToolTargets") {
        capabilities.insert(s(target, "/target/contract/capability"));
    }
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    for capability in capabilities {
        // Enabled.
        // 明确启用状态。
        let enabled = a(ai, "/providerToolEnabledCapabilities")
            .iter()
            .any(|entry| entry.as_str() == Some(&capability));
        rows.push(toggle(
            capability.clone(),
            enabled,
            dto(
                json!({"capability":capability,"enabled":enabled}),
                "/enabled",
                |request| {
                    settings::save_public_ai_provider_tool_capability_enabled(
                        &s(&request, "/capability"),
                        b(&request, "/enabled"),
                    )
                },
            ),
        ));
        // Options.
        // 允许的选择项。
        let mut options = vec![("null".into(), tr!("tui.field.default"))];
        for target in a(ai, "/providerToolTargets")
            .iter()
            .filter(|target| s(target, "/target/contract/capability") == capability)
        {
            options.push((json!({"connection_id":v(target,"/connectionId"),"target_id":v(target,"/target/target_id")}).to_string(),
                format!("{} / {}",s(target,"/connectionDisplayName"),s(target,"/target/display_name"))));
        }
        rows.push(choice(
            capability.clone(),
            options,
            v(ai, &format!("/providerToolSelections/{capability}")).to_string(),
            Edit::Call(Setter::Build {
                ctx: json!(capability),
                build: |context, value| {
                    Some(settings::save_public_ai_provider_tool_selection(
                        context.as_str()?,
                        serde_json::from_str(value.as_str()).ok()?,
                    ))
                },
            }),
        ));
    }
    rows.extend(provider_tasks(app));
    rows
}

/// Lists retained provider asynchronous tasks with their phase and cancellation, as the WebUI tasks tab does.
/// 与 WebUI 任务页一致，列出保留的供应商异步任务及其阶段与取消操作。
fn provider_tasks(app: &App) -> Vec<Row> {
    let mut rows = vec![section(tr!("settings.providerTasks.tab"))];
    let tasks = a(app.slots.value(Slot::ProviderTasks), "");
    if app.slots.loaded(Slot::ProviderTasks) && tasks.is_empty() {
        rows.push(note(tr!("settings.providerTasks.empty"), Tone::Muted));
    }
    for task in tasks {
        // Stable task identity used by status and cancellation calls.
        // 状态查询与取消调用使用的稳定任务标识。
        let id = s(task, "/executionId");
        let title = task
            .get("capability")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| tr!("settings.providerTasks.untitled"));
        let target = task
            .get("targetDisplayName")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| s(task, "/targetId"));
        let started = tr!(
            "settings.providerTasks.started",
            time = utc_time(n(task, "/createdAtUnixMs"))
        );
        let phase = s(task, "/phase");
        let terminal = b(task, "/terminal");
        // The error explains a failure; otherwise a live task shows its deadline.
        // 错误说明失败原因；否则进行中的任务显示截止时间。
        let status = match task.get("error").and_then(Value::as_str) {
            Some(error) => error.to_string(),
            None if !terminal => tr!(
                "settings.providerTasks.deadline",
                time = utc_time(n(task, "/deadlineUnixMs"))
            ),
            None => String::new(),
        };
        let tone = match phase.as_str() {
            "completed" => Tone::Good,
            "failed" | "dispatch_outcome_unknown" => Tone::Bad,
            "cancelled" => Tone::Muted,
            _ => Tone::Info,
        };
        let mut actions = Vec::new();
        if !terminal && b(task, "/cancellationSupported") && phase != "cancellation_requested" {
            actions.push(action(
                tr!("settings.providerTasks.cancel"),
                call(settings::cancel_provider_task(
                    &id,
                    task.get("ownerSessionId").and_then(Value::as_str),
                )),
                Tone::Bad,
            ));
        }
        rows.push(
            item(&id, title, format!("{target} · {started}"))
                .badge(provider_task_phase_label(&phase), tone)
                .lines(if status.is_empty() {
                    Vec::new()
                } else {
                    vec![status]
                })
                .actions(actions),
        );
    }
    rows
}

/// Names one durable provider-task phase in the active language.
/// 以当前语言命名一个持久供应商任务阶段。
fn provider_task_phase_label(phase: &str) -> String {
    match phase {
        "dispatching" => tr!("settings.providerTasks.phase.dispatching"),
        "queued" => tr!("settings.providerTasks.phase.queued"),
        "running" => tr!("settings.providerTasks.phase.running"),
        "cancellation_requested" => tr!("settings.providerTasks.phase.cancellationRequested"),
        "completed" => tr!("settings.providerTasks.phase.completed"),
        "failed" => tr!("settings.providerTasks.phase.failed"),
        "cancelled" => tr!("settings.providerTasks.phase.cancelled"),
        "dispatch_outcome_unknown" => tr!("settings.providerTasks.phase.dispatchOutcomeUnknown"),
        other => other.to_string(),
    }
}

/// Formats Unix milliseconds as a UTC minute, or `-` when the value is out of range.
/// 将 Unix 毫秒格式化为 UTC 分钟；超出范围时为 `-`。
pub(super) fn utc_time(unix_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(unix_ms)
        .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| String::from("-"))
}

/// Initializes the full HTTP server DTO using the upstream createMcpDraft defaults.
/// 使用上游 createMcpDraft 默认值初始化完整 HTTP 服务器 DTO。
fn mcp_http_request() -> Value {
    json!({"serverId":null,"name":"","enabled":true,"transport":"http","command":"","args":[],
        "url":"","bearerToken":"","headers":[],"headersFromEnv":[],"oauthEnabled":false,"oauthClientId":"",
        "oauthClientSecret":"","oauthScopes":[],"oauthRedirectUri":"","cwd":"","env":[],"envPassthrough":[]})
}

/// Copies every required persisted MCP field into its exact save request.
/// 将全部必需的持久化 MCP 字段复制到准确保存请求。
fn mcp_edit_request(server: &Value) -> Option<Value> {
    // Exact protocol request.
    // 协议请求文档。
    let mut request = serde_json::Map::new();
    request.insert("serverId".into(), server.get("id")?.clone());
    for key in [
        "name",
        "enabled",
        "transport",
        "command",
        "args",
        "url",
        "bearerToken",
        "headers",
        "headersFromEnv",
        "oauthEnabled",
        "oauthClientId",
        "oauthClientSecret",
        "oauthScopes",
        "oauthRedirectUri",
        "cwd",
        "env",
        "envPassthrough",
    ] {
        request.insert(key.into(), server.get(key)?.clone());
    }
    Some(Value::Object(request))
}

/// Collects complete stdio/HTTP/SSE configuration; credentials exist only inside the form.
/// 收集完整 stdio、HTTP 或 SSE 配置；凭据仅存在于临时表单内。
fn mcp_form(request: Value) -> UiAction {
    // Fields.
    // 顺序输入字段。
    let mut fields = vec![
        field(tr!("tui.field.displayName"), "/name", false, false),
        super::super::form::FormField {
            title: tr!("tui.field.enabledInput"),
            pointer: "/enabled".into(),
            secret: false,
            optional: false,
            validate: |_, _| true,
            parse: |input| match input {
                "true" => Some(json!(true)),
                "false" => Some(json!(false)),
                _ => None,
            },
        },
    ];
    if s(&request, "/transport") == "stdio" {
        fields.extend([
            field(tr!("tui.field.executable"), "/command", false, false),
            parsed_field(tr!("tui.field.arguments"), "/args", false, parse_lines),
            field(tr!("tui.field.workingDirectory"), "/cwd", false, true),
            parsed_field(tr!("tui.field.environment"), "/env", true, parse_pairs),
            parsed_field(
                tr!("tui.field.envPassthrough"),
                "/envPassthrough",
                false,
                parse_lines,
            ),
        ]);
    } else {
        fields.extend([
            super::super::form::FormField {
                title: tr!("tui.field.endpoint"),
                pointer: "/url".into(),
                secret: false,
                optional: false,
                validate: |_, _| true,
                parse: |input| {
                    // Endpoint.
                    // 已认证传输端点。
                    let endpoint: tungstenite::http::Uri = input.parse().ok()?;
                    (matches!(endpoint.scheme_str(), Some("http" | "https"))
                        && endpoint.host().is_some())
                    .then(|| json!(input))
                },
            },
            field(tr!("tui.field.secret"), "/bearerToken", true, true),
            parsed_field(tr!("tui.field.headers"), "/headers", true, parse_pairs),
            parsed_field(
                tr!("tui.field.headersFromEnv"),
                "/headersFromEnv",
                false,
                parse_pairs,
            ),
            super::super::form::FormField {
                title: tr!("tui.field.oauthEnabledInput"),
                pointer: "/oauthEnabled".into(),
                secret: false,
                optional: false,
                validate: |_, _| true,
                parse: |input| match input {
                    "true" => Some(json!(true)),
                    "false" => Some(json!(false)),
                    _ => None,
                },
            },
            field(tr!("tui.field.oauthClient"), "/oauthClientId", false, true),
            field(
                tr!("tui.field.oauthSecret"),
                "/oauthClientSecret",
                true,
                true,
            ),
            parsed_field(
                tr!("tui.field.oauthScopes"),
                "/oauthScopes",
                false,
                parse_lines,
            ),
            field(
                tr!("tui.field.oauthRedirect"),
                "/oauthRedirectUri",
                false,
                true,
            ),
        ]);
    }
    form(request, fields, |mut request| {
        // OAuth fields apply only when sign-in is enabled, matching the WebUI dialog.
        // 仅在启用登录时应用 OAuth 字段，与 WebUI 对话框一致。
        if request.get("oauthEnabled").and_then(Value::as_bool) != Some(true) {
            request["oauthClientId"] = json!("");
            request["oauthClientSecret"] = json!("");
            request["oauthScopes"] = json!([]);
            request["oauthRedirectUri"] = json!("");
        }
        settings::save_mcp_server_configuration(request)
    })
}

/// Describes the stored credential state of one MCP server, or why it is unknown.
/// 描述单个 MCP 服务器已存储凭据的状态，或其未知的原因。
fn mcp_auth_status_text(status: &Value) -> String {
    match status.get("authStatus").and_then(Value::as_str) {
        Some("authenticated") => tr!("settings.mcp.oauth.statusAuthenticated"),
        Some("expired") => tr!("settings.mcp.oauth.statusExpired"),
        Some("not_authenticated") => tr!("settings.mcp.oauth.statusNotAuthenticated"),
        Some("not_configured") => tr!("settings.mcp.oauth.statusNotConfigured"),
        _ => tr!(
            "settings.mcp.oauth.statusUnknown",
            error = s(status, "/statusError")
        ),
    }
}

/// Signs the opened MCP server in or out; tokens never reach the TUI, only status codes.
/// 为已打开的 MCP 服务器登录或退出；令牌从不到达 TUI，只有状态代码。
fn mcp_oauth(status: &Value) -> Row {
    let server_id = s(status, "/serverId");
    let flow = status.get("flow").filter(|flow| flow.is_object());
    let phase = flow.map(|flow| s(flow, "/phase")).unwrap_or_default();
    let flow_active = matches!(phase.as_str(), "awaiting_callback" | "exchanging");
    let mut lines = Vec::new();
    let mut actions = Vec::new();
    if let Some(flow) = flow {
        let flow_id = s(flow, "/flowId");
        let manual = s(flow, "/callbackMode") == "manual";
        match phase.as_str() {
            "awaiting_callback" | "exchanging" => {
                lines.push(if phase == "exchanging" {
                    tr!("settings.mcp.oauth.exchanging")
                } else {
                    tr!("settings.mcp.oauth.waitingForBrowser")
                });
                lines.push(s(flow, "/authorizationUrl"));
            }
            "failed" => lines.push(tr!("settings.mcp.oauth.failed", error = s(flow, "/error"))),
            "expired" => lines.push(tr!("settings.mcp.oauth.expired")),
            _ => {}
        }
        if phase == "awaiting_callback" {
            if manual {
                lines.push(tr!("settings.mcp.oauth.manualHint"));
                actions.push(action(
                    tr!("settings.mcp.oauth.finish"),
                    form(
                        json!({"serverId": server_id, "flowId": flow_id, "callbackUrl": ""}),
                        vec![field(
                            tr!("settings.mcp.oauth.callbackUrl"),
                            "/callbackUrl",
                            false,
                            false,
                        )],
                        api::mcp_settings::finish_mcp_oauth,
                    ),
                    Tone::Accent,
                ));
            }
            actions.push(action(
                tr!("settings.mcp.oauth.openPage"),
                call(api::project_actions::open_external_url(&s(
                    flow,
                    "/authorizationUrl",
                ))),
                Tone::Normal,
            ));
            actions.push(action(
                tr!("settings.mcp.oauth.cancel"),
                call(api::mcp_settings::cancel_mcp_oauth(&server_id, &flow_id)),
                Tone::Bad,
            ));
        }
    }
    let auth_status = status.get("authStatus").and_then(Value::as_str);
    if !flow_active && auth_status != Some("not_configured") {
        let signed_in = matches!(auth_status, Some("authenticated" | "expired"));
        actions.push(action(
            if signed_in {
                tr!("settings.mcp.oauth.signInAgain")
            } else {
                tr!("settings.mcp.oauth.signIn")
            },
            call(api::mcp_settings::start_mcp_oauth(&server_id)),
            Tone::Accent,
        ));
        // Sign-out also clears credentials whose status is unknown, for example while the server is stopped.
        // 退出登录也会清除状态未知的凭据，例如服务器已停止时。
        if signed_in || auth_status.is_none() {
            actions.push(action(
                tr!("settings.mcp.oauth.signOut"),
                call(api::mcp_settings::remove_mcp_oauth(&server_id)),
                Tone::Bad,
            ));
        }
    }
    item(
        &format!("mcp-oauth-{server_id}"),
        format!("{} · {server_id}", tr!("settings.mcp.oauthSignIn")),
        mcp_auth_status_text(status),
    )
    .lines(lines)
    .actions(actions)
}

/// Displays runtime schemas and real MCP configuration actions.
/// 显示运行时 Schema 及真实 MCP 配置动作。
pub(super) fn mcp(app: &App) -> Vec<Row> {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![note(tr!("tui.field.mcpAuthReentry"), Tone::Muted)];
    for transport in ["stdio", "http", "sse"] {
        // Exact protocol request.
        // 协议请求文档。
        let mut request = mcp_http_request();
        request["transport"] = json!(transport);
        rows.push(button(
            format!("{} · {transport}", tr!("tui.field.addMcp")),
            mcp_form(request),
            Tone::Accent,
        ));
    }
    // Servers.
    // MCP 服务器集合。
    let servers = a(app.slots.value(Slot::Workbench), "/mcpServers");
    for inspection in a(app.slots.value(Slot::McpRuntime), "") {
        // Exact entity identity.
        // 准确实体身份。
        let id = s(inspection, "/state/id");
        // Actions.
        // 声明的操作集合。
        let mut actions = vec![action(
            tr!("tui.field.capabilities"),
            call(api::mcp_settings::load_mcp_server_capabilities(&id)),
            Tone::Normal,
        )];
        // Persisted configuration of a user-defined server.
        // 用户定义服务器的持久化配置。
        let server = servers
            .iter()
            .find(|server| v(server, "/id").as_str() == Some(id.as_str()));
        if server.is_some_and(|server| b(server, "/oauthEnabled"))
            || s(inspection, "/state/status") == "awaiting_authorization"
        {
            actions.push(action(
                tr!("settings.mcp.oauthSignIn"),
                call(api::mcp_settings::load_mcp_oauth_status(&id)),
                Tone::Normal,
            ));
        }
        if s(inspection, "/state/source") == "user" {
            if let Some(request) = server.and_then(mcp_edit_request) {
                actions.push(action(
                    tr!("tui.field.edit"),
                    mcp_form(request),
                    Tone::Normal,
                ));
            }
            actions.push(action(
                tr!("tui.field.remove"),
                confirm(
                    tr!("tui.field.remove"),
                    s(inspection, "/state/name"),
                    tr!("tui.field.remove"),
                    call(settings::remove_mcp_server_configuration(&id)),
                ),
                Tone::Bad,
            ));
        }
        rows.push(
            item(
                &id,
                s(inspection, "/state/name"),
                s(inspection, "/state/status"),
            )
            .lines(details(inspection))
            .actions(actions),
        );
    }
    if app.slots.loaded(Slot::McpOAuth) {
        rows.push(mcp_oauth(app.slots.value(Slot::McpOAuth)));
    }
    if app.slots.loaded(Slot::McpCapabilities) {
        rows.push(
            item("mcp-capabilities", tr!("tui.field.capabilities"), "")
                .lines(details(app.slots.value(Slot::McpCapabilities))),
        );
    }
    rows
}

/// Creates archived conversation restore/delete actions from service identifiers.
/// 根据服务标识创建归档会话恢复与删除动作。
pub(super) fn archived(app: &App) -> Vec<Row> {
    a(app.slots.value(Slot::Workbench), "/archivedConversations")
        .iter()
        .map(|conversation| {
            // Exact entity identity.
            // 准确实体身份。
            let id = s(conversation, "/id");
            item(&id, s(conversation, "/title"), "").actions(vec![
                action(
                    tr!("tui.field.restore"),
                    call(api::project_actions::restore_archived_conversations(
                        std::slice::from_ref(&id),
                    )),
                    Tone::Good,
                ),
                action(
                    tr!("tui.field.remove"),
                    confirm(
                        tr!("tui.field.remove"),
                        s(conversation, "/title"),
                        tr!("tui.field.remove"),
                        call(api::project_actions::delete_archived_conversations(
                            std::slice::from_ref(&id),
                        )),
                    ),
                    Tone::Bad,
                ),
            ])
        })
        .collect()
}

/// Reads diagnostic modules/logs and saves complete provider retry and trace settings.
/// 读取诊断模块与日志，并保存完整供应商重试及追踪设置。
pub(super) fn diagnostics(app: &App) -> Vec<Row> {
    // Authoritative workbench.
    // 工作台权威状态。
    let wb = app.slots.value(Slot::Workbench);
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![
        toggle(
            tr!("tui.field.requestTrace"),
            b(wb, "/providerRequestTraceEnabled"),
            Edit::Call(Setter::Build {
                ctx: Value::Null,
                build: |_, value| {
                    Some(settings::save_provider_request_trace_settings(
                        value.as_bool(),
                        None,
                    ))
                },
            }),
        ),
        toggle(
            tr!("settings.diagnostics.providerRequestTrace.errorsSwitch"),
            b(wb, "/providerErrorTraceEnabled"),
            Edit::Call(Setter::Build {
                ctx: json!({"enabled": b(wb, "/providerRequestTraceEnabled")}),
                build: |ctx, value| {
                    Some(settings::save_provider_request_trace_settings(
                        b(ctx, "/enabled"),
                        Some(value.as_bool()),
                    ))
                },
            }),
        ),
        button(
            tr!("tui.field.openTrace"),
            call(settings::open_selected_provider_api_trace_directory()),
            Tone::Normal,
        ),
        button(
            tr!("tui.field.clearTrace"),
            confirm(
                tr!("tui.field.clearTrace"),
                String::new(),
                tr!("tui.field.clearTrace"),
                call(settings::clear_selected_provider_api_trace_directory()),
            ),
            Tone::Bad,
        ),
    ];
    // Retry.
    // 服务重试配置组。
    let retry = v(wb, "/providerRetry");
    if retry.is_object() {
        rows.push(toggle(
            tr!("tui.field.retryEnabled"),
            b(retry, "/enabled"),
            dto(
                retry.clone(),
                "/enabled",
                settings::save_provider_retry_settings,
            ),
        ));
        for (pointer, label) in [
            ("/maxRetries", tr!("tui.field.retryCount")),
            ("/initialDelayMs", tr!("tui.field.retryInitial")),
            ("/maxDelayMs", tr!("tui.field.retryMax")),
            ("/backoffFactorMilli", tr!("tui.field.retryFactor")),
            ("/jitterPercent", tr!("tui.field.retryJitter")),
            ("/rateLimit/maxRetries", tr!("tui.field.rateCount")),
            ("/rateLimit/initialDelayMs", tr!("tui.field.rateInitial")),
            ("/rateLimit/maxDelayMs", tr!("tui.field.rateMax")),
            ("/rateLimit/backoffFactorMilli", tr!("tui.field.rateFactor")),
        ] {
            rows.push(number(
                label,
                n(retry, pointer),
                (0, i32::MAX.into(), 1),
                dto(
                    retry.clone(),
                    pointer,
                    settings::save_provider_retry_settings,
                ),
            ));
        }
        rows.push(toggle(
            tr!("tui.field.retryAfter"),
            b(retry, "/respectRetryAfter"),
            dto(
                retry.clone(),
                "/respectRetryAfter",
                settings::save_provider_retry_settings,
            ),
        ));
        rows.push(toggle(
            tr!("tui.field.retry429"),
            b(retry, "/retry429"),
            dto(
                retry.clone(),
                "/retry429",
                settings::save_provider_retry_settings,
            ),
        ));
        rows.push(string_list(
            tr!("settings.retry.retryWhenTheError"),
            retry,
            "/retryKeywords",
            settings::save_provider_retry_settings,
        ));
        // restored copies the service-projected default list into the complete retry DTO.
        // restored 将服务投影的默认列表复制进完整重试 DTO。
        let mut restored = retry.clone();
        restored["retryKeywords"] = v(retry, "/defaultRetryKeywords").clone();
        rows.push(button(
            tr!("settings.retry.restoreDefaultKeywords"),
            call(settings::save_provider_retry_settings(restored)),
            Tone::Normal,
        ));
    }
    rows.push(section(tr!("tui.field.modules")));
    for (index, module) in a(wb, "/modules").iter().enumerate() {
        rows.push(
            item(
                &format!("module-{index}"),
                s(module, "/label"),
                s(module, "/status"),
            )
            .lines(details(module)),
        );
    }
    rows.push(section(tr!("tui.field.logs")));
    for (index, log) in a(wb, "/logs").iter().enumerate() {
        rows.push(item(&format!("log-{index}"), s(log, "/message"), "").lines(details(log)));
    }
    rows
}

/// Pages through the release notes one release at a time in the interface language, opening on
/// the running version, as the WebUI changelog area does.
/// 以界面语言逐个发布翻阅更新记录，首先打开正在运行的版本，与 WebUI 更新记录区域一致。
pub(super) fn changelog(app: &App) -> Vec<Row> {
    let releases = vulcan_i18n::RELEASES;
    let running = s(app.slots.value(Slot::Workbench), "/appVersion");
    // at is the shown release: the picked one, else the running version, else the newest.
    // at 为显示的发布：已选的发布，否则为运行版本，否则为最新发布。
    let at = [app.changelog_release.as_deref(), Some(running.as_str())]
        .into_iter()
        .flatten()
        .find_map(|version| {
            releases
                .iter()
                .position(|release| release.version == version)
        })
        .unwrap_or(0);
    let release = &releases[at];
    let turn = |label: String, to: Option<usize>| RowAction {
        label,
        action: to.map(|index| UiAction::ChangelogRelease(releases[index].version.to_string())),
        tone: Tone::Normal,
        switch: None,
    };
    let pager = vec![
        turn(tr!("settings.changelog.newer"), at.checked_sub(1)),
        turn(
            tr!("settings.changelog.older"),
            (at + 1 < releases.len()).then_some(at + 1),
        ),
        action(
            tr!("settings.changelog.version"),
            UiAction::Pick {
                title: tr!("settings.changelog.version"),
                edit: Edit::Local(LocalSetting::ChangelogRelease),
                options: releases
                    .iter()
                    .map(|release| (release.version.to_string(), release.version.to_string()))
                    .collect(),
                current: release.version.to_string(),
            },
            Tone::Normal,
        ),
    ];
    let date = release
        .date
        .map_or_else(|| tr!("settings.changelog.unreleased"), str::to_string);
    let mut heading = item(
        "changelog-release",
        release.version,
        format!("{date} · {} / {}", at + 1, releases.len()),
    )
    .actions(pager.clone());
    if release.version == running {
        heading = heading.badge(tr!("settings.changelog.current"), Tone::Good);
    }
    let mut rows = vec![heading];
    // Every registered language ships notes (vulcan-i18n's build script requires it).
    // 每种注册语言都附有说明（vulcan-i18n 构建脚本强制要求）。
    match release.notes(crate::i18n::active_locale()) {
        Some(notes) => rows.push(Row::new(
            RowKind::Markdown {
                content: notes.to_string(),
            },
            "",
        )),
        None => rows.push(note(tr!("settings.changelog.loadError"), Tone::Bad)),
    }
    if releases.len() > 1 {
        rows.push(item("changelog-pager", "", "").actions(pager));
    }
    rows
}

/// Shows the service version and licenses using catalog-owned ids and source digests.
/// 使用目录持有的标识与来源摘要显示服务版本及许可证。
pub(super) fn about(app: &App) -> Vec<Row> {
    // Catalog.
    // 服务发布的目录。
    let catalog = app.slots.value(Slot::Licenses);
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = vec![
        Row::new(RowKind::Brand, ""),
        info(
            "VulcanCode".into(),
            s(app.slots.value(Slot::Workbench), "/appVersion"),
            Tone::Normal,
        ),
    ];
    for license in a(catalog, "/licenses") {
        for section in a(license, "/sections") {
            rows.push(
                item(&s(section, "/id"), s(license, "/title"), "")
                    .lines(details(section))
                    .actions(vec![action(
                        tr!("tui.field.view"),
                        call(api::licenses::load_license_body(
                            &s(section, "/id"),
                            &s(catalog, "/source_sha256"),
                        )),
                        Tone::Normal,
                    )]),
            );
        }
    }
    if app.slots.loaded(Slot::LicenseBody) {
        rows.push(note(
            s(app.slots.value(Slot::LicenseBody), "/text"),
            Tone::Normal,
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    // A picked model that declares `none` saves with thinking off; the session-model choice clears the route.
    // 所选模型声明 `none` 时以关闭思考保存；选择会话模型时清除路由。
    #[test]
    fn title_route_starts_with_thinking_off_when_the_model_allows() {
        let quiet = json!({"model_id": "quiet"}).to_string();
        let context = json!([quiet]);
        let off = select_session_title_route(&context, &SettingValue::Choice(quiet)).unwrap();
        assert_eq!(off.param("/request/reasoningEffort"), "none");
        assert_eq!(off.param("/request/purposeId"), SESSION_TITLE_PURPOSE);
        let thinking = select_session_title_route(
            &context,
            &SettingValue::Choice(json!({"model_id": "thinking"}).to_string()),
        )
        .unwrap();
        assert!(thinking.param("/request/reasoningEffort").is_null());
        let cleared =
            select_session_title_route(&context, &SettingValue::Choice("null".into())).unwrap();
        assert!(cleared.param("/request/route").is_null());
        assert!(cleared.param("/request/reasoningEffort").is_null());
    }

    // Routes that can turn thinking off are offered first, and only internal routes are offered.
    // 可关闭思考的路由排在前面，且只提供内部路由。
    #[test]
    fn title_candidates_put_thinking_off_models_first() {
        let route = |model: &str, scope: &str| {
            json!({"model_id": model, "usage_scope": scope, "operation": "conversation_respond",
                "binding": {"kind": "protocol"}})
        };
        let ai = json!({"internalInferenceCandidates": [
            {"enabled": true, "route": route("thinking", "internal_background"),
                "capabilities": {"reasoning_control": "effort_selection", "reasoning_efforts": ["low"]}},
            {"enabled": true, "route": route("quiet", "internal_background"),
                "capabilities": {"reasoning_control": "effort_selection", "reasoning_efforts": ["none", "low"]}},
            {"enabled": true, "route": route("foreground", "foreground"),
                "capabilities": {"reasoning_control": "fixed_off"}},
        ]});
        let models: Vec<String> = session_title_candidates(&ai)
            .iter()
            .map(|candidate| s(candidate, "/route/model_id"))
            .collect();
        assert_eq!(models, ["quiet", "thinking"]);
    }
}
