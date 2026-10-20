//! Injection snapshot: full-text parts, catalogs and their budget usage.
//! 注入快照：全文部分、目录及其预算占用。

use serde::Serialize;

use crate::scope::BASE_SECTION_KEY;
use crate::{
    BUDGET_WARNING_PERCENT, InjectionBudget, MemoryDocument, MemoryScope, NodeView, ScopeDocuments,
    estimate_tokens,
};

/// One budgeted part of the injection.
/// 注入中的一个预算分项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BudgetPart {
    /// User profile.
    /// 用户画像。
    User,
    /// Space base section.
    /// 空间基础分区。
    SpaceBase,
    /// Project base section.
    /// 项目基础分区。
    ProjectBase,
    /// Space catalog.
    /// 空间目录。
    SpaceCatalog,
    /// Project catalog.
    /// 项目目录。
    ProjectCatalog,
}

impl BudgetPart {
    /// Returns the stable label used in model-facing text.
    /// 返回面向模型文本中使用的稳定标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::User => "user profile",
            Self::SpaceBase => "space base",
            Self::ProjectBase => "project base",
            Self::SpaceCatalog => "space catalog",
            Self::ProjectCatalog => "project catalog",
        }
    }

    /// Returns this part's budget.
    /// 返回该分项的预算。
    pub const fn budget(self, budget: &InjectionBudget) -> usize {
        match self {
            Self::User => budget.user,
            Self::SpaceBase => budget.space_base,
            Self::ProjectBase => budget.project_base,
            Self::SpaceCatalog => budget.space_catalog,
            Self::ProjectCatalog => budget.project_catalog,
        }
    }
}

/// Usage of one budget part.
/// 单个预算分项的占用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetUsage {
    /// Budget part.
    /// 预算分项。
    pub part: BudgetPart,
    /// Tokens the full content needs.
    /// 完整内容所需的 Token。
    pub tokens: usize,
    /// Configured budget.
    /// 配置的预算。
    pub budget: usize,
    /// Titles left out of a full-text part, or the catalog depth that was dropped.
    /// 全文部分被省略的标题，或被降级舍去的目录层级。
    pub omitted: Vec<String>,
}

impl BudgetUsage {
    /// Returns whether usage reached the warning ratio without exceeding the budget.
    /// 返回占用是否达到提醒比例但未超出预算。
    pub fn near_budget(&self) -> bool {
        !self.over_budget()
            && self.budget > 0
            && self.tokens * 100 >= self.budget * BUDGET_WARNING_PERCENT
    }

    /// Returns whether the content needs more than the budget.
    /// 返回内容所需是否超过预算。
    pub fn over_budget(&self) -> bool {
        self.tokens > self.budget
    }

    /// Renders one usage line for a write receipt.
    /// 为写入回执渲染一行占用。
    pub fn render_line(&self) -> String {
        let percent = (self.tokens * 100).checked_div(self.budget).unwrap_or(0);
        format!(
            "Injected {}: {}/{} tokens ({percent}%).",
            self.part.label(),
            self.tokens,
            self.budget
        )
    }
}

/// The text injected at the start of a session and after compaction, with its usage.
/// 会话开始与压缩后注入的文本及其占用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionSnapshot {
    /// Model-facing snapshot text; empty when every scope is empty.
    /// 面向模型的快照文本；所有层为空时为空。
    pub content: String,
    /// Usage of every part that has content.
    /// 每个有内容分项的占用。
    pub usage: Vec<BudgetUsage>,
}

/// Returns usage of the full-text part of a scope, for write receipts.
/// 返回某层全文部分的占用，供写入回执使用。
pub(crate) fn base_usage(
    scope: MemoryScope,
    document: &MemoryDocument,
    budget: &InjectionBudget,
) -> BudgetUsage {
    let part = full_text_part(scope);
    let entries = full_text_entries(&document.views(scope), scope);
    let (_, usage) = fit_full_text(&entries, part, budget);
    usage
}

/// Builds the injection snapshot of every visible scope.
/// 构建全部可见层的注入快照。
pub fn build_injection_snapshot(
    documents: &ScopeDocuments,
    budget: &InjectionBudget,
) -> InjectionSnapshot {
    let mut sections = Vec::new();
    let mut usage = Vec::new();
    let mut scopes = vec![
        (MemoryScope::User, &documents.user, String::from("user")),
        (MemoryScope::Space, &documents.space, String::from("space")),
    ];
    if let Some((project, document)) = &documents.project {
        scopes.push((
            MemoryScope::Project,
            document,
            format!("project {}", project.name),
        ));
    }
    for (scope, document, label) in scopes {
        let views = document.views(scope);
        let entries = full_text_entries(&views, scope);
        if !entries.is_empty() {
            let part = full_text_part(scope);
            let (text, part_usage) = fit_full_text(&entries, part, budget);
            let heading = if scope.has_base_section() {
                format!("[{label} base]")
            } else {
                format!("[{label}]")
            };
            sections.push(format!("{heading}\n{text}"));
            usage.push(part_usage);
        }
        if scope.has_base_section() {
            let part = if scope == MemoryScope::Space {
                BudgetPart::SpaceCatalog
            } else {
                BudgetPart::ProjectCatalog
            };
            if let Some((text, part_usage)) = fit_catalog(&views, part, budget) {
                let updated = latest_update(&views)
                    .map(|updated| format!(" · updated {updated}"))
                    .unwrap_or_default();
                sections.push(format!("[{label} catalog{updated}]\n{text}"));
                usage.push(part_usage);
            }
        }
    }
    if sections.is_empty() {
        return InjectionSnapshot {
            content: String::new(),
            usage,
        };
    }
    let mut content = String::from(
        "On conflict, project memory overrides space memory, which overrides the user profile. Catalog lines list titles only; read a node with memory_search before relying on it.\n\n",
    );
    content.push_str(&sections.join("\n\n"));
    InjectionSnapshot { content, usage }
}

/// Returns the full-text part of a scope.
/// 返回某层的全文分项。
fn full_text_part(scope: MemoryScope) -> BudgetPart {
    match scope {
        MemoryScope::User => BudgetPart::User,
        MemoryScope::Space => BudgetPart::SpaceBase,
        MemoryScope::Project => BudgetPart::ProjectBase,
    }
}

/// Returns `(title path, rendered entry)` for every non-empty node injected in full.
/// 返回每个全文注入的非空节点的 `(标题路径, 渲染条目)`。
fn full_text_entries(views: &[NodeView], scope: MemoryScope) -> Vec<(String, String)> {
    views
        .iter()
        .filter(|view| !view.body.is_empty())
        .filter(|view| {
            !scope.has_base_section() || view.section.as_deref() == Some(BASE_SECTION_KEY)
        })
        .map(|view| {
            let path = if scope.has_base_section() {
                view.path[1..].join(" › ")
            } else {
                view.path.join(" › ")
            };
            let label = if path.is_empty() {
                view.path.join(" › ")
            } else {
                path
            };
            let entry = if view.body.contains('\n') {
                let indented = view
                    .body
                    .lines()
                    .map(|line| {
                        if line.is_empty() {
                            String::new()
                        } else {
                            format!("  {line}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("- {label}:\n{indented}")
            } else {
                format!("- {label}: {}", view.body)
            };
            (label, entry)
        })
        .collect()
}

/// Keeps whole entries until the budget is reached and lists the omitted titles.
/// 保留完整条目直到达到预算，并列出被省略的标题。
fn fit_full_text(
    entries: &[(String, String)],
    part: BudgetPart,
    budget: &InjectionBudget,
) -> (String, BudgetUsage) {
    let limit = part.budget(budget);
    let total = estimate_tokens(
        &entries
            .iter()
            .map(|(_, entry)| entry.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let mut kept = Vec::new();
    let mut omitted = Vec::new();
    let mut used = 0usize;
    for (label, entry) in entries {
        let cost = estimate_tokens(entry) + 1;
        if omitted.is_empty() && used + cost <= limit {
            used += cost;
            kept.push(entry.as_str());
        } else {
            omitted.push(label.clone());
        }
    }
    let mut text = kept.join("\n");
    if !omitted.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!(
            "(over budget; not shown, read with memory_search: {})",
            omitted.join(" · ")
        ));
    }
    (
        text,
        BudgetUsage {
            part,
            tokens: total,
            budget: limit,
            omitted,
        },
    )
}

/// Renders the catalog at the deepest level that fits, degrading H3 → H2 → section counts.
/// 以能放下的最深层级渲染目录，按 H3 → H2 → 分区计数逐级降级。
fn fit_catalog(
    views: &[NodeView],
    part: BudgetPart,
    budget: &InjectionBudget,
) -> Option<(String, BudgetUsage)> {
    let roots = views
        .iter()
        .filter(|view| view.locator.len() == 1)
        .filter(|view| view.section.as_deref() != Some(BASE_SECTION_KEY))
        .filter(|view| view.descendants > 0 || !view.body.is_empty())
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return None;
    }
    let limit = part.budget(budget);
    let full = render_catalog(views, &roots, 3);
    let tokens = estimate_tokens(&full);
    for (depth, dropped) in [(3, None), (2, Some("H3")), (1, Some("H2 and H3"))] {
        let text = if depth == 3 {
            full.clone()
        } else {
            render_catalog(views, &roots, depth)
        };
        if estimate_tokens(&text) <= limit {
            let mut text = text;
            if dropped.is_some() {
                text.push_str("\n(catalog shortened; list it with memory_outline)");
            }
            return Some((
                text,
                BudgetUsage {
                    part,
                    tokens,
                    budget: limit,
                    omitted: dropped.map(str::to_string).into_iter().collect(),
                },
            ));
        }
    }
    let mut lines = Vec::new();
    let mut used = 0usize;
    for root in &roots {
        let line = catalog_line(views, root, 1);
        let cost = estimate_tokens(&line) + 1;
        if used + cost > limit {
            break;
        }
        used += cost;
        lines.push(line);
    }
    let hidden = roots.len() - lines.len();
    lines.push(format!(
        "(catalog shortened, {hidden} more section(s); list them with memory_outline)"
    ));
    Some((
        lines.join("\n"),
        BudgetUsage {
            part,
            tokens,
            budget: limit,
            omitted: vec![String::from("sections")],
        },
    ))
}

/// Renders catalog lines for every root at a depth.
/// 以给定深度为每个根渲染目录行。
fn render_catalog(views: &[NodeView], roots: &[&NodeView], depth: u8) -> String {
    roots
        .iter()
        .map(|root| catalog_line(views, root, depth))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders one catalog line: `Section(count) › child [grandchildren] · child`.
/// 渲染一行目录：`分区(数量) › 子节点 [孙节点] · 子节点`。
fn catalog_line(views: &[NodeView], root: &NodeView, depth: u8) -> String {
    let title = root.path.last().cloned().unwrap_or_default();
    let mut line = format!("{title}({})", root.descendants);
    if depth < 2 {
        return line;
    }
    let children = direct_children(views, root);
    if children.is_empty() {
        return line;
    }
    let rendered = children
        .iter()
        .map(|child| {
            let child_title = child.path.last().cloned().unwrap_or_default();
            if depth < 3 {
                return child_title;
            }
            let grandchildren = direct_children(views, child)
                .iter()
                .map(|grandchild| grandchild.path.last().cloned().unwrap_or_default())
                .collect::<Vec<_>>();
            if grandchildren.is_empty() {
                child_title
            } else {
                format!("{child_title} [{}]", grandchildren.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");
    line.push_str(" › ");
    line.push_str(&rendered);
    line
}

/// Returns the direct children of a node in document order.
/// 按文档顺序返回节点的直接子节点。
fn direct_children<'views>(views: &'views [NodeView], parent: &NodeView) -> Vec<&'views NodeView> {
    views
        .iter()
        .filter(|view| {
            view.locator.len() == parent.locator.len() + 1
                && view.locator.starts_with(&parent.locator)
        })
        .collect()
}

/// Returns the date part of the newest update time in a scope.
/// 返回某层最新更新时间的日期部分。
fn latest_update(views: &[NodeView]) -> Option<String> {
    views
        .iter()
        .filter_map(|view| view.meta.as_ref().map(|meta| meta.updated.as_str()))
        .max()
        .map(|updated| updated.chars().take(10).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds scope documents from user, space and project texts.
    /// 由用户、空间与项目文本构建各层文档。
    fn documents(user: &str, space: &str, project: Option<&str>) -> ScopeDocuments {
        ScopeDocuments {
            user: MemoryDocument::parse(user),
            space: MemoryDocument::parse(space),
            project: project.map(|text| {
                (
                    crate::MemoryProject {
                        id: String::from("p-1"),
                        name: String::from("demo"),
                        host_project_ids: Vec::new(),
                        paths: Vec::new(),
                        first_commit: None,
                        memory_enabled: true,
                        created: String::new(),
                        updated: String::new(),
                    },
                    MemoryDocument::parse(text),
                )
            }),
        }
    }

    const BASE: &str = "<!-- mem id=m-b section=base created=2026-10-01T00:00Z updated=2026-10-02T00:00Z source=user -->";

    /// Base sections are injected in full and other sections only as catalogs.
    /// 基础分区全文注入，其他分区只以目录注入。
    #[test]
    fn injects_base_in_full_and_catalog_as_titles() {
        let space = format!(
            "# Base\n{BASE}\n## Verify narrowly\nRun targeted tests only.\n# Conventions\n## Commits\nSplit by feature.\n### Message style\nimperative\n"
        );
        let snapshot = build_injection_snapshot(
            &documents("# Identity\n## Role\nBackend engineer\n", &space, None),
            &InjectionBudget::default(),
        );
        assert!(
            snapshot
                .content
                .contains("[user]\n- Identity › Role: Backend engineer")
        );
        assert!(
            snapshot
                .content
                .contains("[space base]\n- Verify narrowly: Run targeted tests only.")
        );
        assert!(
            snapshot
                .content
                .contains("Conventions(2) › Commits [Message style]")
        );
        assert!(!snapshot.content.contains("Split by feature"));
        assert_eq!(snapshot.usage.len(), 3);
    }

    /// Over-budget full text keeps whole leading entries and names the rest.
    /// 超出预算的全文保留完整的前部条目并列出其余标题。
    #[test]
    fn truncates_full_text_at_node_boundaries() {
        let user = "# Identity\n## First\nshort\n## Second\nlonger text that will not fit in a tiny budget at all\n";
        let budget = InjectionBudget {
            user: estimate_tokens("- Identity › First: short") + 1,
            ..InjectionBudget::default()
        };
        let snapshot = build_injection_snapshot(&documents(user, "", None), &budget);
        assert!(snapshot.content.contains("- Identity › First: short"));
        assert!(
            snapshot
                .content
                .contains("read with memory_search: Identity › Second")
        );
        assert!(snapshot.usage[0].over_budget());
        assert_eq!(snapshot.usage[0].omitted, ["Identity › Second"]);
    }

    /// Catalogs degrade from H3 to H2 to section counts as the budget shrinks.
    /// 预算缩小时目录从 H3 降到 H2 再降到分区计数。
    #[test]
    fn degrades_catalog_depth() {
        let project =
            "# Architecture\n## Storage\n### Atomic writes\nx\n### Registry\ny\n## Providers\nz\n";
        let render = |tokens: usize| {
            let budget = InjectionBudget {
                project_catalog: tokens,
                ..InjectionBudget::default()
            };
            build_injection_snapshot(&documents("", "", Some(project)), &budget).content
        };
        assert!(render(1_000).contains("Storage [Atomic writes, Registry] · Providers"));
        let shallow = render(18);
        assert!(
            shallow.contains("Architecture(4) › Storage · Providers"),
            "{shallow}"
        );
        assert!(render(8).contains("Architecture(4)\n(catalog shortened"));
    }

    /// Empty memory produces no snapshot text.
    /// 空记忆不产生快照文本。
    #[test]
    fn empty_memory_injects_nothing() {
        let snapshot = build_injection_snapshot(
            &documents("", "# Base\n\n# Conventions\n", None),
            &InjectionBudget::default(),
        );
        assert!(snapshot.content.is_empty());
    }
}
