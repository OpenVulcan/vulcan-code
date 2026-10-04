//! Credential revisions share one durable selection with packages and approvals, without containing secret values.
//! 凭据修订与插件包及批准共用单个持久选择，不包含秘密值。

use super::*;

/// Empty secure backend proves that a package without declared secrets never touches external storage.
/// 空安全后端证明没有声明秘密的插件包绝不访问外部存储。
pub(super) struct EmptyCredentialBackend;

impl VulcanPluginCredentialBackend for EmptyCredentialBackend {
    /// Reject every read because this fixture declares no secret slots.
    /// 拒绝每次读取，因为此夹具未声明秘密槽位。
    fn read(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Err("Unexpected empty-fixture read.".into())
    }
    /// Reject every write because no value belongs to this fixture.
    /// 拒绝每次写入，因为此夹具不拥有任何值。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        Err("Unexpected empty-fixture write.".into())
    }
    /// Reject deletion because this fixture owns no external entry.
    /// 拒绝删除，因为此夹具不拥有外部条目。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Err("Unexpected empty-fixture delete.".into())
    }
}

/// Persist a genuinely empty staging checkpoint for `package`/`revision`; no secure backend call is permitted.
/// 为 `package`／`revision` 持久保存确实为空的暂存检查点；不允许安全后端调用。
pub(super) fn stage_empty_credentials(
    store: &VulcanPluginGenerationStore,
    package: &PinnedVulcanPluginGeneration,
    revision: &VulcanPluginCredentialRevision,
) {
    store
        .begin_credential_staging(
            package,
            revision.clone(),
            &VulcanPluginCredentialValues::new(),
        )
        .expect("prepare empty credential inventory")
        .write_and_finish(
            &EmptyCredentialBackend,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("verify empty credential staging");
}

/// Credential references reject aliases and missing authority before any filesystem or keyring access.
/// 凭据引用在任何文件系统或凭据库访问前拒绝别名及缺失权威。
#[test]
fn credential_binding_revision_requires_canonical_non_nil_identity() {
    // A fixed nonsecret identity exercises canonical spelling independently of randomness.
    // 固定非秘密身份独立于随机性验证规范拼写。
    let canonical = "f3026e5e-202d-43fe-8dce-65bca0c26ba9";
    for rejected in [
        "",
        "00000000-0000-0000-0000-000000000000",
        "../outside",
        "F3026E5E-202D-43FE-8DCE-65BCA0C26BA9",
        "f3026e5e202d43fe8dce65bca0c26ba9",
    ] {
        assert!(VulcanPluginCredentialRevision::parse(rejected).is_err());
        assert!(
            serde_json::from_value::<VulcanPluginCredentialRevision>(serde_json::json!(rejected))
                .is_err()
        );
    }
    // Persistence round trips through the exact validated identity rather than a normalized alias.
    // 持久往返经过精确已校验身份，不经过规范化别名。
    let revision = VulcanPluginCredentialRevision::parse(canonical).expect("canonical revision");
    assert_eq!(revision.as_str(), canonical);
    assert_eq!(
        serde_json::from_slice::<VulcanPluginCredentialRevision>(
            &serde_json::to_vec(&revision).expect("encode revision")
        )
        .expect("decode revision"),
        revision
    );
    assert!(serde_json::from_str::<VulcanPluginCredentialRevision>("null").is_err());
    assert_ne!(
        VulcanPluginCredentialRevision::allocate(),
        VulcanPluginCredentialRevision::allocate()
    );
}

/// Credential-only rotation retains the physical package, while tombstones preserve identity and reject downgrade.
/// 仅轮换凭据保留物理插件包，停用记录保留身份并拒绝降级。
#[test]
fn credential_binding_selection_rotates_independently_and_preserves_tombstone_authority() {
    // A content-only package proves that credential selection does not imply Lua execution or package copying.
    // 纯内容包证明凭据选择不意味着 Lua 执行或插件包复制。
    let temp = tempfile::tempdir().expect("create credential selection fixture");
    // Real selection locks and original byte writes remain the authority throughout the test.
    // 实际选择锁及原字节写入在测试全程保持权威。
    let store =
        VulcanPluginGenerationStore::open(temp.path()).expect("open credential selection store");
    // This same physical handle remains selected across both credential revisions.
    // 此同一物理句柄在两次凭据修订中保持选中。
    let package = prepare(
        &store,
        &write_package(
            &temp.path().join("source"),
            "sample-plugin",
            "credential selection",
        ),
    );
    store
        .activate_authorized(&package, 0, Vec::new())
        .expect("select version two");
    // The first declared revision is separate from both the activation counter and the package UUID.
    // 第一次声明修订独立于激活计数及插件包 UUID。
    let first = VulcanPluginCredentialRevision::allocate();
    // The successor credential revision is explicitly allocated, never inferred from a global lookup.
    // 后继凭据修订明确分配，绝不从全局查找推断。
    let second = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &package, &first);
    stage_empty_credentials(&store, &package, &second);
    // Approval, package and credential reference are captured in one version-three record.
    // 批准、插件包及凭据引用在单个第三版记录中捕获。
    let selected = store
        .activate_with_credentials(&package, 1, Vec::new(), first.clone())
        .expect("select credential revision");
    assert_eq!(selected.schema_version, CREDENTIAL_ACTIVATION_VERSION);
    assert_eq!(selected.credential_revision(), Some(&first));
    assert!(
        store
            .activate_authorized(&package, 2, Vec::new())
            .expect_err("reject v2 downgrade")
            .message
            .contains("downgrade")
    );
    assert!(store.activate(&package, 2).is_err());
    assert_eq!(
        store
            .activate_with_credentials(&package, 1, Vec::new(), second.clone())
            .expect_err("reject stale credential publication")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    // Only the credential reference and activation revision change here.
    // 此处仅改变凭据引用及激活修订。
    let rotated = store
        .activate_with_credentials(&package, 2, Vec::new(), second.clone())
        .expect("rotate credentials");
    assert_eq!(rotated.generation.as_ref(), Some(package.identity()));
    assert_eq!(selected.credential_revision(), Some(&first));
    assert_eq!(rotated.credential_revision(), Some(&second));
    assert_eq!(
        VulcanPluginGenerationStore::open_existing(temp.path())
            .expect("reopen")
            .expect("store")
            .pin_active("sample-plugin")
            .expect("read persisted selection")
            .activation,
        Some(rotated)
    );
    // The inactive record retains its last credential identity for later explicit cleanup.
    // 非活动记录保留最后凭据身份，供后续明确清理。
    let removed = store
        .deactivate("sample-plugin", 3)
        .expect("remove selection");
    assert!(removed.generation.is_none());
    assert_eq!(removed.credential_revision(), Some(&second));
    assert!(store.activate_authorized(&package, 4, Vec::new()).is_err());
    assert!(
        store
            .pin_active("sample-plugin")
            .expect("read tombstone")
            .generation
            .is_none()
    );
    store
        .activate_with_credentials(&package, 4, Vec::new(), first)
        .expect("explicit retained credential reactivation");
}

/// Every persisted format enforces its own exact credential-field shape and rejects ambiguous identities.
/// 每种持久格式强制自身精确凭据字段形状，并拒绝歧义身份。
#[test]
fn credential_binding_record_rejects_missing_null_and_cross_version_fields() {
    // Corruption is injected into actual metadata bytes consumed by the production reader.
    // 损坏被注入生产读取器消费的实际元数据字节。
    let temp = tempfile::tempdir().expect("create credential corruption fixture");
    // The physical package remains valid so only authority corruption causes rejection.
    // 物理插件包保持有效，确保拒绝仅由权威损坏引起。
    let store =
        VulcanPluginGenerationStore::open(temp.path()).expect("open credential corruption store");
    // Keep a real lease throughout all independent malformed cases.
    // 在所有独立损坏场景期间保留实际租约。
    let package = prepare(
        &store,
        &write_package(
            &temp.path().join("source"),
            "sample-plugin",
            "credential corruption",
        ),
    );
    // Even a zero-secret package requires an explicit durable staging checkpoint before selection.
    // 即使零秘密插件包，也要求选择前具有明确持久暂存检查点。
    let revision = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &package, &revision);
    store
        .activate_with_credentials(&package, 0, Vec::new(), revision)
        .expect("publish credential selection");
    // This single file is the package, approval and credential selection authority.
    // 此单一文件是插件包、批准及凭据选择权威。
    let path = store.root.join("active/sample-plugin.json");
    // Original bytes permit duplicate-field testing without map normalization.
    // 原始字节允许不经映射规范化的重复字段测试。
    let original = fs::read_to_string(&path).expect("read credential selection");
    // Structured copies construct individual invalid formats without mutating the original expectation.
    // 结构化副本构造独立无效格式，不修改原始预期。
    let document: serde_json::Value = serde_json::from_str(&original).expect("decode test record");
    for invalid in [
        serde_json::Value::Null,
        serde_json::json!(""),
        serde_json::json!("00000000-0000-0000-0000-000000000000"),
        serde_json::json!(1),
    ] {
        // Invalid credential authority must not be reinterpreted as legacy storage.
        // 无效凭据权威不得重新解释为旧存储。
        let mut changed = document.clone();
        changed["credential_revision"] = invalid;
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("encode credential corruption"),
        )
        .expect("write credential corruption");
        assert!(store.pin_active("sample-plugin").is_err());
    }
    for field in ["credential_revision", "approved_permissions"] {
        // Both companion authorities are mandatory in the version-three format.
        // 第三版格式中，两种伴随权威均必需。
        let mut changed = document.clone();
        changed.as_object_mut().expect("record").remove(field);
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("encode missing field"),
        )
        .expect("write missing field");
        assert!(store.activation("sample-plugin").is_err());
    }
    for version in [RECORD_VERSION, AUTHORIZED_ACTIVATION_VERSION, 99] {
        // Older versions cannot smuggle a newer credential binding into an otherwise valid record.
        // 较早版本不能向其他部分有效的记录夹带新凭据绑定。
        let mut changed = document.clone();
        changed["schema_version"] = version.into();
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("encode wrong version"),
        )
        .expect("write wrong version");
        assert!(store.activation("sample-plugin").is_err());
    }
    fs::write(
        &path,
        original.replacen(
            '{',
            "{\"credential_revision\":\"f3026e5e-202d-43fe-8dce-65bca0c26ba9\",",
            1,
        ),
    )
    .expect("inject duplicate revision");
    assert!(store.activation("sample-plugin").is_err());
    fs::write(&path, &original).expect("restore fixture bytes");
    assert!(
        store
            .pin_active("sample-plugin")
            .expect("read original authority")
            .generation
            .is_some()
    );
}
