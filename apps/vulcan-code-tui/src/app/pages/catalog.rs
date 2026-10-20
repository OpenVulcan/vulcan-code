// Agent and skill catalogs use the source-aware WebUI DTOs and permissions.
// 代理与技能目录使用 WebUI 的来源 DTO 与权限声明。

use super::*;
use api::skills_actions as skills;

/// Builds local enabled/disabled filters, optionally including path-associated skill issues.
/// 构建本地启用、停用筛选，可选包含与路径关联的技能问题。
/// `issues` enables the skill-only issue entry; returns protocol keys and translated labels.
/// `issues` 启用仅技能可用的问题条目；返回协议键与翻译标签。
fn status_options(issues: bool) -> Vec<(String, String)> {
    // Common status choices retain the WebUI filter keys.
    // 共用状态选项保留 WebUI 筛选键。
    let mut options = vec![
        ("all".into(), tr!("tui.catalog.allStatus")),
        ("enabled".into(), tr!("tui.catalog.enabled")),
        ("disabled".into(), tr!("tui.catalog.disabled")),
    ];
    if issues {
        options.push(("issues".into(), tr!("tui.catalog.issues")));
    }
    options
}

/// Matches a local keyword against verified default and every declared localized metadata field.
/// 将本地关键字与已确认默认字段及所有已声明本地化元数据字段匹配。
/// `record` owns metadata; `fields` are JSON pointers; `localized_fields` are locale keys.
/// `record` 持有元数据；`fields` 为 JSON 指针；`localized_fields` 为语言键。
/// `query` is trimmed and case-insensitive; an empty query includes every record.
/// `query` 去除首尾空格且不区分大小写；空查询包含每条记录。
fn matches_query(record: &Value, fields: &[&str], localized_fields: &[&str], query: &str) -> bool {
    // Every whitespace-separated token must match the verified default or localized metadata.
    // 每个空白分隔的词均须匹配已确认默认或本地化元数据。
    let tokens: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    // One searchable text preserves the WebUI's cross-field, cross-locale AND semantics.
    // 单个可搜索文本保留 WebUI 跨字段、跨语言的与匹配语义。
    let mut text = fields
        .iter()
        .map(|pointer| s(record, pointer))
        .collect::<Vec<_>>();
    if let Some(locales) = record.get("locales").and_then(Value::as_object) {
        for locale in locales.values() {
            text.extend(
                localized_fields
                    .iter()
                    .filter_map(|field| locale.get(field).and_then(Value::as_str))
                    .map(str::to_string),
            );
        }
    }
    // Case folding affects search only and never service identities.
    // 大小写转换仅影响搜索，不影响服务身份。
    let text = text.join("\n").to_lowercase();
    tokens.iter().all(|token| text.contains(token))
}

/// Returns the localized source-layer caption for an upstream scopeKey.
/// 返回上游 scopeKey 对应的本地化来源层标题。
fn layer_label(scope: &str) -> String {
    match scope {
        "system" => tr!("agentSkills.system"),
        "user" => tr!("agentSkills.user"),
        "plugin" => tr!("agentSkills.plugin"),
        value => value.to_string(),
    }
}

/// Builds subagent definition controls, honoring each DTO's canToggle/canDelete flags.
/// 构建子代理定义控件，遵守各 DTO 的 canToggle 与 canDelete 标记。
pub(super) fn agents(app: &App) -> Vec<Row> {
    // Local filters never mutate definitions; service-issued permissions still own each operation.
    // 本地筛选不修改定义；各操作仍受服务签发权限约束。
    let mut rows = vec![
        choice(
            tr!("tui.catalog.status"),
            status_options(false),
            app.agent_status.clone(),
            Edit::Local(LocalSetting::AgentStatus),
        ),
        button(
            tr!("tui.catalog.search"),
            PageId::AgentList
                .search(app)
                .expect("searchable agent catalog"),
            Tone::Normal,
        ),
        info(
            tr!("tui.catalog.query"),
            app.agent_query.clone(),
            Tone::Muted,
        ),
        button(
            tr!("tui.field.sync"),
            call(skills::refresh_subagent_catalog()),
            Tone::Normal,
        ),
        button(
            tr!("tui.field.openUserDirectory"),
            call(skills::open_subagent_user_directory()),
            Tone::Normal,
        ),
    ];
    // Definitions are authoritative and may have an optional categoryId.
    // 定义属于权威记录，并可包含可选 categoryId。
    let definitions = a(
        app.slots.value(Slot::Workbench),
        "/subagentCatalog/definitions",
    );
    // Count visible definitions to distinguish an empty filter from a pending catalog.
    // 统计可见定义，用于区分空筛选与目录未加载。
    let mut visible = 0;
    for agent in definitions {
        if (!app.agent_category.is_empty() && s(agent, "/categoryId") != app.agent_category)
            || (app.agent_status == "enabled" && !b(agent, "/enabled"))
            || (app.agent_status == "disabled" && b(agent, "/enabled"))
            || !matches_query(
                agent,
                &["/agentId", "/name", "/description"],
                &["name", "description"],
                &app.agent_query,
            )
        {
            continue;
        }
        visible += 1;
        // Exact operation identity and localized presentation.
        // 准确操作身份与本地化展示。
        let id = s(agent, "/agentId");
        // Upstream metadata declares localized name and description as optional.
        // 上游元数据将本地化名称及描述声明为可选。
        let name = navigation::localized(agent, "name").unwrap_or_else(|| s(agent, "/name"));
        // Optional localized description has its declared default.
        // 可选本地化描述使用其已声明默认值。
        let description =
            navigation::localized(agent, "description").unwrap_or_else(|| s(agent, "/description"));
        if b(agent, "/canToggle") {
            rows.push(toggle(
                name.clone(),
                b(agent, "/enabled"),
                dto(agent.clone(), "/enabled", |request| {
                    skills::set_subagent_definition_enabled(
                        &s(&request, "/agentId"),
                        b(&request, "/enabled"),
                    )
                }),
            ));
        }
        // Preserve distinct restore/delete permissions for system and user definitions.
        // 为系统及用户定义保留独立恢复、删除权限。
        let mut actions = Vec::new();
        if b(agent, "/canRestoreDefault") {
            actions.push(action(
                tr!("tui.field.restore"),
                call(skills::restore_subagent_definition_default(&id)),
                Tone::Normal,
            ));
        }
        if b(agent, "/canDelete") {
            actions.push(action(
                tr!("tui.field.remove"),
                confirm(
                    tr!("tui.field.remove"),
                    name.clone(),
                    tr!("tui.field.remove"),
                    call(skills::delete_subagent_definition(&id)),
                ),
                Tone::Bad,
            ));
        }
        // Prompt text is not part of the catalog summary; the row shows the stable identifier.
        // 提示词正文不属于目录摘要；该行展示稳定标识。
        let lines = vec![format!("{}: {}", tr!("tui.catalog.identifier"), id)];
        rows.push(
            item(&id, name, description)
                .badge(layer_label(&s(agent, "/scopeKey")), Tone::Muted)
                .lines(lines)
                .actions(actions),
        );
    }
    if visible == 0 {
        rows.push(note(
            if app.slots.loaded(Slot::Workbench) {
                tr!("tui.catalog.empty")
            } else {
                tr!("tui.page.loading")
            },
            Tone::Muted,
        ));
    }
    rows
}

/// Builds source-aware skill rows; canonical names, rather than invented ids, identify actions.
/// 构建包含来源的技能行；动作使用规范名称，不使用虚构标识。
pub(super) fn skills(app: &App) -> Vec<Row> {
    // The effective default and explicit source views match the upstream catalog semantics.
    // 生效默认视图及明确来源视图与上游目录语义一致。
    let mut rows = vec![
        choice(
            tr!("tui.catalog.source"),
            vec![
                ("all".into(), tr!("agentSkills.all")),
                ("system".into(), layer_label("system")),
                ("user".into(), layer_label("user")),
                ("plugin".into(), layer_label("plugin")),
                ("overrides".into(), tr!("tui.catalog.overrides")),
            ],
            app.skill_source.clone(),
            Edit::Local(LocalSetting::SkillSource),
        ),
        choice(
            tr!("tui.catalog.status"),
            status_options(true),
            app.skill_status.clone(),
            Edit::Local(LocalSetting::SkillStatus),
        ),
        button(
            tr!("tui.catalog.search"),
            PageId::SkillList
                .search(app)
                .expect("searchable skill catalog"),
            Tone::Normal,
        ),
        info(
            tr!("tui.catalog.query"),
            app.skill_query.clone(),
            Tone::Muted,
        ),
        button(
            tr!("tui.field.sync"),
            call(skills::refresh_agent_skills_catalog()),
            Tone::Normal,
        ),
        button(
            tr!("tui.field.openUserDirectory"),
            call(skills::open_agent_skills_user_directory()),
            Tone::Normal,
        ),
    ];
    // Each error owns a declared skill-directory path, as in the upstream issues filter.
    // 每条错误持有已声明技能目录路径，与上游问题筛选一致。
    let catalog = v(app.slots.value(Slot::Workbench), "/agentSkills");
    // Visible filtered record count.
    // 筛选后可见记录数。
    let mut visible = 0;
    for skill in a(catalog, "/records") {
        // Explicit source views include shadowed records; all shows only the effective version.
        // 明确来源视图包含被覆盖记录；全部视图只显示生效版本。
        let source_matches = match app.skill_source.as_str() {
            "all" => b(skill, "/effective"),
            "overrides" => s(skill, "/scopeKey") == "user" && !v(skill, "/overrideOf").is_null(),
            source => s(skill, "/scopeKey") == source,
        };
        // Upstream reports issues by a path rooted in the record baseDir.
        // 上游按位于记录 baseDir 下的路径报告问题。
        let has_issue = a(catalog, "/errors")
            .iter()
            .any(|issue| s(issue, "/path").starts_with(&s(skill, "/baseDir")));
        if !source_matches
            || (!app.skill_category.is_empty() && s(skill, "/categoryId") != app.skill_category)
            || (app.skill_status == "enabled" && !(b(skill, "/effective") && b(skill, "/enabled")))
            || (app.skill_status == "disabled"
                && !(b(skill, "/effective") && !b(skill, "/enabled")))
            || (app.skill_status == "issues" && !has_issue)
            || !matches_query(
                skill,
                &[
                    "/name",
                    "/displayName",
                    "/description",
                    "/standard/description",
                    "/origin/kind",
                    "/origin/label",
                    "/categoryId",
                    "/tags",
                    "/body",
                ],
                &["displayName", "shortDescription"],
                &app.skill_query,
            )
        {
            continue;
        }
        visible += 1;
        // Stable service name owns writes; skillKey distinguishes source versions in UI.
        // 稳定服务名称持有写操作；skillKey 在界面区分来源版本。
        let name = s(skill, "/name");
        // Optional localized display name falls back to the documented nonempty name.
        // 可选本地化显示名称回退到文档规定的非空名称。
        let label = navigation::localized(skill, "displayName").unwrap_or_else(|| {
            // Empty displayName is explicitly handled by the upstream name helper.
            // 上游名称辅助函数明确处理空 displayName。
            let display = s(skill, "/displayName");
            if display.is_empty() {
                name.clone()
            } else {
                display
            }
        });
        // The upstream description helper declares shortDescription, description and standard.description.
        // 上游描述辅助函数声明 shortDescription、description 及 standard.description。
        let description = navigation::localized(skill, "shortDescription").unwrap_or_else(|| {
            // Empty optional summary uses the standard frontmatter description.
            // 空的可选摘要使用标准 frontmatter 描述。
            let description = s(skill, "/description");
            if description.is_empty() {
                s(skill, "/standard/description")
            } else {
                description
            }
        });
        if b(skill, "/canToggle") {
            rows.push(toggle(
                label.clone(),
                b(skill, "/enabled"),
                dto(skill.clone(), "/enabled", |request| {
                    skills::set_agent_skill_enabled(&s(&request, "/name"), b(&request, "/enabled"))
                }),
            ));
        }
        // Permission-specific actions retain their service-owned identity.
        // 基于权限的操作保留服务持有身份。
        let mut actions = Vec::new();
        if b(skill, "/canRestoreDefault") {
            actions.push(action(
                tr!("tui.field.restore"),
                call(skills::restore_agent_skill_default(&name)),
                Tone::Normal,
            ));
        }
        if b(skill, "/canDelete") {
            actions.push(action(
                tr!("tui.field.remove"),
                confirm(
                    tr!("tui.field.remove"),
                    label.clone(),
                    tr!("tui.field.remove"),
                    call(skills::delete_agent_skill(&name)),
                ),
                Tone::Bad,
            ));
        }
        // Prompt body is expanded as readable lines rather than escaped JSON.
        // 提示词正文展开为可读行，而非转义 JSON。
        let mut lines = vec![format!("{}: {}", tr!("tui.catalog.identifier"), name)];
        lines.extend(s(skill, "/body").lines().map(str::to_string));
        rows.push(
            item(&s(skill, "/skillKey"), label, description)
                .badge(
                    format!(
                        "{} · {}",
                        layer_label(&s(skill, "/scopeKey")),
                        if b(skill, "/effective") {
                            tr!("tui.catalog.effective")
                        } else {
                            tr!("tui.catalog.shadowed")
                        }
                    ),
                    if has_issue { Tone::Warn } else { Tone::Muted },
                )
                .lines(lines)
                .actions(actions),
        );
    }
    if visible == 0 {
        rows.push(note(
            if app.slots.loaded(Slot::Workbench) {
                tr!("tui.catalog.empty")
            } else {
                tr!("tui.page.loading")
            },
            Tone::Muted,
        ));
    }
    rows
}
