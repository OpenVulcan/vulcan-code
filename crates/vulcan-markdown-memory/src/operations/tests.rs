use super::*;

/// A Windows sharing lock on the destination cannot lose a node moved from another scope.
/// Windows 目标文件共享锁不会导致从其他层移动的节点丢失。
#[cfg(windows)]
#[test]
fn failed_cross_scope_move_preserves_both_original_files() {
    use std::os::windows::fs::OpenOptionsExt;
    // Seed both scopes through the real write API before locking the target's directory entry.
    // 通过真实写入 API 初始化两个层，再锁定目标目录项。
    let (root, store) = store();
    let context = project_context(&root);
    let receipt = write(&store, &context, vec![
        serde_json::json!({"action":"create","scope":"user","section":"identity","title":"Move me","body":"Preserve this node."}),
        serde_json::json!({"action":"create","scope":"space","section":"conventions","title":"Existing","body":"Keep this node too."}),
    ]).unwrap();
    let node = receipt
        .operations
        .iter()
        .find(|entry| entry.scope == MemoryScope::User)
        .unwrap();
    let user_before = store.read_file_text(MemoryScope::User, None).unwrap();
    let space_before = store.read_file_text(MemoryScope::Space, None).unwrap();
    // FILE_SHARE_READ | FILE_SHARE_WRITE deliberately omits FILE_SHARE_DELETE.
    // FILE_SHARE_READ | FILE_SHARE_WRITE 有意省略 FILE_SHARE_DELETE。
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x00000003)
        .open(store.scope_path(MemoryScope::Space, None).unwrap())
        .unwrap();
    let result = write(
        &store,
        &context,
        vec![
            serde_json::json!({"action":"move","id":node.id,"expectedRevision":node.revision,"scope":"space","section":"conventions"}),
        ],
    );
    assert!(matches!(result, Err(MemoryError::Io { .. })));
    assert_eq!(
        store.read_file_text(MemoryScope::User, None).unwrap(),
        user_before
    );
    assert_eq!(
        store.read_file_text(MemoryScope::Space, None).unwrap(),
        space_before
    );
    drop(lock);
}

/// Builds a store in a temporary user root.
/// 在临时用户根目录中构建存储。
fn store() -> (tempfile::TempDir, MarkdownMemoryStore) {
    let root = tempfile::tempdir().expect("temp dir");
    let store = MarkdownMemoryStore::new(root.path());
    (root, store)
}

/// A project context for a host project rooted in the temporary directory.
/// 根目录位于临时目录的宿主项目上下文。
fn project_context(root: &tempfile::TempDir) -> ProjectContext {
    ProjectContext {
        host_project_id: Some(String::from("host-1")),
        root_path: Some(root.path().join("repo").display().to_string()),
        memory_project_id: None,
    }
}

/// Parses one operation from JSON as the tool layer does.
/// 按工具层的方式从 JSON 解析单个操作。
fn operation(value: serde_json::Value) -> WriteOperation {
    serde_json::from_value(value).expect("valid operation")
}

/// Writes operations as the AI with default budget.
/// 以 AI 身份使用默认预算写入操作。
fn write(
    store: &MarkdownMemoryStore,
    context: &ProjectContext,
    operations: Vec<serde_json::Value>,
) -> Result<WriteReceipt, MemoryError> {
    store.write(
        context,
        Some("session-1"),
        NodeSource::Ai,
        &InjectionBudget::default(),
        operations.into_iter().map(operation).collect(),
    )
}

/// Operations parse from camelCase JSON and reject unknown fields.
/// 操作从 camelCase JSON 解析，并拒绝未知字段。
#[test]
fn operations_parse_and_reject_unknown_fields() {
    let parsed = operation(serde_json::json!({
        "action": "create",
        "scope": "user",
        "section": "identity",
        "title": "Name",
        "body": "Ada",
    }));
    assert!(matches!(
        parsed,
        WriteOperation::Create {
            scope: Some(MemoryScope::User),
            section: Some(_),
            new_section: false,
            ..
        }
    ));
    let moved = operation(serde_json::json!({
        "action": "move",
        "id": "m-1",
        "expectedRevision": "abc",
        "parentId": "m-2",
    }));
    assert!(matches!(
        moved,
        WriteOperation::Move {
            parent_id: Some(_),
            scope: None,
            ..
        }
    ));
    let unknown = serde_json::from_value::<WriteOperation>(serde_json::json!({
        "action": "replace",
        "id": "m-1",
        "expectedRevision": "abc",
        "body": "x",
        "extra": true,
    }));
    assert!(unknown.is_err());
}

/// The first write seeds preset sections, records metadata and reports budget usage.
/// 首次写入会写入预置分区、记录元数据并报告预算占用。
#[test]
fn create_seeds_presets_and_records_metadata() {
    let (root, store) = store();
    let context = project_context(&root);
    let receipt = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "user", "section": "identity",
            "title": "Preferred name", "body": "Call the user Ada.",
        })],
    )
    .expect("write");
    assert_eq!(receipt.operations.len(), 1);
    assert!(receipt.created_project.is_none());
    assert_eq!(receipt.usage.len(), 1);
    let id = receipt.operations[0].id.clone();

    let documents = store.load_scopes(&context).expect("load");
    let views = documents.user.views(MemoryScope::User);
    let sections = views
        .iter()
        .filter(|view| view.level == 1)
        .filter_map(|view| view.section.clone())
        .collect::<Vec<_>>();
    let presets = preset_sections(MemoryScope::User)
        .iter()
        .map(|(key, _)| (*key).to_string())
        .collect::<Vec<_>>();
    assert_eq!(sections, presets);
    let node = views
        .iter()
        .find(|view| view.id == id)
        .expect("created node");
    assert_eq!(node.level, 2);
    assert_eq!(node.body, "Call the user Ada.");
    let meta = node.meta.as_ref().expect("metadata");
    assert_eq!(meta.source, NodeSource::Ai);
    assert_eq!(meta.session.as_deref(), Some("session-1"));
    assert!(crate::timestamp::is_utc_minute(&meta.updated));
}

/// A stale revision fails the whole call and leaves every file untouched.
/// 过期修订会使整个调用失败，并保持所有文件不变。
#[test]
fn stale_revision_rolls_back_whole_call() {
    let (root, store) = store();
    let context = project_context(&root);
    let first = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "space", "section": "conventions",
            "title": "Commits", "body": "Use categorized commits.",
        })],
    )
    .expect("seed");
    let id = first.operations[0].id.clone();
    let path = store.scope_path(MemoryScope::Space, None).expect("path");
    let before = std::fs::read_to_string(&path).expect("read");

    let result = write(
        &store,
        &context,
        vec![
            serde_json::json!({
                "action": "create", "scope": "space", "section": "conventions",
                "title": "Branches", "body": "Branch before committing.",
            }),
            serde_json::json!({
                "action": "replace", "id": id, "expectedRevision": "00000000", "body": "x",
            }),
        ],
    );
    assert!(matches!(
        result,
        Err(MemoryError::Operation { index: 2, ref source }) if matches!(**source, MemoryError::RevisionMismatch { .. })
    ));
    assert_eq!(std::fs::read_to_string(&path).expect("read"), before);
}

/// Replace, append and rename keep the node id and change its revision.
/// 替换、追加与重命名保留节点标识并改变修订。
#[test]
fn edits_keep_identity_and_change_revision() {
    let (root, store) = store();
    let context = project_context(&root);
    let created = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "user", "section": "communication",
            "title": "Language", "body": "Reply in Traditional Chinese.",
        })],
    )
    .expect("create");
    let id = created.operations[0].id.clone();
    let mut revision = created.operations[0].revision.clone();
    for change in [
        serde_json::json!({"action": "append", "text": "Keep status lines short."}),
        serde_json::json!({"action": "rename", "title": "Language and tone"}),
        serde_json::json!({"action": "replace", "body": "Reply in English."}),
    ] {
        let mut value = change;
        value["id"] = serde_json::Value::String(id.clone());
        value["expectedRevision"] = serde_json::Value::String(revision.clone());
        let receipt = write(&store, &context, vec![value]).expect("edit");
        assert_eq!(receipt.operations[0].id, id);
        assert_ne!(receipt.operations[0].revision, revision);
        revision = receipt.operations[0].revision.clone();
    }
    let documents = store.load_scopes(&context).expect("load");
    let view = documents
        .user
        .views(MemoryScope::User)
        .into_iter()
        .find(|view| view.id == id)
        .expect("node");
    assert_eq!(
        view.path.last().map(String::as_str),
        Some("Language and tone")
    );
    assert_eq!(view.body, "Reply in English.");
}

/// Bodies with node headings or metadata comments and titles with heading syntax are refused.
/// 含节点标题或元数据注释的正文，以及含标题语法的标题都会被拒绝。
#[test]
fn invalid_titles_and_bodies_are_refused() {
    let (root, store) = store();
    let context = project_context(&root);
    for value in [
        serde_json::json!({"action": "create", "scope": "user", "section": "identity", "title": "A", "body": "## Inner"}),
        serde_json::json!({"action": "create", "scope": "user", "section": "identity", "title": "A", "body": "<!-- mem id=x -->"}),
        serde_json::json!({"action": "create", "scope": "user", "section": "identity", "title": "# A"}),
        serde_json::json!({"action": "create", "scope": "user", "section": "identity", "title": "two\nlines"}),
        serde_json::json!({"action": "create", "scope": "user", "section": "missing", "title": "A"}),
        serde_json::json!({"action": "create", "scope": "user", "section": "identity", "newSection": true, "title": "A"}),
        serde_json::json!({"action": "create", "section": "identity", "title": "A"}),
    ] {
        assert!(
            write(&store, &context, vec![value.clone()]).is_err(),
            "{value}"
        );
    }
    let accepted = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "user", "section": "identity",
            "title": "Deep body", "body": "##### Allowed body heading",
        })],
    );
    assert!(accepted.is_ok());
}

/// The base section cannot be moved or deleted, and children need explicit confirmation.
/// 基础分区不能移动或删除，子节点需要显式确认。
#[test]
fn delete_guards_base_and_children() {
    let (root, store) = store();
    let context = project_context(&root);
    let parent = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "space", "section": "references",
            "title": "Docs", "body": "Index of docs.",
        })],
    )
    .expect("parent");
    let parent_id = parent.operations[0].id.clone();
    write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "space", "parentId": parent_id,
            "title": "Contracts", "body": "docs/contracts holds contracts.",
        })],
    )
    .expect("child");
    let documents = store.load_scopes(&context).expect("load");
    let views = documents.space.views(MemoryScope::Space);
    let base = views
        .iter()
        .find(|view| view.section.as_deref() == Some(BASE_SECTION_KEY))
        .expect("base");
    let parent_view = views
        .iter()
        .find(|view| view.id == parent_id)
        .expect("parent");

    let base_delete = serde_json::json!({
        "action": "delete", "id": base.id, "expectedRevision": base.revision, "withChildren": true,
    });
    assert!(write(&store, &context, vec![base_delete]).is_err());
    let unconfirmed = serde_json::json!({
        "action": "delete", "id": parent_id, "expectedRevision": parent_view.revision,
    });
    assert!(write(&store, &context, vec![unconfirmed]).is_err());
    let confirmed = serde_json::json!({
        "action": "delete", "id": parent_id, "expectedRevision": parent_view.revision, "withChildren": true,
    });
    let receipt = write(&store, &context, vec![confirmed]).expect("delete");
    assert_eq!(receipt.operations[0].deleted_children, 1);
    let remaining = store
        .load_scopes(&context)
        .expect("load")
        .space
        .views(MemoryScope::Space);
    assert!(remaining.iter().all(|view| view.id != parent_id));
}

/// A move across scopes relevels the subtree and keeps node ids.
/// 跨层移动会重设子树级别并保留节点标识。
#[test]
fn move_relevels_subtree_across_scopes() {
    let (root, store) = store();
    let context = project_context(&root);
    let parent = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "space", "section": "environment",
            "title": "Toolchain", "body": "Rust and pnpm.",
        })],
    )
    .expect("parent");
    let parent_id = parent.operations[0].id.clone();
    let child = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "space", "parentId": parent_id,
            "title": "Cargo jobs", "body": "build.jobs = 16",
        })],
    )
    .expect("child");
    let child_id = child.operations[0].id.clone();
    let revision = store
        .load_scopes(&context)
        .expect("load")
        .space
        .views(MemoryScope::Space)
        .into_iter()
        .find(|view| view.id == parent_id)
        .expect("parent")
        .revision;
    let receipt = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "move", "id": parent_id, "expectedRevision": revision,
            "scope": "project", "newSection": true,
        })],
    )
    .expect("move");
    assert!(receipt.created_project.is_some());
    assert!(receipt.operations[0].new_section);
    let documents = store.load_scopes(&context).expect("load");
    assert!(
        documents
            .space
            .views(MemoryScope::Space)
            .iter()
            .all(|view| view.id != parent_id && view.id != child_id)
    );
    let project = documents
        .project
        .expect("project bound")
        .1
        .views(MemoryScope::Project);
    let moved = project
        .iter()
        .find(|view| view.id == parent_id)
        .expect("moved");
    let moved_child = project
        .iter()
        .find(|view| view.id == child_id)
        .expect("child");
    assert_eq!(moved.level, 1);
    assert!(moved.section.is_some());
    assert_eq!(moved_child.level, 2);
}

/// A project write without a bound memory project creates and binds one; no project context fails.
/// 未绑定记忆项目时的项目层写入会新建并绑定；无项目上下文时失败。
#[test]
fn project_write_binds_new_memory_project() {
    let (root, store) = store();
    let context = project_context(&root);
    let receipt = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "project", "section": "pitfalls",
            "title": "Linking", "body": "Linking dominates edit cycles.",
        })],
    )
    .expect("write");
    let created = receipt.created_project.expect("created project");
    let registry = store.registry().expect("registry");
    assert_eq!(
        registry
            .project_for_host("host-1")
            .map(|project| project.id.as_str()),
        Some(created.as_str())
    );
    let again = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "project", "section": "pitfalls",
            "title": "Hooks", "body": "Profile hook runs per user turn.",
        })],
    )
    .expect("second write");
    assert!(again.created_project.is_none());

    let no_project = ProjectContext::default();
    let result = write(
        &store,
        &no_project,
        vec![serde_json::json!({
            "action": "create", "scope": "project", "section": "pitfalls", "title": "X",
        })],
    );
    assert!(matches!(
        result,
        Err(MemoryError::Operation { index: 1, ref source }) if matches!(**source, MemoryError::ProjectUnavailable(_))
    ));
}

/// A similar title in the same scope is reported as a possible duplicate.
/// 同层相似标题会被报告为可能重复。
#[test]
fn similar_titles_are_reported_as_duplicates() {
    let (root, store) = store();
    let context = project_context(&root);
    write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "space", "section": "conventions",
            "title": "Commit message style", "body": "Categorized.",
        })],
    )
    .expect("first");
    let receipt = write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "space", "section": "conventions",
            "title": "Commit message styles", "body": "Short subject.",
        })],
    )
    .expect("second");
    assert_eq!(receipt.operations[0].duplicates.len(), 1);
}

/// Untouched bytes of a hand-edited file survive a write elsewhere in the file.
/// 手工编辑文件中未触及的字节在文件其他位置写入后保持不变。
#[test]
fn untouched_bytes_survive_writes() {
    let (root, store) = store();
    let context = project_context(&root);
    let path = store.scope_path(MemoryScope::User, None).expect("path");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    let hand = "Intro paragraph.\r\n\r\n# Identity\r\n\r\nWritten   by *hand*.\r\n\r\n## Name   \r\n\r\nAda\r\n";
    std::fs::write(&path, hand).expect("seed");
    let documents = store.load_scopes(&context).expect("load");
    let identity = documents
        .user
        .views(MemoryScope::User)
        .into_iter()
        .find(|view| view.level == 1)
        .expect("identity");
    write(
        &store,
        &context,
        vec![serde_json::json!({
            "action": "create", "scope": "user", "parentId": identity.id,
            "title": "Timezone", "body": "UTC+8",
        })],
    )
    .expect("write");
    let after = std::fs::read_to_string(&path).expect("read");
    assert!(after.starts_with("Intro paragraph.\r\n\r\n# Identity\r\n\r\nWritten   by *hand*.\r\n\r\n## Name   \r\n\r\nAda\r\n"));
    assert!(after.contains("## Timezone\r\n"));
    assert!(!after.replace("\r\n", "").contains('\n'));
}
