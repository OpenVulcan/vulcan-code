// Memory mode and Markdown memory pages, mirroring the WebUI memory mode page and the Markdown
// scope pages. The mode decides which memory pages the area offers; the Markdown pages list the
// notes of one scope and edit them as the user, one note at a time or as the whole file.
// 记忆模式与 Markdown 记忆页，镜像 WebUI 的记忆模式页与 Markdown 层页面。模式决定记忆区域提供
// 哪些页面；Markdown 页面列出单个层的笔记，并以用户身份逐条或整文件编辑。

use super::*;
use crate::service_api::markdown_memory as markdown;

/// Budget parts in the WebUI order: full-text parts first, then catalogs.
/// 与 WebUI 相同顺序的预算分项：先全文部分，再目录。
const BUDGET_PARTS: [&str; 5] = [
    "user",
    "spaceBase",
    "projectBase",
    "spaceCatalog",
    "projectCatalog",
];

/// The section key whose notes are injected in full; its heading cannot be moved or deleted.
/// 其笔记全文注入的分区键；该标题不能移动或删除。
const BASE_SECTION: &str = "base";

/// Localized name of one scope.
/// 单个层的本地化名称。
fn scope_label(scope: &str) -> String {
    match scope {
        "user" => tr!("memory.markdown.nav.user"),
        "space" => tr!("memory.markdown.nav.space"),
        _ => tr!("memory.markdown.nav.project"),
    }
}

/// Localized label of one budget part.
/// 单个预算分项的本地化标签。
fn part_label(part: &str) -> String {
    match part {
        "user" => tr!("memory.markdown.part.user"),
        "spaceBase" => tr!("memory.markdown.part.spaceBase"),
        "projectBase" => tr!("memory.markdown.part.projectBase"),
        "spaceCatalog" => tr!("memory.markdown.part.spaceCatalog"),
        "projectCatalog" => tr!("memory.markdown.part.projectCatalog"),
        other => other.to_string(),
    }
}

/// Budget parts each scope page reports.
/// 每层页面报告的预算分项。
fn scope_parts(scope: &str) -> &'static [&'static str] {
    match scope {
        "user" => &["user"],
        "space" => &["spaceBase", "spaceCatalog"],
        _ => &["projectBase", "projectCatalog"],
    }
}

/// Maps a memory page onto the pages the mode offers, as the WebUI `memorySectionForMode` does:
/// everything leads to the mode page while memory is off, VMM pages lead to the user scope in
/// Markdown mode and Markdown pages lead to the VMM overview in VMM mode. An unknown mode (still
/// loading) keeps the page.
/// 将记忆页面映射到当前模式提供的页面，与 WebUI `memorySectionForMode` 一致：记忆关闭时一律转到
/// 模式页，Markdown 模式下 VMM 页面转到用户层，VMM 模式下 Markdown 页面转到 VMM 总览。模式未知
/// （仍在加载）时保留原页面。
pub fn page_for_mode(page: PageId, mode: Option<&str>) -> PageId {
    let markdown = matches!(
        page,
        PageId::MarkdownUser | PageId::MarkdownSpace | PageId::MarkdownProject
    );
    match mode {
        _ if page == PageId::MemoryMode => page,
        Some("off") => PageId::MemoryMode,
        Some("markdown") if !markdown => PageId::MarkdownUser,
        Some("vmm") if markdown => PageId::MemoryOverview,
        _ => page,
    }
}

/// The scope key of a Markdown page.
/// Markdown 页面的层键。
pub(super) fn page_scope(page: PageId) -> Option<&'static str> {
    match page {
        PageId::MarkdownUser => Some("user"),
        PageId::MarkdownSpace => Some("space"),
        PageId::MarkdownProject => Some("project"),
        _ => None,
    }
}

/// Returns the loaders of a memory page: every memory page reads the mode, which decides the
/// pages; the mode page and the Markdown pages also read the overview.
/// 返回记忆页面的加载器：每个记忆页面都读取模式（模式决定页面）；模式页与 Markdown 页还读取总览。
pub(super) fn loaders(app: &App, page: PageId) -> Vec<ServiceCall> {
    let mut calls = vec![markdown::load_memory_settings()];
    match page {
        PageId::MemoryMode | PageId::MarkdownUser | PageId::MarkdownSpace => {
            calls.push(markdown::load_markdown_memory(None))
        }
        PageId::MarkdownProject => calls.push(markdown::load_markdown_memory(
            app.markdown_memory_project.as_deref(),
        )),
        _ => {}
    }
    calls
}

/// Saves the chosen mode with the saved budget.
/// 以已保存的预算保存所选模式。
fn save_mode(settings: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(markdown::save_memory_settings(
        value.as_str(),
        v(settings, "/budget").clone(),
    ))
}

/// Saves the budget with one part changed; `ctx` is `{settings, part}`.
/// 保存修改了一个分项的预算；`ctx` 为 `{settings, part}`。
fn save_part(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let settings = v(ctx, "/settings");
    let part = v(ctx, "/part").as_str()?;
    let tokens = value.as_number();
    if tokens < 0 || tokens > n(settings, "/maxPartTokens") {
        return None;
    }
    let mut budget = v(settings, "/budget").clone();
    *budget.get_mut(part)? = json!(tokens);
    Some(markdown::save_memory_settings(
        v(settings, "/mode").as_str()?,
        budget,
    ))
}

/// Builds the memory mode page: the mode, the Markdown injection budget and the memory projects.
/// 构建记忆模式页：模式、Markdown 注入预算与记忆项目。
pub(super) fn mode(app: &App) -> Vec<Row> {
    let mut rows = vec![
        section(tr!("memory.markdown.mode.title")),
        note(tr!("memory.markdown.mode.hint"), Tone::Muted),
    ];
    let settings = app.slots.value(Slot::MemorySettings);
    if settings.is_null() {
        return rows;
    }
    let current = s(settings, "/mode");
    rows.push(choice(
        tr!("memory.markdown.mode.title"),
        vec![
            ("off".into(), tr!("memory.markdown.mode.off")),
            ("vmm".into(), tr!("memory.markdown.mode.vmm")),
            ("markdown".into(), tr!("memory.markdown.mode.markdown")),
        ],
        current.clone(),
        Edit::Call(Setter::Build {
            ctx: settings.clone(),
            build: save_mode,
        }),
    ));
    rows.push(note(
        match current.as_str() {
            "off" => tr!("memory.markdown.mode.offHint"),
            "vmm" => tr!("memory.markdown.mode.vmmHint"),
            _ => tr!("memory.markdown.mode.markdownHint"),
        },
        Tone::Muted,
    ));
    if current != "markdown" {
        return rows;
    }
    rows.push(section(tr!("memory.markdown.budget.title")));
    rows.push(note(tr!("memory.markdown.budget.hint"), Tone::Muted));
    let max = n(settings, "/maxPartTokens");
    for part in BUDGET_PARTS {
        rows.push(number(
            part_label(part),
            n(settings, &format!("/budget/{part}")),
            (0, max, 50),
            Edit::Call(Setter::Build {
                ctx: json!({ "settings": settings, "part": part }),
                build: save_part,
            }),
        ));
    }
    let total: i64 = BUDGET_PARTS
        .iter()
        .map(|part| n(settings, &format!("/budget/{part}")))
        .sum();
    rows.push(info(
        tr!("memory.markdown.budget.title"),
        tr!("memory.markdown.budget.total", tokens = total),
        Tone::Muted,
    ));
    rows.push(button(
        tr!("memory.markdown.budget.reset"),
        call(markdown::save_memory_settings(
            "markdown",
            v(settings, "/defaultBudget").clone(),
        )),
        Tone::Normal,
    ));
    rows.push(section(tr!("memory.markdown.projects.title")));
    rows.push(note(tr!("memory.markdown.projects.hint"), Tone::Muted));
    let overview = app.slots.value(Slot::MarkdownMemory);
    if overview.is_null() {
        return rows;
    }
    let projects = a(overview, "/projects");
    if projects.is_empty() {
        rows.push(note(tr!("memory.markdown.projects.empty"), Tone::Muted));
    }
    for project in projects {
        let id = s(project, "/id");
        let enabled = b(project, "/memoryEnabled");
        let mut row = item(
            &id,
            s(project, "/name"),
            a(project, "/paths")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" · "),
        )
        .actions(vec![switch(
            tr!("memory.markdown.projects.enabled"),
            enabled,
            Some(call(markdown::apply_markdown_memory_project_action(
                json!({ "action": "set_enabled", "memoryProjectId": id, "enabled": !enabled }),
            ))),
        )]);
        if b(project, "/orphaned") {
            row = row.badge(tr!("memory.markdown.projects.orphaned"), Tone::Warn);
        }
        rows.push(row);
    }
    transfer_rows(&mut rows, app, projects);
    rows
}

/// Conflict resolutions in the WebUI order.
/// 与 WebUI 相同顺序的冲突解决方式。
const CONFLICT_CHOICES: [&str; 3] = ["keep_local", "take_imported", "keep_both"];

/// Export and import of memory packages. Export writes every scope; import is previewed against
/// the chosen project targets and applied with a resolution for every conflict.
/// 记忆导出包的导出与导入。导出写入全部层；导入按所选项目去向预览，并在每个冲突都有解决方式后应用。
fn transfer_rows(rows: &mut Vec<Row>, app: &App, projects: &[Value]) {
    rows.push(section(tr!("memory.markdown.transfer.title")));
    rows.push(note(tr!("memory.markdown.transfer.hint"), Tone::Muted));
    rows.push(note(tr!("tui.memory.transfer.exportAll"), Tone::Muted));
    let ids = projects
        .iter()
        .map(|project| s(project, "/id"))
        .collect::<Vec<_>>();
    rows.push(button(
        tr!("memory.markdown.transfer.export"),
        form(
            json!({ "destinationDirectory": "", "user": true, "space": true, "memoryProjectIds": ids }),
            vec![field(
                tr!("memory.markdown.transfer.folderTitle"),
                "/destinationDirectory",
                false,
                false,
            )],
            markdown::export_markdown_memory,
        ),
        Tone::Normal,
    ));
    rows.push(button(
        tr!("memory.markdown.transfer.import"),
        form(
            json!({ "archivePath": "" }),
            vec![field(
                tr!("memory.markdown.transfer.pickTitle"),
                "/archivePath",
                false,
                false,
            )],
            |request| {
                markdown::preview_markdown_memory_import(&s(&request, "/archivePath"), json!([]))
            },
        ),
        Tone::Normal,
    ));
    import_rows(rows, app, projects);
}

/// The open import: project targets, the merge of every mapped scope and the apply button.
/// 已打开的导入：项目去向、每个已指定层的合并以及应用按钮。
fn import_rows(rows: &mut Vec<Row>, app: &App, local: &[Value]) {
    let preview = app.slots.value(Slot::MarkdownMemoryImport);
    let Some(loader) = app.slots.loader(Slot::MarkdownMemoryImport) else {
        return;
    };
    if preview.is_null() {
        return;
    }
    let archive = s(&loader.params, "/request/archivePath");
    let mappings = v(&loader.params, "/request/projects").clone();
    rows.push(section(tr!("memory.markdown.transfer.importTitle")));
    rows.push(info(
        tr!("memory.markdown.transfer.pickTitle"),
        archive.clone(),
        Tone::Normal,
    ));
    rows.push(note(
        tr!(
            "memory.markdown.transfer.importHint",
            time = s(preview, "/exportedAt")
        ),
        Tone::Muted,
    ));
    let packaged = a(preview, "/projects");
    let mut unset = false;
    if !packaged.is_empty() {
        rows.push(note(
            tr!("memory.markdown.transfer.projectsHint"),
            Tone::Muted,
        ));
    }
    for entry in packaged {
        let id = s(entry, "/project/id");
        let current = target_value(&mappings, &id);
        unset |= current.is_empty();
        let candidates = a(entry, "/candidates");
        let mut options = Vec::new();
        if current.is_empty() {
            options.push((String::new(), tr!("memory.markdown.transfer.targetUnset")));
        }
        for candidate in candidates {
            let reasons = a(candidate, "/reasons")
                .iter()
                .filter_map(Value::as_str)
                .map(reason_label)
                .collect::<Vec<_>>()
                .join(", ");
            options.push((
                format!("link:{}", s(candidate, "/memoryProjectId")),
                tr!(
                    "memory.markdown.transfer.linkCandidate",
                    name = s(candidate, "/name"),
                    reasons = reasons
                ),
            ));
        }
        for project in local {
            let local_id = s(project, "/id");
            if candidates
                .iter()
                .any(|candidate| s(candidate, "/memoryProjectId") == local_id)
            {
                continue;
            }
            options.push((
                format!("link:{local_id}"),
                tr!("memory.markdown.transfer.link", name = s(project, "/name")),
            ));
        }
        options.push(("create".into(), tr!("memory.markdown.transfer.create")));
        options.push(("ignore".into(), tr!("memory.markdown.transfer.ignore")));
        rows.push(choice(
            s(entry, "/project/name"),
            options,
            current,
            Edit::Call(Setter::Build {
                ctx: json!({ "archivePath": archive, "projects": mappings, "id": id }),
                build: retarget,
            }),
        ));
    }
    if unset {
        for (label, target) in [
            (tr!("memory.markdown.transfer.createRest"), "create"),
            (tr!("memory.markdown.transfer.ignoreRest"), "ignore"),
        ] {
            rows.push(button(
                label,
                call(markdown::preview_markdown_memory_import(
                    &archive,
                    filled(packaged, &mappings, target),
                )),
                Tone::Normal,
            ));
        }
        return;
    }
    let scopes = a(preview, "/scopes");
    if scopes.is_empty() {
        rows.push(note(tr!("memory.markdown.transfer.nothing"), Tone::Muted));
        return;
    }
    let mut resolutions = Vec::new();
    for scope in scopes {
        let key = s(scope, "/scope");
        let package_project = v(scope, "/packageProjectId").as_str();
        let label = match package_project {
            Some(package_project) => tr!(
                "memory.markdown.transfer.projectScope",
                name = packaged
                    .iter()
                    .find(|entry| s(entry, "/project/id") == package_project)
                    .map(|entry| s(entry, "/project/name"))
                    .unwrap_or_else(|| package_project.to_string())
            ),
            None => scope_label(&key),
        };
        let conflicts = a(scope, "/conflicts");
        rows.push(info(
            label,
            tr!(
                "memory.markdown.transfer.counts",
                same = n(scope, "/same"),
                added = a(scope, "/added").len(),
                conflicts = conflicts.len()
            ),
            Tone::Normal,
        ));
        let titles = |pointer: &str| {
            a(scope, pointer)
                .iter()
                .map(|node| title_path(v(node, "/path")))
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !a(scope, "/newSections").is_empty() {
            let sections = a(scope, "/newSections")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            rows.push(note(
                tr!("memory.markdown.transfer.newSections", titles = sections),
                Tone::Muted,
            ));
        }
        if !a(scope, "/duplicates").is_empty() {
            rows.push(note(
                tr!(
                    "memory.markdown.transfer.duplicates",
                    titles = titles("/duplicates")
                ),
                Tone::Muted,
            ));
        }
        for conflict in conflicts {
            let node = s(conflict, "/id");
            let conflict_key = format!("{key}:{}:{node}", package_project.unwrap_or_default());
            let chosen = app
                .markdown_memory_conflicts
                .get(&conflict_key)
                .cloned()
                .unwrap_or_else(|| newer_choice(conflict).to_string());
            let mut resolution = json!({ "scope": key, "nodeId": node, "choice": chosen });
            if let Some(package_project) = package_project {
                resolution["packageProjectId"] = json!(package_project);
            }
            resolutions.push(resolution);
            rows.push(
                choice(
                    title_path(v(conflict, "/path")),
                    CONFLICT_CHOICES
                        .iter()
                        .map(|choice| {
                            (
                                format!("{conflict_key}{PERIOD_SEPARATOR}{choice}"),
                                choice_label(choice),
                            )
                        })
                        .collect(),
                    format!("{conflict_key}{PERIOD_SEPARATOR}{chosen}"),
                    Edit::Local(LocalSetting::MarkdownMemoryConflict),
                )
                .detail(format!(
                    "{}: {}\n{}: {}",
                    tr!("memory.markdown.transfer.local"),
                    s(conflict, "/local/body"),
                    tr!("memory.markdown.transfer.imported"),
                    s(conflict, "/imported/body")
                )),
            );
        }
    }
    rows.push(button(
        tr!("memory.markdown.transfer.apply"),
        call(markdown::import_markdown_memory(json!({
            "archivePath": archive,
            "expectedSha256": s(preview, "/sha256"),
            "projects": mappings,
            "resolutions": resolutions,
        }))),
        Tone::Accent,
    ));
}

/// The picker value of one packaged project's chosen target, or "" when none is chosen.
/// 单个包内项目所选去向的选择值；未选择时为 ""。
fn target_value(mappings: &Value, package_project: &str) -> String {
    let Some(target) = mappings.as_array().and_then(|mappings| {
        mappings
            .iter()
            .find(|mapping| s(mapping, "/packageProjectId") == package_project)
    }) else {
        return String::new();
    };
    match s(target, "/target/action").as_str() {
        "link" => format!("link:{}", s(target, "/target/memoryProjectId")),
        other => other.to_string(),
    }
}

/// The `ProjectTarget` of a picker value.
/// 选择值对应的 `ProjectTarget`。
fn target_of(value: &str) -> Option<Value> {
    match value {
        "create" | "ignore" => Some(json!({ "action": value })),
        _ => value
            .strip_prefix("link:")
            .map(|id| json!({ "action": "link", "memoryProjectId": id })),
    }
}

/// Re-previews with one packaged project's target changed; `ctx` is `{archivePath, projects, id}`.
/// 修改单个包内项目的去向后重新预览；`ctx` 为 `{archivePath, projects, id}`。
fn retarget(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let id = s(ctx, "/id");
    let target = target_of(value.as_str())?;
    let mut projects = a(ctx, "/projects")
        .iter()
        .filter(|mapping| s(mapping, "/packageProjectId") != id)
        .cloned()
        .collect::<Vec<_>>();
    projects.push(json!({ "packageProjectId": id, "target": target }));
    Some(markdown::preview_markdown_memory_import(
        &s(ctx, "/archivePath"),
        Value::Array(projects),
    ))
}

/// The mappings with every packaged project still without a target given `action`.
/// 为所有尚无去向的包内项目指定 `action` 后的去向列表。
fn filled(packaged: &[Value], mappings: &Value, action: &str) -> Value {
    let mut projects = mappings.as_array().cloned().unwrap_or_default();
    for entry in packaged {
        let id = s(entry, "/project/id");
        if target_value(mappings, &id).is_empty() {
            projects.push(json!({ "packageProjectId": id, "target": { "action": action } }));
        }
    }
    Value::Array(projects)
}

/// The default resolution: the newer side, keeping the local note when unknown.
/// 默认解决方式：较新一侧；无法判断时保留本地笔记。
fn newer_choice(conflict: &Value) -> &'static str {
    if s(conflict, "/newer") == "imported" {
        "take_imported"
    } else {
        "keep_local"
    }
}

/// Localized label of one conflict resolution.
/// 单个冲突解决方式的本地化标签。
fn choice_label(choice: &str) -> String {
    match choice {
        "keep_local" => tr!("memory.markdown.transfer.choice.keepLocal"),
        "take_imported" => tr!("memory.markdown.transfer.choice.takeImported"),
        _ => tr!("memory.markdown.transfer.choice.keepBoth"),
    }
}

/// Localized label of one candidate match reason.
/// 单个候选匹配原因的本地化标签。
fn reason_label(reason: &str) -> String {
    match reason {
        "same_path" => tr!("memory.markdown.transfer.reason.samePath"),
        "same_first_commit" => tr!("memory.markdown.orphan.reason.sameFirstCommit"),
        "same_directory_name" => tr!("memory.markdown.orphan.reason.sameDirectoryName"),
        other => other.to_string(),
    }
}

/// A note's titles from its section down, joined for display.
/// 笔记从分区向下的标题，拼接用于显示。
fn title_path(path: &Value) -> String {
    path.as_array()
        .map(|titles| {
            titles
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" › ")
        })
        .unwrap_or_default()
}

/// Replaces one note's body; `ctx` is `{projectId, id, expectedRevision}`.
/// 替换单条笔记的正文；`ctx` 为 `{projectId, id, expectedRevision}`。
fn replace_body(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    node_write(ctx, json!({ "action": "replace", "body": value.as_str() }))
}

/// Renames one note; a blank title is refused before any request.
/// 重命名单条笔记；空白标题在发出请求前即被拒绝。
fn rename_node(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let title = value.as_str().trim();
    if title.is_empty() {
        return None;
    }
    node_write(ctx, json!({ "action": "rename", "title": title }))
}

/// Builds one write of an existing note from its context and the operation's own fields.
/// 根据已有笔记的上下文与操作自身字段构建一次写入。
fn node_write(ctx: &Value, mut operation: Value) -> Option<ServiceCall> {
    operation["id"] = v(ctx, "/id").clone();
    operation["expectedRevision"] = v(ctx, "/expectedRevision").clone();
    Some(markdown::write_markdown_memory(
        v(ctx, "/projectId").as_str(),
        json!([operation]),
    ))
}

/// Sends the note collected by the add form; `request` is `{projectId, operation}`.
/// 发送添加表单收集的笔记；`request` 为 `{projectId, operation}`。
fn add_note(request: Value) -> ServiceCall {
    markdown::write_markdown_memory(
        v(&request, "/projectId").as_str(),
        json!([v(&request, "/operation")]),
    )
}

/// Replaces the whole file; `ctx` is `{scope, memoryProjectId, expected}`.
/// 替换整个文件；`ctx` 为 `{scope, memoryProjectId, expected}`。
fn replace_file(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    Some(markdown::replace_markdown_memory_file(
        v(ctx, "/scope").as_str()?,
        v(ctx, "/memoryProjectId").as_str(),
        v(ctx, "/expected").as_str()?,
        value.as_str(),
    ))
}

/// Builds one Markdown scope page: the project choice, usage, notes and whole-file editing.
/// 构建单个 Markdown 层页面：项目选择、用量、笔记与整文件编辑。
pub(super) fn scope(app: &App, scope: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let project_id = (scope == "project")
        .then(|| app.markdown_memory_project.clone())
        .flatten();
    if scope == "project" {
        if app.projects.is_empty() {
            rows.push(note(tr!("memory.markdown.project.none"), Tone::Muted));
            return rows;
        }
        rows.push(choice(
            tr!("tui.field.project"),
            app.projects
                .iter()
                .map(|project| (project.id.clone(), project.name.clone()))
                .collect(),
            project_id.clone().unwrap_or_default(),
            Edit::Local(LocalSetting::MarkdownMemoryProject),
        ));
    }
    let overview = app.slots.value(Slot::MarkdownMemory);
    // The overview of another project may still be shown while the chosen one loads.
    // 所选项目加载期间可能仍显示其他项目的总览。
    if overview.is_null()
        || (scope == "project" && v(overview, "/hostProjectId").as_str() != project_id.as_deref())
    {
        return rows;
    }
    let memory_project = v(overview, "/memoryProjectId").as_str();
    if scope == "project" {
        project_notices(
            &mut rows,
            overview,
            project_id.as_deref().unwrap_or_default(),
        );
        if memory_project.is_none() {
            rows.push(note(tr!("memory.markdown.project.unbound"), Tone::Muted));
        }
    }
    usage(&mut rows, overview, scope);
    rows.push(section(scope_label(scope)));
    for preset in a(overview, "/presetSections")
        .iter()
        .filter(|preset| s(preset, "/scope") == scope)
    {
        rows.push(button(
            format!(
                "{} · {}",
                tr!("memory.markdown.node.add"),
                s(preset, "/title")
            ),
            form(
                json!({
                    "projectId": project_id,
                    "operation": {
                        "action": "create",
                        "scope": scope,
                        "section": s(preset, "/key"),
                        "title": "",
                        "body": "",
                    },
                }),
                vec![
                    field(
                        tr!("memory.markdown.editor.title"),
                        "/operation/title",
                        false,
                        false,
                    ),
                    field(
                        tr!("memory.markdown.editor.body"),
                        "/operation/body",
                        false,
                        true,
                    ),
                ],
                add_note,
            ),
            Tone::Accent,
        ));
    }
    let nodes = a(overview, &format!("/{scope}"));
    if nodes.is_empty() {
        rows.push(note(tr!("memory.markdown.nodes.empty"), Tone::Muted));
    }
    for node in nodes {
        rows.push(node_row(node, project_id.as_deref()));
    }
    file_rows(&mut rows, app, scope, memory_project);
    rows
}

/// Adds the orphan suggestion and the disabled warning of the project page.
/// 添加项目页的孤立记忆建议与停用提醒。
fn project_notices(rows: &mut Vec<Row>, overview: &Value, host: &str) {
    let projects = a(overview, "/projects");
    let named = |id: &str| {
        projects
            .iter()
            .find(|project| s(project, "/id") == id)
            .map(|project| s(project, "/name"))
    };
    if let Some(orphan) = a(overview, "/orphanCandidates").first() {
        let memory_project = s(orphan, "/memoryProjectId");
        let reasons = a(orphan, "/reasons")
            .iter()
            .filter_map(Value::as_str)
            .map(|reason| match reason {
                "same_directory_name" => tr!("memory.markdown.orphan.reason.sameDirectoryName"),
                "same_first_commit" => tr!("memory.markdown.orphan.reason.sameFirstCommit"),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        rows.push(section(tr!("memory.markdown.orphan.title")));
        rows.push(note(
            tr!(
                "memory.markdown.orphan.body",
                name = named(&memory_project).unwrap_or_else(|| memory_project.clone()),
                reasons = reasons,
            ),
            Tone::Warn,
        ));
        for (label, action, tone) in [
            (
                tr!("memory.markdown.orphan.link"),
                json!({ "action": "link", "projectId": host, "memoryProjectId": memory_project }),
                Tone::Accent,
            ),
            (
                tr!("memory.markdown.orphan.create"),
                json!({ "action": "create", "projectId": host }),
                Tone::Normal,
            ),
            (
                tr!("memory.markdown.orphan.ignore"),
                json!({ "action": "ignore", "projectId": host }),
                Tone::Muted,
            ),
        ] {
            rows.push(button(
                label,
                call(markdown::apply_markdown_memory_project_action(action)),
                tone,
            ));
        }
    }
    let shown = v(overview, "/memoryProjectId").as_str();
    if projects
        .iter()
        .any(|project| v(project, "/id").as_str() == shown && !b(project, "/memoryEnabled"))
    {
        rows.push(note(tr!("memory.markdown.project.disabled"), Tone::Warn));
    }
}

/// Adds the injection usage of the scope's budget parts, warning from the service's percentage.
/// 添加本层预算分项的注入用量，从服务给出的百分比起提醒。
fn usage(rows: &mut Vec<Row>, overview: &Value, scope: &str) {
    let parts = scope_parts(scope);
    let usage: Vec<&Value> = a(overview, "/usage")
        .iter()
        .filter(|row| parts.contains(&s(row, "/part").as_str()))
        .collect();
    if usage.is_empty() {
        return;
    }
    let warning = n(overview, "/settings/warningPercent");
    rows.push(section(tr!("memory.markdown.usage.title")));
    let mut crowded = false;
    for row in usage {
        let (tokens, budget) = (n(row, "/tokens"), n(row, "/budget"));
        let over = tokens > budget;
        let near = !over && budget > 0 && tokens * 100 >= budget * warning;
        crowded |= over || near;
        let value = tr!(
            "memory.markdown.usage.value",
            tokens = tokens,
            budget = budget
        );
        rows.push(meter(
            part_label(&s(row, "/part")),
            tokens as f64 / budget.max(1) as f64,
            if over {
                format!("{value} · {}", tr!("memory.markdown.usage.over"))
            } else if near {
                format!("{value} · {}", tr!("memory.markdown.usage.near"))
            } else {
                value
            },
            if over {
                Tone::Bad
            } else if near {
                Tone::Warn
            } else {
                Tone::Good
            },
        ));
        let omitted = a(row, "/omitted")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        if !omitted.is_empty() {
            rows.push(note(
                tr!("memory.markdown.usage.omitted", titles = omitted.join(", ")),
                Tone::Muted,
            ));
        }
    }
    if crowded {
        rows.push(note(tr!("memory.markdown.usage.curateHint"), Tone::Muted));
    }
}

/// Builds one note row with its edit, rename and delete actions; the base heading stays fixed.
/// 构建单条笔记行及其编辑、重命名与删除动作；基础分区标题保持固定。
fn node_row(node: &Value, project_id: Option<&str>) -> Row {
    let id = s(node, "/id");
    let level = n(node, "/level").max(1) as usize;
    let title = a(node, "/path")
        .last()
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let ctx = json!({
        "projectId": project_id,
        "id": id,
        "expectedRevision": s(node, "/revision"),
    });
    let fixed = level == 1 && v(node, "/section").as_str() == Some(BASE_SECTION);
    let mut detail = Vec::new();
    match v(node, "/source").as_str() {
        Some("user") => detail.push(tr!("memory.markdown.node.sourceUser")),
        Some("ai") => detail.push(tr!("memory.markdown.node.sourceAi")),
        _ => {}
    }
    if let Some(updated) = v(node, "/updated").as_str() {
        detail.push(tr!("memory.markdown.node.updated", time = updated));
    }
    let body = s(node, "/body");
    let mut actions = vec![
        action(
            tr!("memory.markdown.node.edit"),
            super::super::action::prompt(
                title.clone(),
                tr!("memory.markdown.editor.body"),
                body.clone(),
                PromptThen::Edit(Edit::Call(Setter::Build {
                    ctx: ctx.clone(),
                    build: replace_body,
                })),
            ),
            Tone::Accent,
        ),
        action(
            tr!("memory.markdown.editor.title"),
            super::super::action::prompt(
                tr!("memory.markdown.editor.editTitle"),
                tr!("memory.markdown.editor.title"),
                title.clone(),
                PromptThen::Edit(Edit::Call(Setter::Build {
                    ctx: ctx.clone(),
                    build: rename_node,
                })),
            ),
            Tone::Normal,
        ),
    ];
    if !fixed {
        let descendants = n(node, "/descendants");
        actions.push(action(
            tr!("memory.markdown.node.delete"),
            confirm(
                tr!("memory.markdown.node.deleteConfirm", title = title),
                if descendants > 0 {
                    tr!("memory.markdown.node.deleteChildren", count = descendants)
                } else {
                    String::new()
                },
                tr!("memory.markdown.node.delete"),
                call(markdown::write_markdown_memory(
                    project_id,
                    json!([{
                        "action": "delete",
                        "id": id,
                        "expectedRevision": s(node, "/revision"),
                        "withChildren": descendants > 0,
                    }]),
                )),
            ),
            Tone::Bad,
        ));
    }
    let mut row = item(
        &id,
        format!("{}{title}", "  ".repeat(level - 1)),
        detail.join(" · "),
    )
    .lines(body.lines().map(str::to_string).collect())
    .actions(actions);
    if fixed {
        row = row.badge(tr!("memory.markdown.node.base"), Tone::Info);
    }
    row
}

/// Adds whole-file editing: a button reads the file, and once the text of this exact scope and
/// project has arrived it is shown as an editable text row that saves against the read text.
/// 添加整文件编辑：按钮读取文件；当本层与本项目的文本到达后，显示为可编辑文本行，并以读取的
/// 文本为基准保存。
fn file_rows(rows: &mut Vec<Row>, app: &App, scope: &str, memory_project: Option<&str>) {
    if scope == "project" && memory_project.is_none() {
        return;
    }
    let memory_project = memory_project.filter(|_| scope == "project");
    rows.push(section(tr!("memory.markdown.file.title")));
    rows.push(note(tr!("memory.markdown.file.hint"), Tone::Muted));
    let read = markdown::read_markdown_memory_file(scope, memory_project);
    let file = app.slots.value(Slot::MarkdownMemoryFile);
    let current = app
        .slots
        .loader(Slot::MarkdownMemoryFile)
        .is_some_and(|loader| loader.params == read.params);
    if !current || file.is_null() {
        rows.push(button(
            tr!("memory.markdown.file.edit"),
            call(read),
            Tone::Accent,
        ));
        return;
    }
    let text_value = s(file, "/text");
    rows.push(
        text(
            s(file, "/path"),
            text_value.clone(),
            Edit::Call(Setter::Build {
                ctx: json!({
                    "scope": scope,
                    "memoryProjectId": memory_project,
                    "expected": text_value,
                }),
                build: replace_file,
            }),
        )
        .detail(tr!("memory.markdown.file.edit")),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // The mode decides the pages: off leads to the mode page, each mode keeps its own pages.
    // 模式决定页面：关闭时转到模式页，各模式保留自身页面。
    #[test]
    fn pages_follow_the_memory_mode() {
        assert_eq!(
            page_for_mode(PageId::MemoryContent, None),
            PageId::MemoryContent
        );
        assert_eq!(
            page_for_mode(PageId::MarkdownUser, Some("off")),
            PageId::MemoryMode
        );
        assert_eq!(
            page_for_mode(PageId::MemoryOverview, Some("markdown")),
            PageId::MarkdownUser
        );
        assert_eq!(
            page_for_mode(PageId::MarkdownProject, Some("markdown")),
            PageId::MarkdownProject
        );
        assert_eq!(
            page_for_mode(PageId::MarkdownSpace, Some("vmm")),
            PageId::MemoryOverview
        );
        for mode in ["off", "vmm", "markdown"] {
            assert_eq!(
                page_for_mode(PageId::MemoryMode, Some(mode)),
                PageId::MemoryMode
            );
        }
    }

    // A budget edit keeps the other parts and the mode; out-of-range values send nothing.
    // 预算编辑保留其他分项与模式；超出范围的值不发送请求。
    #[test]
    fn a_budget_edit_saves_the_whole_budget() {
        let settings = json!({
            "mode": "markdown",
            "budget": { "user": 800, "spaceBase": 600 },
            "maxPartTokens": 4000,
        });
        let ctx = json!({ "settings": settings, "part": "spaceBase" });
        let call = save_part(&ctx, &SettingValue::Number(900)).expect("a valid value");
        assert_eq!(call.method, "service_save_memory_settings");
        assert_eq!(
            call.params,
            json!({ "request": { "mode": "markdown", "budget": { "user": 800, "spaceBase": 900 } } })
        );
        assert!(save_part(&ctx, &SettingValue::Number(4001)).is_none());
        assert!(save_part(&ctx, &SettingValue::Number(-1)).is_none());
    }

    // Note writes carry the node revision and leave an absent project out of the request.
    // 笔记写入携带节点修订号，缺失的项目不出现在请求中。
    #[test]
    fn note_writes_carry_the_revision() {
        let ctx = json!({ "projectId": null, "id": "n1", "expectedRevision": "abcd1234" });
        let call = replace_body(&ctx, &SettingValue::Text("new body".into())).unwrap();
        assert_eq!(
            call.params,
            json!({ "request": { "operations": [
                { "action": "replace", "body": "new body", "id": "n1", "expectedRevision": "abcd1234" }
            ] } })
        );
        assert!(rename_node(&ctx, &SettingValue::Text("  ".into())).is_none());
        let added = add_note(json!({
            "projectId": "p1",
            "operation": { "action": "create", "scope": "project", "section": "base", "title": "T", "body": "" },
        }));
        assert_eq!(added.param("/request/projectId"), &json!("p1"));
        assert_eq!(added.param("/request/operations/0/section"), &json!("base"));
    }
}
