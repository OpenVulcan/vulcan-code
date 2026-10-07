// Fake filesystem behind `service_browse_project_directories` and the local attachment listing.
// `service_browse_project_directories` 与本地附件列表背后的伪造文件系统。
//
// The demo never touches the real disk: a flat table of paths stands in for the host so the picker
// can be exercised identically on every machine.
// demo 从不访问真实磁盘：用一张扁平路径表代替宿主，使选择器在任何机器上的表现一致。

use crate::model::{DirEntry, DirectoryListing};

pub const HOME: &str = r"C:\Users\demo";
const ROOTS: [&str; 2] = [r"C:\", r"D:\"];

#[derive(Clone, Copy)]
enum Kind {
    Dir,
    Repo,
    File(u64),
}

/// Repositories in the fake tree; each gets the same template of children.
/// 伪造目录树中的仓库；每个仓库使用同一套子项模板。
const REPOS: [&str; 6] = [
    r"C:\Users\demo\projects\vulcan-demo",
    r"C:\Users\demo\projects\rust-playground",
    r"C:\Users\demo\projects\website",
    r"D:\openvulcan\vulcan-code",
    r"D:\openvulcan\vulcan-code-tui",
    r"D:\openvulcan\luaskills",
];

const PLAIN: [(&str, Kind); 18] = [
    (r"C:\Users", Kind::Dir),
    (r"C:\Program Files", Kind::Dir),
    (r"C:\Windows", Kind::Dir),
    (r"C:\Users\demo", Kind::Dir),
    (r"C:\Users\Public", Kind::Dir),
    (r"C:\Users\demo\.config", Kind::Dir),
    (r"C:\Users\demo\Documents", Kind::Dir),
    (r"C:\Users\demo\Downloads", Kind::Dir),
    (r"C:\Users\demo\projects", Kind::Dir),
    (r"C:\Users\demo\notes.md", Kind::File(2_140)),
    (r"C:\Users\demo\avatar.png", Kind::File(48_912)),
    (r"C:\Users\demo\Documents\roadmap.pdf", Kind::File(412_004)),
    (
        r"C:\Users\demo\Documents\meeting-notes.md",
        Kind::File(6_820),
    ),
    (
        r"C:\Users\demo\Downloads\screenshot.png",
        Kind::File(233_190),
    ),
    (r"C:\Users\demo\projects\README.md", Kind::File(512)),
    (r"D:\openvulcan", Kind::Dir),
    (r"D:\openvulcan\design-assets", Kind::Dir),
    (
        r"D:\openvulcan\design-assets\palette.json",
        Kind::File(1_024),
    ),
];

const REPO_CHILDREN: [(&str, Kind); 13] = [
    (".git", Kind::Dir),
    (".github", Kind::Dir),
    (".gitignore", Kind::File(96)),
    ("docs", Kind::Dir),
    ("src", Kind::Dir),
    ("tests", Kind::Dir),
    ("Cargo.toml", Kind::File(1_180)),
    ("README.md", Kind::File(4_300)),
    (r"docs\design.md", Kind::File(9_870)),
    (r"src\main.rs", Kind::File(3_412)),
    (r"src\lib.rs", Kind::File(640)),
    (r"src\app.rs", Kind::File(18_220)),
    (r"tests\smoke.rs", Kind::File(2_048)),
];

fn table() -> Vec<(String, Kind)> {
    let mut entries: Vec<(String, Kind)> = PLAIN
        .iter()
        .map(|(path, kind)| (path.to_string(), *kind))
        .collect();
    for repo in REPOS {
        entries.push((repo.to_string(), Kind::Repo));
        for (child, kind) in REPO_CHILDREN {
            entries.push((format!(r"{repo}\{child}"), kind));
        }
    }
    entries
}

fn normalize(path: &str) -> String {
    let path = path.trim().replace('/', r"\");
    let trimmed = path.trim_end_matches('\\');
    if trimmed.len() == 2 && trimmed.ends_with(':') {
        format!(r"{}\", trimmed.to_uppercase())
    } else {
        trimmed.to_string()
    }
}

fn parent_of(path: &str) -> Option<String> {
    if ROOTS.iter().any(|root| root.eq_ignore_ascii_case(path)) {
        return None;
    }
    let (parent, _) = path.rsplit_once('\\')?;
    Some(normalize(parent))
}

/// Lists one directory of the fake tree; unknown paths return an error listing.
/// 列出伪造目录树中的一个目录；未知路径返回带错误的结果。
pub fn list(path: Option<&str>, show_hidden: bool, files: bool) -> DirectoryListing {
    let path = normalize(path.unwrap_or(HOME));
    let table = table();
    let exists = ROOTS.iter().any(|root| root.eq_ignore_ascii_case(&path))
        || table
            .iter()
            .any(|(p, kind)| p.eq_ignore_ascii_case(&path) && !matches!(kind, Kind::File(_)));
    let roots = ROOTS.iter().map(|r| r.to_string()).collect();
    if !exists {
        return DirectoryListing {
            path: path.clone(),
            parent: parent_of(&path),
            roots,
            entries: Vec::new(),
            error: Some(format!("Directory not found: {path}")),
        };
    }
    let mut entries: Vec<DirEntry> = table
        .iter()
        .filter(|(p, _)| parent_of(p).is_some_and(|parent| parent.eq_ignore_ascii_case(&path)))
        .map(|(p, kind)| {
            let name = p.rsplit('\\').next().unwrap_or(p).to_string();
            DirEntry {
                hidden: name.starts_with('.'),
                is_dir: !matches!(kind, Kind::File(_)),
                is_repo: matches!(kind, Kind::Repo),
                size: match kind {
                    Kind::File(size) => Some(*size),
                    _ => None,
                },
                name,
            }
        })
        .filter(|e| show_hidden || !e.hidden)
        .filter(|e| files || e.is_dir)
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    DirectoryListing {
        parent: parent_of(&path),
        path,
        roots,
        entries,
        error: None,
    }
}

/// True when `path` names a directory in the fake tree.
/// `path` 是否为伪造目录树中的目录。
pub fn is_dir(path: &str) -> bool {
    list(Some(path), true, false).error.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_home_parent_and_repos() {
        let home = list(None, false, false);
        assert_eq!(home.path, HOME);
        assert_eq!(home.parent.as_deref(), Some(r"C:\Users"));
        assert!(home.entries.iter().any(|e| e.name == "projects"));
        assert!(home.entries.iter().all(|e| e.is_dir && !e.hidden));
        let projects = list(Some(r"c:/users/demo/projects/"), false, true);
        assert!(projects.error.is_none());
        assert!(
            projects
                .entries
                .iter()
                .any(|e| e.name == "vulcan-demo" && e.is_repo)
        );
        assert!(
            projects
                .entries
                .iter()
                .any(|e| e.name == "README.md" && !e.is_dir)
        );
        let root = list(Some("D:"), false, false);
        assert_eq!(root.path, r"D:\");
        assert!(root.parent.is_none());
        assert!(list(Some(r"Q:\nowhere"), false, false).error.is_some());
    }
}
