use super::*;
use crate::{InjectionBudget, NodeSource, ProjectContext, WriteOperation};

/// Builds a store in a temporary user root; the second value keeps the directory alive.
/// 在临时用户根目录中构建存储；第二个值保持目录存在。
fn store() -> (MarkdownMemoryStore, tempfile::TempDir) {
    let root = tempfile::tempdir().expect("temp dir");
    (MarkdownMemoryStore::new(root.path()), root)
}

/// A context of host project `host` rooted at `path`.
/// 根目录为 `path` 的宿主项目 `host` 的上下文。
fn context(host: &str, path: &str) -> ProjectContext {
    ProjectContext {
        host_project_id: Some(host.to_string()),
        root_path: Some(path.to_string()),
        memory_project_id: None,
    }
}

/// Writes operations as the user.
/// 以用户身份写入操作。
fn write(store: &MarkdownMemoryStore, context: &ProjectContext, operations: serde_json::Value) {
    let operations: Vec<WriteOperation> =
        serde_json::from_value(operations).expect("valid operations");
    store
        .write(
            context,
            None,
            NodeSource::User,
            &InjectionBudget::default(),
            operations,
        )
        .expect("write succeeds");
}

/// Returns the view at a title path of a scope.
/// 返回某层中位于标题链的视图。
fn view(
    store: &MarkdownMemoryStore,
    scope: MemoryScope,
    project: Option<&str>,
    path: &[&str],
) -> NodeView {
    store
        .load(scope, project)
        .expect("load")
        .views(scope)
        .into_iter()
        .find(|view| view.path == path)
        .unwrap_or_else(|| panic!("no node at {path:?}"))
}

/// Seeds a user note, a space note and one project note, and returns the memory project id.
/// 写入一条用户笔记、一条空间笔记与一条项目笔记，并返回记忆项目标识。
fn seed(store: &MarkdownMemoryStore) -> String {
    let project = context("host-a", "/work/alpha");
    write(
        store,
        &project,
        serde_json::json!([
            { "action": "create", "scope": "user", "section": "identity", "title": "Name", "body": "Ada" },
            { "action": "create", "scope": "space", "section": "base", "title": "Shell", "body": "PowerShell" },
            { "action": "create", "scope": "project", "section": "pitfalls", "title": "Line endings", "body": "CRLF" },
        ]),
    );
    store.registry().unwrap().projects[0].id.clone()
}

/// Exports everything of a seeded store into its own root.
/// 将已写入存储的全部内容导出到其根目录。
fn export_all(store: &MarkdownMemoryStore, root: &tempfile::TempDir, project: &str) -> PathBuf {
    let exported = store
        .export_package(
            &ExportSelection {
                user: true,
                space: true,
                memory_project_ids: vec![project.to_string()],
            },
            root.path(),
        )
        .expect("export");
    PathBuf::from(exported.path)
}

/// A mapping of one packaged project.
/// 单个包内项目的去向。
fn mapping(package_project_id: &str, target: ProjectTarget) -> ProjectMapping {
    ProjectMapping {
        package_project_id: package_project_id.to_string(),
        target,
    }
}

// Importing a package into the store it came from changes nothing, so no file is backed up.
// 将导出包导入其来源存储不会改变任何内容，因此不备份任何文件。
#[test]
fn importing_into_the_source_store_is_a_no_op() {
    let (store, root) = store();
    let project = seed(&store);
    let archive = export_all(&store, &root, &project);
    let mappings = [mapping(
        &project,
        ProjectTarget::Link {
            memory_project_id: project.clone(),
        },
    )];
    let preview = store.preview_import(&archive, &mappings).unwrap();
    assert_eq!(preview.scopes.len(), 3);
    for scope in &preview.scopes {
        assert!(
            scope.added.is_empty() && scope.conflicts.is_empty(),
            "{scope:?}"
        );
        assert!(scope.same > 0);
    }
    let report = store
        .apply_import(&archive, &preview.sha256, &mappings, &[])
        .unwrap();
    assert_eq!((report.added, report.replaced), (0, 0));
    assert_eq!(report.backup_directory, None);
}

// A fresh store receives every node, preset sections match by key instead of duplicating, and a
// second import of the same package finds everything identical.
// 新存储接收全部节点；预置分区按键匹配而不重复；同一导出包第二次导入时全部相同。
#[test]
fn a_fresh_store_receives_everything_once() {
    let (source, source_root) = store();
    let project = seed(&source);
    let archive = export_all(&source, &source_root, &project);

    let (target, _target_root) = store();
    // The target already has its own preset sections with other ids.
    // 目标已有自己的预置分区，标识不同。
    write(
        &target,
        &context("host-b", "/elsewhere/beta"),
        serde_json::json!([{ "action": "create", "scope": "user", "section": "expertise", "title": "Rust", "body": "Daily" }]),
    );
    let preview = target.preview_import(&archive, &[]).unwrap();
    assert_eq!(preview.projects.len(), 1);
    assert!(preview.projects[0].candidates.is_empty());
    let user = &preview.scopes[0];
    assert_eq!(user.scope, MemoryScope::User);
    assert!(user.new_sections.is_empty(), "preset sections match by key");
    assert_eq!(user.added.len(), 1);
    assert!(
        target
            .apply_import(&archive, &preview.sha256, &[], &[])
            .is_err(),
        "every packaged project needs a mapping"
    );

    let mappings = [mapping(&project, ProjectTarget::Create)];
    let report = target
        .apply_import(&archive, &preview.sha256, &mappings, &[])
        .unwrap();
    assert_eq!(report.created_projects.len(), 1);
    assert!(report.backup_directory.is_some(), "the user file existed");
    let created = &report.created_projects[0];
    assert_eq!(
        view(
            &target,
            MemoryScope::Project,
            Some(created),
            &["Pitfalls", "Line endings"]
        )
        .body,
        "CRLF"
    );
    assert_eq!(
        view(&target, MemoryScope::User, None, &["Identity", "Name"]).body,
        "Ada"
    );
    let sections = target
        .load(MemoryScope::User, None)
        .unwrap()
        .roots
        .iter()
        .filter(|root| root.title == "Identity")
        .count();
    assert_eq!(sections, 1);

    let again = [mapping(
        &project,
        ProjectTarget::Link {
            memory_project_id: created.clone(),
        },
    )];
    let second = target.preview_import(&archive, &again).unwrap();
    for scope in &second.scopes {
        assert!(
            scope.added.is_empty() && scope.conflicts.is_empty(),
            "a repeated import is identical: {scope:?}"
        );
    }
}

// A changed node is a conflict that must be resolved; taking the imported side replaces it after
// backing up the file, and a resolution for an unknown conflict is refused.
// 内容不同的节点是必须解决的冲突；采用导入一侧会在备份文件后替换它，未知冲突的解决会被拒绝。
#[test]
fn conflicts_need_a_resolution() {
    let (store, root) = store();
    let project = seed(&store);
    let archive = export_all(&store, &root, &project);
    let name = view(&store, MemoryScope::User, None, &["Identity", "Name"]);
    write(
        &store,
        &context("host-a", "/work/alpha"),
        serde_json::json!([{ "action": "replace", "id": name.id, "expectedRevision": name.revision, "body": "Grace" }]),
    );
    let mappings = [mapping(&project, ProjectTarget::Ignore)];
    let preview = store.preview_import(&archive, &mappings).unwrap();
    assert_eq!(preview.scopes.len(), 2, "an ignored project is not merged");
    let conflict = &preview.scopes[0].conflicts[0];
    assert_eq!(
        (
            conflict.local.body.as_str(),
            conflict.imported.body.as_str()
        ),
        ("Grace", "Ada")
    );

    assert!(
        store
            .apply_import(&archive, &preview.sha256, &mappings, &[])
            .is_err()
    );
    let unknown = ConflictResolution {
        scope: MemoryScope::Space,
        package_project_id: None,
        node_id: String::from("m-missing"),
        choice: ConflictChoice::KeepLocal,
    };
    let take = ConflictResolution {
        scope: MemoryScope::User,
        package_project_id: None,
        node_id: conflict.id.clone(),
        choice: ConflictChoice::TakeImported,
    };
    assert!(
        store
            .apply_import(
                &archive,
                &preview.sha256,
                &mappings,
                &[take.clone(), unknown]
            )
            .is_err()
    );
    assert!(
        store
            .apply_import(
                &archive,
                "0".repeat(64).as_str(),
                &mappings,
                std::slice::from_ref(&take)
            )
            .is_err(),
        "the previewed bytes are required"
    );
    let report = store
        .apply_import(&archive, &preview.sha256, &mappings, &[take])
        .unwrap();
    assert_eq!(report.replaced, 1);
    assert_eq!(
        view(&store, MemoryScope::User, None, &["Identity", "Name"]).body,
        "Ada"
    );
    let backup = PathBuf::from(report.backup_directory.expect("backup"));
    assert!(
        std::fs::read_to_string(backup.join("user.md"))
            .unwrap()
            .contains("Grace")
    );
}

// Keeping both sides adds the imported node under a new id beside the local one.
// 两者都留会以新标识在本地节点旁添加导入节点。
#[test]
fn keeping_both_adds_a_copy() {
    let (store, root) = store();
    let project = seed(&store);
    let archive = export_all(&store, &root, &project);
    let shell = view(&store, MemoryScope::Space, None, &["Base", "Shell"]);
    write(
        &store,
        &context("host-a", "/work/alpha"),
        serde_json::json!([{ "action": "replace", "id": shell.id, "expectedRevision": shell.revision, "body": "Bash" }]),
    );
    let mappings = [mapping(&project, ProjectTarget::Ignore)];
    let preview = store.preview_import(&archive, &mappings).unwrap();
    let report = store
        .apply_import(
            &archive,
            &preview.sha256,
            &mappings,
            &[ConflictResolution {
                scope: MemoryScope::Space,
                package_project_id: None,
                node_id: shell.id.clone(),
                choice: ConflictChoice::KeepBoth,
            }],
        )
        .unwrap();
    assert_eq!(report.added, 1);
    let shells = store
        .load(MemoryScope::Space, None)
        .unwrap()
        .views(MemoryScope::Space)
        .into_iter()
        .filter(|view| view.path == ["Base", "Shell"])
        .map(|view| (view.id, view.body))
        .collect::<Vec<_>>();
    assert_eq!(shells.len(), 2);
    assert!(
        shells
            .iter()
            .any(|(id, body)| *id == shell.id && body == "Bash")
    );
    assert!(
        shells
            .iter()
            .any(|(id, body)| *id != shell.id && body == "PowerShell")
    );
}

// A same-path node under another id is reported as a duplicate and kept; a project is suggested
// by its path.
// 不同标识但标题链相同的节点被报告为疑似重复并保留；项目按路径给出建议。
#[test]
fn duplicates_are_kept_and_projects_are_suggested() {
    let (source, source_root) = store();
    let project = seed(&source);
    let archive = export_all(&source, &source_root, &project);
    let (target, _target_root) = store();
    write(
        &target,
        &context("host-z", "/work/alpha"),
        serde_json::json!([
            { "action": "create", "scope": "user", "section": "identity", "title": "Name", "body": "Someone" },
            { "action": "create", "scope": "project", "section": "base", "title": "Build", "body": "make" },
        ]),
    );
    let preview = target.preview_import(&archive, &[]).unwrap();
    assert_eq!(preview.scopes[0].duplicates.len(), 1);
    let candidate = &preview.projects[0].candidates[0];
    assert!(candidate.reasons.iter().any(|reason| reason == "same_path"));
}

// Packages with another version or an undeclared entry are refused.
// 版本不同或含未声明条目的导出包会被拒绝。
#[test]
fn foreign_packages_are_refused() {
    let (store, root) = store();
    let manifest = |version: u32| {
        serde_json::json!({
            "format": PACKAGE_FORMAT, "version": version, "exportedAt": "2026-10-20T10:00Z",
            "user": true, "space": false, "projects": [],
        })
        .to_string()
    };
    let write_package = |name: &str, manifest: String, entries: &[(&str, &str)]| {
        let entries = entries
            .iter()
            .map(|(name, text)| (name.to_string(), text.to_string()))
            .collect::<Vec<_>>();
        let path = root.path().join(name);
        std::fs::write(&path, write_archive(&manifest, &entries).unwrap()).unwrap();
        path
    };
    let valid = write_package(
        "valid.zip",
        manifest(EXPORT_FORMAT_VERSION),
        &[(USER_ENTRY, "# Identity\n")],
    );
    assert!(store.preview_import(&valid, &[]).is_ok());
    let newer = write_package(
        "newer.zip",
        manifest(EXPORT_FORMAT_VERSION + 1),
        &[(USER_ENTRY, "")],
    );
    assert!(matches!(
        store.preview_import(&newer, &[]),
        Err(MemoryError::UnsupportedFormat(_))
    ));
    let extra = write_package(
        "extra.zip",
        manifest(EXPORT_FORMAT_VERSION),
        &[(USER_ENTRY, ""), ("notes.txt", "")],
    );
    assert!(store.preview_import(&extra, &[]).is_err());
    let missing = write_package("missing.zip", manifest(EXPORT_FORMAT_VERSION), &[]);
    assert!(store.preview_import(&missing, &[]).is_err());
}
