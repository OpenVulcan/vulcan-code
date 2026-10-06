//! Real atomic selections bind verified immutable packages and complete approvals in one record.
//! 真实原子选择在同一记录中绑定已验证不可变包与完整批准权限。

use super::*;

/// Prepare one content package with the declared `permissions`; return its real lease and canonical approval.
/// 准备声明 `permissions` 的内容包；返回实际租约及规范批准权限。
/// The source belongs to `root`, and `store` owns the separate publication namespace.
/// 来源属于 `root`，`store` 持有独立发布命名空间。
fn permission_package(
    store: &VulcanPluginGenerationStore,
    root: &Path,
    permissions: &[&str],
) -> (PinnedVulcanPluginGeneration, Vec<String>) {
    // Extend a real valid manifest, then let production inspection derive all capability keys.
    // 扩展真实有效清单，再由生产检查派生全部能力键。
    let source = write_package(root, "sample-plugin", "approval fixture");
    // Only fixture-owned bytes are rewritten; approval never comes from a synthesized descriptor.
    // 仅重写夹具字节；批准权限绝不来自合成描述符。
    let manifest = source.join("plugin.yaml");
    // Preserve the original source and append the explicitly requested declaration.
    // 保留原来源并追加明确请求的声明。
    let mut yaml = fs::read_to_string(&manifest).expect("read approval manifest");
    yaml.push_str(&format!("permissions: [{}]\n", permissions.join(", ")));
    fs::write(manifest, yaml).expect("write approval declarations");
    // The same canonical helper feeds authorization previews and verified generation handles.
    // 同一规范辅助函数供授权预览及已验证代次句柄使用。
    let descriptor = crate::inspect_user_plugin_package(&source).expect("inspect approval source");
    // Freeze expected approval before preparing any physical candidate.
    // 准备任何物理候选前固定预期批准权限。
    let approved =
        crate::runtime::plugin_required_permissions(&descriptor).expect("derive approval");
    (prepare(store, &source), approved)
}

/// Exact approval is required for every selection; rejected approval leaves no record behind.
/// 每次选择都要求精确批准；被拒绝的批准不留下任何记录。
#[test]
fn managed_approval_publication_is_exact() {
    // The record uses the real store and filesystem in an isolated root.
    // 记录在隔离根中使用真实存储及文件系统。
    let temp = tempfile::tempdir().expect("create approval publication fixture");
    // Namespace locks and CAS are production implementations.
    // 命名空间锁与比较替换均为生产实现。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open approval store");
    // Two permissions make missing, duplicate and caller-order behavior observable.
    // 两项权限使缺失、重复及调用方顺序行为可观测。
    let (generation, approved) = permission_package(
        &store,
        &temp.path().join("source"),
        &["network_https", "config"],
    );
    // Rejections must not create a partial selection record.
    // 拒绝不得创建部分选择记录。
    let path = store.root.join("active/sample-plugin.json");
    for rejected in [
        Vec::new(),
        vec!["config".into()],
        vec!["config".into(), "config".into(), "network_https".into()],
        vec![
            "config".into(),
            "network_https".into(),
            "process_binary".into(),
        ],
    ] {
        assert_eq!(
            store
                .activate_for_test(&generation, 0, rejected)
                .expect_err("inexact approval must fail")
                .code,
            VulcanPluginStoreErrorCode::Invalid
        );
        assert!(!path.exists());
    }
    // Caller order may differ; the sole canonical declaration order becomes the stored approval.
    // 调用方顺序可以不同；唯一规范声明顺序成为存储批准顺序。
    let mut reversed = approved.clone();
    reversed.reverse();
    // Package and approval become visible in one record.
    // 插件包与批准在单个记录中共同可见。
    let published = store
        .activate_for_test(&generation, 0, reversed)
        .expect("publish approval");
    assert_eq!(published.schema_version, ACTIVATION_RECORD_VERSION);
    assert_eq!(published.approved_permissions(), approved.as_slice());
    assert_eq!(published.generation(), Some(generation.identity()));
    assert_eq!(published.revision, 1);
    // Reopening must recover exactly the selected approval without another file or write.
    // 重新打开必须恢复精确选定批准，不依赖其他文件或写入。
    let reopened = VulcanPluginGenerationStore::open_existing(temp.path())
        .expect("reopen")
        .expect("store");
    assert_eq!(
        reopened
            .pin_active("sample-plugin")
            .expect("read approved package")
            .activation,
        Some(published)
    );
    // Tombstones retain revision history and carry no approval.
    // 停用记录保留修订历史且不携带批准。
    let removed = store
        .deactivate("sample-plugin", 1)
        .expect("remove approved selection");
    assert!(removed.generation().is_none());
    assert!(removed.approved_permissions().is_empty());
    assert_eq!(
        store
            .activate_for_test(&generation, 1, approved.clone())
            .expect_err("stale approval revision")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    store
        .activate_for_test(&generation, 2, approved)
        .expect("explicit approved reactivation");
}

/// Strict original-byte decoding rejects malformed approvals, and package pinning rejects mismatched declarations.
/// 严格原字节解码拒绝损坏批准，插件固定拒绝不匹配声明。
#[test]
fn managed_approval_corrupt_records_never_fall_back_to_manifest_authority() {
    // Real selected contents distinguish protocol corruption from absent package data.
    // 真实选中内容区分协议损坏与包数据缺失。
    let temp = tempfile::tempdir().expect("create approval corruption fixture");
    // Every query goes through the production bounded decoder and namespace lock.
    // 每次查询均经过生产有界解码器及命名空间锁。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open corruption store");
    // The complete permission set has a meaningful nonempty requirement.
    // 完整权限集合具有实际非空要求。
    let (generation, approved) = permission_package(
        &store,
        &temp.path().join("source"),
        &["config", "network_https"],
    );
    store
        .activate_for_test(&generation, 0, approved)
        .expect("select approved package");
    // Preserve the one actual published document for independent corruption cases.
    // 为独立损坏场景保留唯一实际发布文档。
    let path = store.root.join("active/sample-plugin.json");
    // Original bytes also allow duplicate-field injection without a map collapsing duplicates.
    // 原始字节还允许注入重复字段，避免映射折叠重复项。
    let original = fs::read_to_string(&path).expect("read original approval");
    // Structured copies are used only to construct isolated malformed fixtures.
    // 结构化副本仅用于构造隔离损坏夹具。
    let document: serde_json::Value = serde_json::from_str(&original).expect("parse fixture");
    for invalid in [
        serde_json::Value::Null,
        serde_json::json!(["config", "config"]),
        serde_json::json!(["network_https", "config"]),
        serde_json::json!([""]),
        serde_json::json!(["bad\u{0}key"]),
    ] {
        // One malformed field is enough to block selection without modifying the rejected file.
        // 单个损坏字段足以阻止选择，且不修改被拒绝文件。
        let mut changed = document.clone();
        changed["approved_permissions"] = invalid;
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("encode corrupt fixture"),
        )
        .expect("write corruption");
        assert!(store.activation("sample-plugin").is_err());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path).expect("read corruption"))
                .expect("decode fixture"),
            changed
        );
    }
    // Every version other than the sole current format fails closed.
    // 唯一当前格式以外的每个版本都拒绝。
    for version in [1, 2, 3, 4, 99] {
        let mut changed = document.clone();
        changed["schema_version"] = version.into();
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("encode version fixture"),
        )
        .expect("write version");
        assert!(store.activation("sample-plugin").is_err());
    }
    // Every selection authority is a required field.
    // 每个选择权威都是必需字段。
    for (parent, field) in [
        ("", "approved_permissions"),
        ("", "package"),
        ("", "settings"),
        ("/settings", "revision"),
        ("/settings", "credential_revision"),
        ("/settings", "configuration"),
    ] {
        let mut changed = document.clone();
        changed
            .pointer_mut(parent)
            .and_then(serde_json::Value::as_object_mut)
            .expect("record object")
            .remove(field)
            .expect("field present in the current record format");
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("encode missing-field fixture"),
        )
        .expect("write missing field");
        assert!(store.activation("sample-plugin").is_err());
    }
    for injected in [
        "{\"approved_permissions\":[],",
        "{\"schema_version\":2,",
        "{\"unknown_field\":true,",
    ] {
        fs::write(&path, original.replacen('{', injected, 1)).expect("inject invalid field");
        assert!(store.activation("sample-plugin").is_err());
    }
    // A canonical but incomplete set must still fail when the real package is pinned.
    // 规范但不完整集合在固定实际插件包时仍须失败。
    let mut incomplete = document.clone();
    incomplete["approved_permissions"] = serde_json::json!([]);
    fs::write(
        &path,
        serde_json::to_vec(&incomplete).expect("encode incomplete approval"),
    )
    .expect("write incomplete approval");
    assert!(
        store
            .pin_active("sample-plugin")
            .expect_err("reject missing required approval")
            .message
            .contains("capability set")
    );
    // A tombstone must never carry permission grants for a nonexistent selected package.
    // 停用记录绝不能携带对不存在选中包的权限授予。
    let mut invalid_tombstone = document;
    invalid_tombstone["package"] = serde_json::json!({"kind": "uninstalled"});
    fs::write(
        &path,
        serde_json::to_vec(&invalid_tombstone).expect("encode tombstone"),
    )
    .expect("write tombstone");
    assert!(store.activation("sample-plugin").is_err());
}

/// A failed record replacement leaves prepared bytes and their lease intact, permitting an explicit original retry.
/// 记录替换失败保留准备字节及租约，允许明确重试原操作。
#[test]
fn managed_approval_failed_publication_keeps_candidate_unselected() {
    // This failure uses a real filesystem obstruction rather than mocking the writer.
    // 此失败使用真实文件系统障碍，不模拟写入器。
    let temp = tempfile::tempdir().expect("create approval write fixture");
    // Store and candidate share a controlled root while the published file is obstructed.
    // 存储与候选共享受控根，发布文件被阻塞。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open blocked store");
    // Empty approval is valid for this zero-capability package, not for a missing field.
    // 空批准对此零能力包有效，不代表字段缺失。
    let generation = prepare(
        &store,
        &write_package(
            &temp.path().join("source"),
            "sample-plugin",
            "zero permissions",
        ),
    );
    // Credentials are staged first so only the selection write meets the obstruction.
    // 先暂存凭据，使只有选择写入遇到阻碍。
    let credentials = store.stage_unconfigured_credentials_for_test(&generation);
    // An exact directory at the final metadata file path prevents atomic replacement.
    // 最终元数据文件路径上的精确目录阻止原子替换。
    let path = store.root.join("active/sample-plugin.json");
    fs::create_dir(&path).expect("obstruct selection file");
    assert!(
        store
            .activate_with_credentials_for_test(&generation, 0, Vec::new(), credentials.clone())
            .is_err()
    );
    assert!(generation.package_root().join("plugin.yaml").is_file());
    assert!(path.is_dir());
    fs::remove_dir(&path).expect("remove fixture obstruction");
    assert!(
        store
            .activation("sample-plugin")
            .expect("unselected record")
            .is_none()
    );
    store
        .activate_with_credentials_for_test(&generation, 0, Vec::new(), credentials)
        .expect("explicit retry");
    assert!(
        store
            .pin_active("sample-plugin")
            .expect("read zero approval")
            .generation
            .is_some()
    );
}

/// A real Windows file lease rejects replacement after staging without changing either selected identity or grants.
/// 真实 Windows 文件租约在暂存后拒绝替换，不改变选中身份或授予。
#[cfg(windows)]
#[test]
fn managed_approval_locked_record_retains_the_complete_previous_selection() {
    use std::os::windows::fs::OpenOptionsExt;

    // Every file involved belongs to this temporary fixture; no user credential or package is touched.
    // 涉及的全部文件属于此临时夹具；不触碰用户凭据或插件包。
    let temp = tempfile::tempdir().expect("create locked approval fixture");
    // The writer below is the production synchronized sibling-file replacement.
    // 下方写入器为生产同步同级文件替换实现。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open locked approval store");
    // Different required permissions make package/approval mixing detectable.
    // 不同必需权限使包与批准混合可被检测。
    let (first, first_permissions) =
        permission_package(&store, &temp.path().join("first"), &["config"]);
    // Candidate remains independently prepared and pinned after a failed selection write.
    // 选择写入失败后，候选仍保持独立准备及固定。
    let (second, second_permissions) =
        permission_package(&store, &temp.path().join("second"), &["network_https"]);
    // This exact predecessor is the only selection the failed write may leave visible.
    // 此精确前驱是失败写入可以保持可见的唯一选择。
    let original = store
        .activate_for_test(&first, 0, first_permissions)
        .expect("publish original approval");
    // The final record is opened without deletion sharing to obstruct the actual rename boundary.
    // 打开最终记录时不共享删除权限，以阻塞实际重命名边界。
    let path = store.root.join("active/sample-plugin.json");
    // Preserve actual bytes instead of normalizing metadata for the failure assertion.
    // 保留实际字节，不为失败断言规范化元数据。
    let bytes = fs::read(&path).expect("read predecessor bytes");
    // Read and write sharing remain allowed; only rename/delete is held by Windows.
    // 仍允许共享读取及写入；Windows 仅阻止重命名与删除。
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(&path)
        .expect("retain real file lease");
    assert_eq!(
        store
            .activate_for_test(&second, 1, second_permissions.clone())
            .expect_err("reject locked replacement")
            .code,
        VulcanPluginStoreErrorCode::Storage
    );
    assert_eq!(fs::read(&path).expect("read retained bytes"), bytes);
    assert_eq!(
        store
            .pin_active("sample-plugin")
            .expect("read retained selection")
            .activation,
        Some(original)
    );
    assert!(second.package_root().join("plugin.yaml").is_file());
    drop(blocker);
    // Explicit retry uses the same original predecessor and candidate, with no implicit side-effect replay.
    // 明确重试使用相同原前驱及候选，不隐式重放副作用。
    let committed = store
        .activate_for_test(&second, 1, second_permissions)
        .expect("retry original replacement");
    assert_eq!(committed.generation(), Some(second.identity()));
    assert_eq!(
        store
            .pin_active("sample-plugin")
            .expect("read confirmed replacement")
            .activation,
        Some(committed)
    );
}

/// Competing approvals can publish only one complete package/approval pair at the same predecessor.
/// 竞争批准在相同前驱只能发布一个完整插件包与批准组合。
#[test]
fn managed_approval_competing_writers_cannot_mix_generation_and_grants() {
    // Distinct source declarations make a mixed record detectable after the race.
    // 不同来源声明使竞争后能够检测混合记录。
    let temp = tempfile::tempdir().expect("create approval race fixture");
    // Cloneable stores still coordinate through real per-namespace operating-system locks.
    // 可克隆存储仍通过实际命名空间操作系统锁协调。
    let store = VulcanPluginGenerationStore::open(temp.path()).expect("open race store");
    // Both packages are already prepared before either writer can publish.
    // 任一写入者发布前，两个插件包均已准备。
    // Credential inventories are staged up front so the race covers only selection publication.
    // 凭据清单预先暂存，使竞争只覆盖选择发布。
    let candidates = [
        permission_package(&store, &temp.path().join("first"), &["config"]),
        permission_package(&store, &temp.path().join("second"), &["network_https"]),
    ]
    .map(|(package, permissions)| {
        let credentials = store.stage_unconfigured_credentials_for_test(&package);
        (package, permissions, credentials)
    });
    // Each writer gets one simultaneous attempt, with no hidden retry.
    // 每个写入者得到一次同时尝试，不隐藏重试。
    let barrier = Arc::new(Barrier::new(candidates.len()));
    // Join every real writer before observing the one published selection.
    // 观测唯一发布选择前等待每个实际写入者结束。
    let results = std::thread::scope(|scope| {
        // Scoped handles retain each package lease until both submissions end.
        // 作用域句柄保留每个插件包租约，直至两次提交结束。
        let workers = candidates
            .iter()
            .map(|(package, permissions, credentials)| {
                // Shared references remain valid for the complete scoped writer lifetime.
                // 共享引用在完整作用域写入者寿命内有效。
                let store = &store;
                // This barrier only synchronizes the fixture, never the production lock.
                // 此屏障仅同步夹具，不代替生产锁。
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    store.activate_with_credentials_for_test(
                        package,
                        0,
                        permissions.clone(),
                        credentials.clone(),
                    )
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("join approval writer"))
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    for error in results.iter().filter_map(|result| result.as_ref().err()) {
        assert!(matches!(
            error.code,
            VulcanPluginStoreErrorCode::Busy | VulcanPluginStoreErrorCode::Conflict
        ));
    }
    // Pinning independently checks the actual chosen manifest against its stored grants.
    // 独立固定将实际选中清单与其存储授予比较。
    let selected = store
        .pin_active("sample-plugin")
        .expect("read winning approval");
    assert_eq!(
        selected.activation.as_ref(),
        results.iter().find_map(|result| result.as_ref().ok())
    );
    assert!(selected.generation.is_some());
}
