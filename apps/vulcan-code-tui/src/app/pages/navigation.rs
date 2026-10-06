// Navigation projects server-owned categories and libraries without creating a second catalog.
// 导航投影服务端分类及知识库，不创建第二套目录。

use super::*;

/// One selectable navigation row, including its exact shared action and current selection.
/// 单个可选导航行，包含准确共享动作及当前选择状态。
#[derive(Debug, Clone)]
pub struct NavigationEntry {
    /// Localized catalog name or static page label.
    /// 本地化目录名称或静态页面标签。
    pub label: String,
    /// Authoritative catalog count or static page badge.
    /// 权威目录计数或静态页面徽标。
    pub badge: Option<(String, Tone)>,
    /// Shared action used by keyboard, wheel and mouse activation.
    /// 键盘、滚轮及鼠标激活使用的共享动作。
    pub action: UiAction,
    /// Whether this exact category, library or page is active.
    /// 此准确分类、知识库或页面是否处于活动状态。
    pub selected: bool,
}

/// Reads locale metadata using the WebUI's exact-locale then same-language rule.
/// 按 WebUI 的精确语言再同语言规则读取本地化元数据。
/// `record` owns optional locales; `field` is its declared localized metadata field.
/// `record` 持有可选 locales；`field` 为其已声明本地化元数据字段。
/// Returns nonempty localized text, or None when the protocol supplies no matching value.
/// 返回非空本地化文本；协议未提供匹配值时返回 None。
pub(super) fn localized(record: &Value, field: &str) -> Option<String> {
    // Optional locale maps are declared by both catalog summary schemas.
    // 两种目录摘要 Schema 均声明了可选语言映射。
    let locales = record.get("locales")?.as_object()?;
    // Current resolved TUI locale.
    // 当前已解析 TUI 语言。
    let locale = crate::i18n::active_locale();
    // WebUI ignores empty localized metadata.
    // WebUI 忽略空的本地化元数据。
    let read = |entry: &Value| {
        entry
            .get(field)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    if let Some(value) = locales.get(locale).and_then(read) {
        return Some(value);
    }
    // Same-language fallback is explicitly defined by the upstream localization helpers.
    // 同语言回退由上游本地化辅助函数明确规定。
    let language = locale.split('-').next()?;
    locales
        .iter()
        .find(|(key, _)| key.split('-').next() == Some(language))
        .and_then(|(_, value)| read(value))
}

/// Resolves the category display name from declared locale metadata and its default name.
/// 从已声明语言元数据及默认名称解析分类显示名称。
/// `category` is a server-owned category summary; returns its localized/default label.
/// `category` 为服务端分类摘要；返回其本地化或默认标签。
pub(super) fn category_name(category: &Value) -> String {
    localized(category, "name").unwrap_or_else(|| s(category, "/name"))
}

/// Builds the active area's navigation from authoritative categories or library identities.
/// 从权威分类或知识库身份构建当前区域导航。
/// `app` supplies the sanitized workbench/library slots and client-only selection state.
/// `app` 提供脱敏工作台、知识库槽位及纯客户端选择状态。
/// Returns grouped rows; all destinations remain UiAction values.
/// 返回分组行；全部目标保持为 UiAction 值。
pub fn navigation(app: &App) -> Vec<(Option<String>, Vec<NavigationEntry>)> {
    // Current workbench projection contains the exact catalog metadata.
    // 当前工作台投影包含准确目录元数据。
    let workbench = app.slots.value(Slot::Workbench);
    if matches!(app.area, Area::Agents | Area::AgentSkills) {
        // Agent definitions and prompt skills have distinct catalog paths and ownership.
        // 智能体定义与提示词技能拥有不同目录路径及归属。
        let (catalog, category, page, title, all) = if app.area == Area::Agents {
            (
                v(workbench, "/subagentCatalog"),
                app.agent_category.as_str(),
                PageId::AgentList,
                tr!("agents.categories"),
                tr!("agents.all"),
            )
        } else {
            (
                v(workbench, "/agentSkills"),
                app.skill_category.as_str(),
                PageId::SkillList,
                tr!("agentSkills.categories"),
                tr!("agentSkills.all"),
            )
        };
        // Skill counts match the WebUI effective-record category menu.
        // 技能计数与 WebUI 生效记录分类菜单一致。
        let records: Vec<&Value> = if page == PageId::AgentList {
            a(catalog, "/definitions").iter().collect()
        } else {
            a(catalog, "/records")
                .iter()
                .filter(|record| b(record, "/effective"))
                .collect()
        };
        // Agent categories sort by order and localized name; skills retain the server's stable order.
        // 智能体分类按排序值及本地化名称排列；技能保留服务端稳定顺序。
        let mut categories: Vec<&Value> = a(catalog, "/categories").iter().collect();
        if page == PageId::AgentList {
            categories.sort_by(|left, right| {
                v(left, "/order")
                    .as_i64()
                    .unwrap_or(i64::MAX)
                    .cmp(&v(right, "/order").as_i64().unwrap_or(i64::MAX))
                    .then_with(|| category_name(left).cmp(&category_name(right)))
                    .then_with(|| s(left, "/categoryId").cmp(&s(right, "/categoryId")))
            });
        }
        // Category actions address exact server identities, including the all-categories empty id.
        // 分类动作使用准确服务端身份，包含全部分类的空标识。
        let action_for = |id: String| {
            if page == PageId::AgentList {
                UiAction::AgentCategory(id)
            } else {
                UiAction::SkillCategory(id)
            }
        };
        // All-categories row remains available for empty or failed catalog loads.
        // 空目录或目录加载失败时，全部分类行仍可用。
        let mut entries = vec![NavigationEntry {
            label: all,
            badge: Some((records.len().to_string(), Tone::Muted)),
            action: action_for(String::new()),
            selected: category.is_empty(),
        }];
        for row in categories {
            // Exact category identity and its authoritative member count.
            // 准确分类身份及其权威成员计数。
            let id = s(row, "/categoryId");
            // Count logical membership independently from the right-side source/state filters.
            // 独立于右侧来源、状态筛选统计逻辑分类成员。
            let count = records
                .iter()
                .filter(|record| s(record, "/categoryId") == id)
                .count();
            entries.push(NavigationEntry {
                label: format!("{} {}", s(row, "/emoji"), category_name(row)),
                badge: Some((count.to_string(), Tone::Muted)),
                action: action_for(id.clone()),
                selected: category == id,
            });
        }
        return vec![(Some(title), entries)];
    }
    if app.area == Area::Knowledge {
        // Registry management remains reachable when no library exists.
        // 不存在知识库时仍可进入注册表管理。
        let mut entries = vec![NavigationEntry {
            label: tr!("tui.knowledge.catalog"),
            badge: None,
            action: UiAction::GoPage(PageId::KnowledgeBases),
            selected: app.knowledge_page == PageId::KnowledgeBases || app.library.is_none(),
        }];
        for library in a(app.slots.value(Slot::KnowledgeLibraries), "") {
            // Stable catalog identity, never a transient array index.
            // 稳定目录身份，不使用临时数组下标。
            let id = s(library, "/id");
            entries.push(NavigationEntry {
                label: s(library, "/name"),
                badge: Some((n(library, "/documentCount").to_string(), Tone::Muted)),
                action: UiAction::OpenLibrary(id.clone()),
                selected: app.knowledge_page != PageId::KnowledgeBases
                    && app.library.as_deref() == Some(id.as_str()),
            });
        }
        return vec![(Some(tr!("tui.knowledge.libraries")), entries)];
    }
    app.area
        .nav()
        .into_iter()
        .map(|(group, pages)| {
            (
                group,
                pages
                    .into_iter()
                    .flat_map(|page| {
                        // Plugin pages expand to one row per runtime-ready mount.
                        // 插件页面展开为每个运行就绪挂载一行。
                        if page == PageId::PluginMount {
                            return super::plugin_settings::navigation(app);
                        }
                        vec![NavigationEntry {
                            label: page.label(),
                            badge: page.badge(app),
                            action: UiAction::GoPage(page),
                            selected: app.current_page() == Some(page),
                        }]
                    })
                    .collect(),
            )
        })
        .collect()
}
