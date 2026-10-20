use super::*;
use crate::{MemoryDocument, MemoryProject};

/// Builds scope documents from file texts.
/// 由文件文本构建各层文档。
fn documents(user: &str, space: &str, project: Option<&str>) -> ScopeDocuments {
    ScopeDocuments {
        user: MemoryDocument::parse(user),
        space: MemoryDocument::parse(space),
        project: project.map(|text| {
            (
                MemoryProject {
                    id: String::from("p-1"),
                    name: String::from("demo"),
                    host_project_ids: vec![String::from("host-1")],
                    paths: Vec::new(),
                    first_commit: None,
                    memory_enabled: true,
                    created: String::from("2026-10-20T00:00Z"),
                    updated: String::from("2026-10-20T00:00Z"),
                },
                MemoryDocument::parse(text),
            )
        }),
    }
}

/// Builds a pattern request.
/// 构建模式请求。
fn patterns(values: &[&str]) -> SearchRequest {
    SearchRequest {
        patterns: values.iter().map(|value| (*value).to_string()).collect(),
        ..SearchRequest::default()
    }
}

/// Hits rank by distinct patterns, then title hits, then scope priority.
/// 命中按不同模式数、标题命中、层优先级排序。
#[test]
fn hits_rank_by_patterns_title_and_scope() {
    let docs = documents(
        "# Identity\n\n## Editor\n\nUses neovim.\n",
        "# Conventions\n\n## Rust\n\nRun cargo check and cargo test.\n",
        Some(
            "# Pitfalls\n\n## Cargo\n\nLinking dominates.\n\n## Tests\n\nUse cargo nextest with rust.\n",
        ),
    );
    let page = search_nodes(&docs, &patterns(&["cargo", "rust"])).expect("search");
    let titles = page
        .hits
        .iter()
        .map(|hit| hit.view.path.last().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(titles, ["Rust", "Tests", "Cargo"]);
    assert_eq!(page.hits[0].matched_patterns, 2);
    assert!(page.hits[0].title_hit);
    assert!(!page.hits[1].title_hit);
    assert!(
        page.hits[1]
            .text
            .lines()
            .any(|line| line.starts_with(HIT_MARKER))
    );
}

/// A hit belongs to the deepest node whose own body matched, not to its ancestors.
/// 命中归属于自身正文匹配的最深节点，而非其祖先。
#[test]
fn hit_belongs_to_deepest_node() {
    let docs = documents("# Identity\n\n## Name\n\nAda Lovelace\n", "", None);
    let page = search_nodes(&docs, &patterns(&["lovelace"])).expect("search");
    assert_eq!(page.total, 1);
    assert_eq!(page.hits[0].view.path, ["Identity", "Name"]);
}

/// Invalid argument combinations and regexes are refused.
/// 非法参数组合与正则会被拒绝。
#[test]
fn invalid_requests_are_refused() {
    let docs = documents("", "", None);
    assert!(search_nodes(&docs, &SearchRequest::default()).is_err());
    let both = SearchRequest {
        ids: vec![String::from("m-1")],
        ..patterns(&["x"])
    };
    assert!(search_nodes(&docs, &both).is_err());
    assert!(search_nodes(&docs, &patterns(&["("])).is_err());
    let literal = SearchRequest {
        fixed_strings: true,
        ..patterns(&["("])
    };
    assert!(search_nodes(&docs, &literal).is_ok());
    let too_many = patterns(&["a"; MAX_SEARCH_PATTERNS + 1]);
    assert!(search_nodes(&docs, &too_many).is_err());
}

/// Results page with a cursor and the rendered text points at the next cursor.
/// 结果按游标分页，渲染文本指向下一游标。
#[test]
fn results_page_with_cursor() {
    let space = (1..=5)
        .map(|index| format!("## Note {index}\n\nshared term\n"))
        .collect::<Vec<_>>()
        .join("\n");
    let docs = documents("", &format!("# Notes\n\n{space}"), None);
    let first = SearchRequest {
        limit: Some(2),
        ..patterns(&["shared"])
    };
    let page = search_nodes(&docs, &first).expect("first page");
    assert_eq!((page.total, page.hits.len(), page.offset), (5, 2, 0));
    assert!(render_search_page(&page).contains("cursor=\"3\""));
    let next = SearchRequest {
        cursor: Some(String::from("3")),
        ..first
    };
    let page = search_nodes(&docs, &next).expect("next page");
    assert_eq!((page.hits.len(), page.offset), (2, 2));
}

/// Reading a long node by id pages its body on line boundaries.
/// 按标识读取长节点时在行边界分页正文。
#[test]
fn long_node_pages_by_id() {
    let line = "x".repeat(99);
    let body = std::iter::repeat_n(line.as_str(), NODE_PAGE_BYTES / 100 * 2 + 10)
        .collect::<Vec<_>>()
        .join("\n");
    let docs = documents(&format!("# Big\n\n{body}\n"), "", None);
    let id = docs.user.views(MemoryScope::User)[0].id.clone();
    let request = SearchRequest {
        ids: vec![id.clone()],
        ..SearchRequest::default()
    };
    let page = search_nodes(&docs, &request).expect("first page");
    let hit = &page.hits[0];
    assert_eq!((hit.page, hit.pages), (1, 3));
    assert!(hit.text.len() <= NODE_PAGE_BYTES);
    assert!(hit.text.lines().all(|text| text.len() == line.len()));
    let last = SearchRequest {
        cursor: Some(String::from("3")),
        ..request.clone()
    };
    assert_eq!(
        search_nodes(&docs, &last).expect("last page").hits[0].page,
        3
    );
    let beyond = SearchRequest {
        cursor: Some(String::from("4")),
        ..request
    };
    assert!(search_nodes(&docs, &beyond).is_err());
}

/// Section and path-prefix filters narrow results.
/// 分区与路径前缀过滤会缩小结果。
#[test]
fn filters_narrow_results() {
    let docs = documents(
        "",
        "# Conventions\n\n## Git\n\nterm\n\n### Hooks\n\nterm\n\n# Environment\n\n## Shell\n\nterm\n",
        None,
    );
    let by_section = SearchRequest {
        section: Some(String::from("environment")),
        ..patterns(&["term"])
    };
    assert_eq!(search_nodes(&docs, &by_section).expect("section").total, 1);
    let by_prefix = SearchRequest {
        path_prefix: Some(String::from("conventions / git")),
        ..patterns(&["term"])
    };
    assert_eq!(search_nodes(&docs, &by_prefix).expect("prefix").total, 2);
}

/// The outline lists nodes up to the requested depth and counts what lies below.
/// 大纲列出到指定深度的节点并统计其下内容。
#[test]
fn outline_respects_depth() {
    let docs = documents(
        "",
        "# Conventions\n\n## Git\n\n### Hooks\n\nbody\n",
        Some("# Pitfalls\n"),
    );
    let shallow = render_outline(
        &docs,
        &OutlineRequest {
            depth: Some(1),
            ..OutlineRequest::default()
        },
    )
    .expect("outline");
    assert!(
        shallow
            .lines()
            .any(|line| line.starts_with("# Conventions") && line.contains("+2"))
    );
    assert!(!shallow.lines().any(|line| line.starts_with("## ")));
    assert!(shallow.contains("[project demo]"));
    let full = render_outline(&docs, &OutlineRequest::default()).expect("outline");
    assert!(full.lines().any(|line| line.starts_with("### Hooks")));
    let invalid = OutlineRequest {
        depth: Some(5),
        ..OutlineRequest::default()
    };
    assert!(render_outline(&docs, &invalid).is_err());
}

/// An empty result tells the model how to continue.
/// 空结果会告诉模型如何继续。
#[test]
fn empty_result_renders_a_hint() {
    let docs = documents("# Identity\n", "", None);
    let page = search_nodes(&docs, &patterns(&["absent"])).expect("search");
    assert_eq!(page.total, 0);
    assert!(!render_search_page(&page).is_empty());
}
