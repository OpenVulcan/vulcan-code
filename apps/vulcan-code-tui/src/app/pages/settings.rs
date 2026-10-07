// Settings pages reading the WebUI settings and workbench DTOs.
// 读取 WebUI 设置与工作台 DTO 的设置页面。

use super::*;
use crate::app::action::LocalSetting;
use crate::i18n;
use api::settings_actions as settings;

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
    rows
}

/// Initializes the full HTTP server DTO using the upstream createMcpDraft defaults.
/// 使用上游 createMcpDraft 默认值初始化完整 HTTP 服务器 DTO。
fn mcp_http_request() -> Value {
    json!({"serverId":null,"name":"","enabled":true,"transport":"http","command":"","args":[],
        "url":"","bearerToken":"","headers":[],"headersFromEnv":[],"oauthClientId":"",
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
    form(request, fields, settings::save_mcp_server_configuration)
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
        if s(inspection, "/state/source") == "user" {
            if let Some(server) = servers
                .iter()
                .find(|server| v(server, "/id").as_str() == Some(id.as_str()))
                && let Some(request) = mcp_edit_request(server)
            {
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
