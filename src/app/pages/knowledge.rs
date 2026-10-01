// Knowledge pages: the library list, one library's settings, sources and index health, and search.
// 知识库页面：知识库列表、单个知识库的设置、来源与索引健康度，以及搜索。

use super::*;
use crate::app::action::LocalSetting;
use crate::ui::text::compact_count;

fn status_label(status: &str) -> (String, Tone) {
    match status {
        "empty" => (tr!("knowledge.indexStatus.empty"), Tone::Muted),
        "ready" => (tr!("knowledge.indexStatus.ready"), Tone::Good),
        "dirty" => (tr!("knowledge.indexStatus.dirty"), Tone::Warn),
        "rebuilding" => (tr!("knowledge.indexStatus.rebuilding"), Tone::Warn),
        "failed" => (tr!("knowledge.indexStatus.failed"), Tone::Bad),
        other => (other.to_string(), Tone::Muted),
    }
}

/// The library shown on the details page: the chosen one, else the first.
/// 详情页显示的知识库：已选择的，否则为第一个。
fn selected<'a>(app: &App, system: &'a SystemSnapshot) -> Option<&'a KnowledgeBase> {
    app.library
        .as_ref()
        .and_then(|id| system.knowledge.iter().find(|b| b.id == *id))
        .or_else(|| system.knowledge.first())
}

pub(super) fn libraries(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let bases = &system.knowledge;
    let mut rows = vec![
        note(tr!("knowledge.defaultDescription"), Tone::Muted),
        stats(vec![
            (tr!("area.knowledge"), bases.len().to_string(), Tone::Normal),
            (
                tr!("knowledge.documents"),
                bases.iter().map(|b| b.documents).sum::<u32>().to_string(),
                Tone::Normal,
            ),
            (
                tr!("tui.knowledge.chunks"),
                compact_count(bases.iter().map(|b| b.chunks as u64).sum()),
                Tone::Normal,
            ),
            (
                tr!("knowledge.sessionDefaultEnabled"),
                bases
                    .iter()
                    .filter(|b| b.session_default)
                    .count()
                    .to_string(),
                Tone::Accent,
            ),
        ]),
        section(tr!("area.knowledge")),
    ];
    if bases.is_empty() {
        rows.push(note(tr!("knowledge.emptyLibraries"), Tone::Muted));
    }
    let current = selected(app, system).map(|b| b.id.clone());
    for base in bases {
        let (status, tone) = status_label(&base.status);
        let mut actions = vec![
            action(
                tr!("knowledge.view"),
                UiAction::OpenLibrary(base.id.clone()),
                Tone::Accent,
            ),
            switch(
                tr!("settings.mcp.enabled"),
                base.enabled,
                Edit::Enabled(EntityKind::Knowledge, base.id.clone()),
            ),
        ];
        if base.job.is_none() {
            actions.push(action(
                tr!("knowledge.refresh"),
                sys_action(SystemCommand::KnowledgeReindex(base.id.clone())),
                Tone::Normal,
            ));
        }
        let mut name = base.name.clone();
        if base.session_default {
            name = format!("{name}  ★");
        }
        if current.as_deref() == Some(base.id.as_str()) {
            name = format!("{name}  ‹");
        }
        rows.push(
            item(
                &base.id,
                name,
                tr!(
                    "knowledge.libraryCounts",
                    documents = base.documents,
                    chunks = compact_count(base.chunks as u64),
                    status = mode_label(&base.mode)
                ),
            )
            .badge(status, tone)
            .actions(actions)
            .job(base.job.clone()),
        );
    }
    rows.push(button(
        format!("+ {}", tr!("knowledge.experience.createTitle")),
        prompt(
            tr!("knowledge.experience.createTitle"),
            tr!("knowledge.name"),
            PromptThen::KnowledgeCreate,
            false,
        ),
        Tone::Accent,
    ));
    rows
}

fn mode_label(mode: &str) -> String {
    match mode {
        "raw" => tr!("knowledge.experience.rawSearch"),
        "display" => tr!("knowledge.experience.displaySearch"),
        "hybrid" => tr!("knowledge.experience.hybridSearch"),
        other => other.to_string(),
    }
}

pub(super) fn library(app: &App, system: &SystemSnapshot) -> Vec<Row> {
    let Some(base) = selected(app, system) else {
        return vec![note(tr!("knowledge.emptySelection"), Tone::Muted)];
    };
    let field = |name: &str| Edit::Field(EntityKind::Knowledge, base.id.clone(), name.to_string());
    let (status, status_tone) = status_label(&base.status);
    let mut rows = vec![
        choice(
            tr!("tui.knowledge.library"),
            system
                .knowledge
                .iter()
                .map(|b| (b.id.clone(), b.name.clone()))
                .collect(),
            base.id.clone(),
            Edit::Local(LocalSetting::Library),
        ),
        stats(vec![
            (
                tr!("knowledge.documents"),
                base.documents.to_string(),
                Tone::Normal,
            ),
            (
                tr!("tui.knowledge.chunks"),
                compact_count(base.chunks as u64),
                Tone::Normal,
            ),
            (
                tr!("knowledge.sources.title"),
                base.sources.len().to_string(),
                Tone::Normal,
            ),
            (tr!("agentSkills.status"), status, status_tone),
        ]),
    ];
    if let Some(job) = &base.job {
        job_rows(&mut rows, std::slice::from_ref(job));
    }

    rows.push(section(tr!("knowledge.experience.settingsTitle")));
    rows.push(toggle(
        tr!("settings.mcp.enabled"),
        base.enabled,
        Edit::Enabled(EntityKind::Knowledge, base.id.clone()),
    ));
    rows.push(
        toggle(
            tr!("knowledge.sessionDefaultEnabled"),
            base.session_default,
            field("sessionDefault"),
        )
        .detail(tr!("knowledge.selectionInherited")),
    );
    rows.push(
        choice(
            tr!("knowledge.experience.retrievalMode"),
            vec![
                ("raw".into(), tr!("knowledge.experience.rawMode")),
                ("display".into(), tr!("knowledge.experience.displayMode")),
                ("hybrid".into(), tr!("knowledge.experience.hybridMode")),
            ],
            base.mode.clone(),
            field("mode"),
        )
        .detail(tr!("tui.knowledge.modeNote")),
    );

    rows.push(section(tr!("knowledge.sources.title")));
    rows.push(note(tr!("knowledge.sources.description"), Tone::Muted));
    if base.sources.is_empty() {
        rows.push(note(tr!("knowledge.sources.empty"), Tone::Muted));
    }
    let source_op = |op: SourceOp| {
        sys_action(SystemCommand::KnowledgeSource {
            library: base.id.clone(),
            op,
        })
    };
    for source in &base.sources {
        rows.push(
            item(
                &source.id,
                source.path.clone(),
                format!(
                    "{} · {}",
                    tr!(
                        "knowledge.sources.counts",
                        documents = source.documents,
                        chunks = compact_count(source.chunks as u64)
                    ),
                    tr!("knowledge.sources.lastScanned", time = source.last_scanned)
                ),
            )
            .actions(vec![
                action(
                    tr!("knowledge.sources.sync"),
                    source_op(SourceOp::Sync(source.id.clone())),
                    Tone::Accent,
                ),
                action(
                    tr!("knowledge.delete"),
                    confirm(
                        tr!("knowledge.delete"),
                        tr!("tui.knowledge.removeSourceBody", path = source.path),
                        tr!("knowledge.delete"),
                        source_op(SourceOp::Remove(source.id.clone())),
                    ),
                    Tone::Bad,
                ),
            ]),
        );
    }
    rows.push(button(
        format!("+ {}", tr!("knowledge.sources.add")),
        UiAction::AddKnowledgeSource(base.id.clone()),
        Tone::Accent,
    ));

    let health = &base.health;
    let searchable = health.searchable();
    rows.push(section(tr!("knowledge.indexHealth.title")));
    rows.push(note(tr!("knowledge.indexHealth.description"), Tone::Muted));
    rows.push(info(
        tr!("agentSkills.status"),
        if searchable {
            tr!("knowledge.indexHealth.searchable")
        } else {
            tr!("knowledge.indexHealth.needsRepair")
        },
        if searchable { Tone::Good } else { Tone::Bad },
    ));
    let ratio = if health.authoritative == 0 {
        1.0
    } else {
        health.indexed as f64 / health.authoritative as f64
    };
    rows.push(meter(
        tr!("knowledge.indexHealth.indexed"),
        ratio,
        format!("{} / {}", health.indexed, health.authoritative),
        if searchable { Tone::Good } else { Tone::Warn },
    ));
    let problem = |n: u32| if n > 0 { Tone::Bad } else { Tone::Normal };
    rows.push(stats(vec![
        (
            tr!("knowledge.indexHealth.authoritative"),
            health.authoritative.to_string(),
            Tone::Normal,
        ),
        (
            tr!("knowledge.indexHealth.missing"),
            health.missing.to_string(),
            problem(health.missing),
        ),
        (
            tr!("knowledge.indexHealth.orphaned"),
            health.orphaned.to_string(),
            problem(health.orphaned),
        ),
    ]));
    let revision = |(indexed, current): (u32, u32)| {
        if indexed == current {
            Tone::Muted
        } else {
            Tone::Warn
        }
    };
    rows.push(note(
        tr!(
            "knowledge.indexHealth.contentRevision",
            indexed = health.content_revision.0,
            current = health.content_revision.1
        ),
        revision(health.content_revision),
    ));
    rows.push(note(
        tr!(
            "knowledge.indexHealth.dictionaryRevision",
            indexed = health.dictionary_revision.0,
            current = health.dictionary_revision.1
        ),
        revision(health.dictionary_revision),
    ));
    rows.push(note(
        tr!(
            "knowledge.indexHealth.tokenizer",
            tokenizer = health.tokenizer
        ),
        Tone::Muted,
    ));
    if base.job.is_none() {
        rows.push(button(
            tr!("knowledge.repairIndex"),
            sys_action(SystemCommand::KnowledgeRepair(base.id.clone())),
            if searchable {
                Tone::Normal
            } else {
                Tone::Accent
            },
        ));
        rows.push(button(
            tr!("knowledge.rebuildIndex"),
            confirm(
                tr!("knowledge.operationDialog.title.rebuild"),
                tr!("tui.knowledge.rebuildBody", name = base.name),
                tr!("knowledge.rebuildIndex"),
                sys_action(SystemCommand::KnowledgeReindex(base.id.clone())),
            ),
            Tone::Normal,
        ));
    }
    rows.push(spacer());
    rows.push(button(
        tr!("knowledge.delete"),
        confirm(
            tr!("knowledge.delete"),
            tr!("knowledge.deleteConfirm", name = base.name),
            tr!("knowledge.delete"),
            sys_action(SystemCommand::KnowledgeDelete(base.id.clone())),
        ),
        Tone::Bad,
    ));
    rows
}

pub(super) fn search(system: &SystemSnapshot) -> Vec<Row> {
    let mut rows = vec![button(
        format!("⌕ {}", tr!("knowledge.search")),
        PageId::KnowledgeSearch
            .search()
            .expect("knowledge search has a prompt"),
        Tone::Accent,
    )];
    match &system.knowledge_search {
        None => rows.push(note(tr!("tui.knowledge.searchNote"), Tone::Muted)),
        Some((query, hits)) => {
            rows.push(info(
                tr!("tui.page.query"),
                format!("“{query}” · {}", hits.len()),
                Tone::Accent,
            ));
            rows.push(button(
                tr!("tui.page.clearSearch"),
                sys_action(SystemCommand::KnowledgeSearch(String::new())),
                Tone::Muted,
            ));
            if hits.is_empty() {
                rows.push(note(tr!("knowledge.search.empty"), Tone::Muted));
            }
            for (index, hit) in hits.iter().enumerate() {
                rows.push(
                    item(
                        &format!("hit-{index}"),
                        hit.title.clone(),
                        format!("{:.2}", hit.score),
                    )
                    .lines(vec![hit.snippet.clone()]),
                );
            }
        }
    }
    rows
}
