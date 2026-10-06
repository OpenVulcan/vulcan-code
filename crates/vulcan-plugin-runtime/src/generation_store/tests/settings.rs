//! Settings edits change only the settings section; superseded credentials are collected by reachability.
//! 设置编辑只改变设置段；被取代凭据按可达性回收。

use super::configuration::fixture;
use super::credentials::{EmptyCredentialBackend, stage_empty_credentials};
use super::*;
use serde_json::json;
use std::collections::BTreeSet;

/// Build a change that replaces only the configuration document.
/// 构造只替换配置文档的变更。
fn configure(value: serde_json::Value) -> VulcanPluginSettingsChange {
    VulcanPluginSettingsChange {
        configuration: Some(VulcanPluginConfigurationSelection::Document { value }),
        credential_revision: None,
    }
}

/// Build a change that replaces only the credential revision.
/// 构造只替换凭据修订的变更。
fn rotate(revision: &VulcanPluginCredentialRevision) -> VulcanPluginSettingsChange {
    VulcanPluginSettingsChange {
        configuration: None,
        credential_revision: Some(revision.clone()),
    }
}

/// Read the phase of one credential revision.
/// 读取单个凭据修订的阶段。
fn phase(
    store: &VulcanPluginGenerationStore,
    revision: &VulcanPluginCredentialRevision,
) -> VulcanPluginCredentialPhase {
    store
        .credential_record("configured-plugin", revision)
        .expect("read credential record")
        .expect("credential record present")
        .phase()
}

/// An edit keeps the package selection, advances only the settings revision and validates against the selected package.
/// 编辑保留插件包选择，只递增设置修订，并按所选插件包校验。
#[test]
fn settings_reconfigure_keeps_package_selection_and_checks_both_revisions() {
    let temp = tempfile::tempdir().expect("create settings fixture");
    let (store, package, credentials) = fixture(temp.path(), true);
    let selected = store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            credentials,
            package.default_configuration_for_test(),
        )
        .expect("select configured package");

    let edited = store
        .reconfigure(
            "configured-plugin",
            selected.revision,
            selected.settings().revision,
            configure(json!({"label": "edited"})),
        )
        .expect("edit configuration");
    assert_eq!(edited.revision, selected.revision);
    assert_eq!(edited.package(), selected.package());
    assert_eq!(
        edited.approved_permissions(),
        selected.approved_permissions()
    );
    assert_eq!(edited.credential_revision(), selected.credential_revision());
    assert_eq!(edited.settings().revision, selected.settings().revision + 1);
    assert_eq!(
        edited.configuration().value(),
        Some(&json!({"label": "edited"}))
    );
    assert_eq!(
        store.activation("configured-plugin").expect("read edit"),
        Some(edited.clone())
    );
    store
        .pin_active("configured-plugin")
        .expect("edited selection stays pinnable");

    // Either stale revision is a conflict and leaves the record untouched.
    // 任一陈旧修订均为冲突，且不改动记录。
    for (package_revision, settings_revision) in [
        (edited.revision, selected.settings().revision),
        (edited.revision + 1, edited.settings().revision),
    ] {
        let error = store
            .reconfigure(
                "configured-plugin",
                package_revision,
                settings_revision,
                configure(json!({"label": "stale"})),
            )
            .expect_err("stale revision");
        assert_eq!(error.code, VulcanPluginStoreErrorCode::Conflict);
    }
    // Schema-invalid documents and unstaged credentials are rejected before any write.
    // 模式无效文档及未暂存凭据在任何写入之前被拒绝。
    for change in [
        configure(json!({"label": 123})),
        rotate(&VulcanPluginCredentialRevision::allocate()),
    ] {
        store
            .reconfigure(
                "configured-plugin",
                edited.revision,
                edited.settings().revision,
                change,
            )
            .expect_err("invalid settings change");
    }
    assert_eq!(
        store
            .activation("configured-plugin")
            .expect("read after rejections"),
        Some(edited)
    );
}

/// Unfinished transactions, missing records and tombstones reject edits; transaction-held credentials stay reachable.
/// 未完成事务、缺失记录及停用记录拒绝编辑；事务持有的凭据保持可达。
#[test]
fn settings_reconfigure_rejects_transactions_and_unselected_packages() {
    let temp = tempfile::tempdir().expect("create settings fixture");
    let (store, package, credentials) = fixture(temp.path(), false);
    let change = || VulcanPluginSettingsChange::default();
    assert_eq!(
        store
            .reconfigure("configured-plugin", 0, 0, change())
            .expect_err("nothing published")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    let selected = store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            credentials,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("select package");

    // The durable journal, not the dropped handle, keeps the transaction unfinished.
    // 使事务保持未完成的是持久日志，而不是已释放句柄。
    let target_credentials = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &package, &target_credentials);
    drop(
        store
            .begin_activation_transaction(
                uuid::Uuid::new_v4(),
                &package,
                selected.revision,
                package.required_permissions.clone(),
                target_credentials.clone(),
                VulcanPluginConfigurationSelection::NotDeclared {},
            )
            .expect("begin transaction"),
    );
    assert_eq!(
        store
            .reconfigure(
                "configured-plugin",
                selected.revision,
                selected.settings().revision,
                change(),
            )
            .expect_err("transaction pending")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    assert!(
        store
            .collect_unreachable_credentials("configured-plugin", &EmptyCredentialBackend)
            .expect("sweep during transaction")
            .is_empty()
    );
    assert_eq!(
        phase(&store, &target_credentials),
        VulcanPluginCredentialPhase::Staged
    );

    // A tombstone has no package to validate settings against.
    // 停用记录没有可用于校验设置的插件包。
    let temp = tempfile::tempdir().expect("create tombstone fixture");
    let (store, package, credentials) = fixture(temp.path(), false);
    store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            credentials,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("select package");
    let removed = store
        .deactivate("configured-plugin", 1)
        .expect("publish tombstone");
    assert_eq!(
        store
            .reconfigure(
                "configured-plugin",
                removed.revision,
                removed.settings().revision,
                change(),
            )
            .expect_err("tombstone")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
}

/// Superseded and abandoned revisions are collected; the selected revision and leased revisions are kept.
/// 回收被取代及被放弃的修订；保留已选修订及有租约的修订。
#[test]
fn unreachable_credentials_are_collected_and_reachable_ones_kept() {
    let temp = tempfile::tempdir().expect("create collection fixture");
    let (store, package, first) = fixture(temp.path(), false);
    let selected = store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            first.clone(),
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("select first credentials");
    let second = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &package, &second);
    store
        .reconfigure(
            "configured-plugin",
            selected.revision,
            selected.settings().revision,
            rotate(&second),
        )
        .expect("rotate credentials");
    // A staged revision held by a reader is still reachable.
    // 被读者持有的已暂存修订仍然可达。
    let leased = VulcanPluginCredentialRevision::allocate();
    stage_empty_credentials(&store, &package, &leased);
    let reader = store
        .pin_credentials(&package, &leased)
        .expect("pin leased revision");
    // A parentless writer that never finished leaves an abandoned preparation.
    // 从未完成的无父写入者留下被放弃的准备。
    let abandoned = VulcanPluginCredentialRevision::allocate();
    drop(
        store
            .begin_credential_staging(
                &package,
                abandoned.clone(),
                &VulcanPluginCredentialValues::new(),
            )
            .expect("begin abandoned staging"),
    );
    assert_eq!(
        phase(&store, &abandoned),
        VulcanPluginCredentialPhase::Preparing
    );

    let collected: BTreeSet<_> = store
        .collect_unreachable_credentials("configured-plugin", &EmptyCredentialBackend)
        .expect("first sweep")
        .into_iter()
        .collect();
    assert_eq!(
        collected,
        BTreeSet::from([first.clone(), abandoned.clone()])
    );
    assert_eq!(
        phase(&store, &first),
        VulcanPluginCredentialPhase::Discarded
    );
    assert_eq!(
        phase(&store, &abandoned),
        VulcanPluginCredentialPhase::Discarded
    );
    assert_eq!(phase(&store, &second), VulcanPluginCredentialPhase::Staged);
    assert_eq!(phase(&store, &leased), VulcanPluginCredentialPhase::Staged);

    drop(reader);
    assert_eq!(
        store
            .collect_unreachable_credentials("configured-plugin", &EmptyCredentialBackend)
            .expect("second sweep"),
        vec![leased]
    );
    assert!(
        store
            .collect_unreachable_credentials("configured-plugin", &EmptyCredentialBackend)
            .expect("idle sweep")
            .is_empty()
    );
    store
        .pin_active("configured-plugin")
        .expect("selected credentials survive every sweep");
}

/// Bundled settings records are created and edited only through their own revision, carry no credentials, and block package transactions.
/// 内建设置记录只经其自身修订创建及编辑，不携带凭据，并阻止插件包交易。
#[test]
fn bundled_settings_records_are_revisioned_and_reject_package_transactions() {
    let temp = tempfile::tempdir().expect("create bundled settings fixture");
    let (store, package, credentials) = fixture(temp.path(), true);
    let document = |label: &str| VulcanPluginConfigurationSelection::Document {
        value: json!({ "label": label }),
    };

    let created = store
        .configure_bundled("configured-plugin", 0, document("first"))
        .expect("create bundled settings");
    assert_eq!(created.package(), &VulcanPluginPackageSelection::Bundled {});
    assert!(created.credential_revision().is_none());
    assert!(created.approved_permissions().is_empty());
    let edited = store
        .configure_bundled(
            "configured-plugin",
            created.settings().revision,
            document("second"),
        )
        .expect("edit bundled settings");
    assert_eq!(edited.revision, created.revision);
    assert_eq!(edited.settings().revision, created.settings().revision + 1);
    assert_eq!(
        edited.configuration().value(),
        Some(&json!({"label": "second"}))
    );

    // A stale settings revision, a managed edit, a selection and a deactivation all leave the record untouched.
    // 陈旧设置修订、受管编辑、选择及停用均不改动记录。
    let stale = store
        .configure_bundled(
            "configured-plugin",
            created.settings().revision,
            document("stale"),
        )
        .expect_err("stale bundled revision");
    assert_eq!(stale.code, VulcanPluginStoreErrorCode::Conflict);
    store
        .reconfigure(
            "configured-plugin",
            edited.revision,
            edited.settings().revision,
            configure(json!({"label": "managed"})),
        )
        .expect_err("bundled records are not managed settings");
    let selection = store
        .activate_configured(
            &package,
            edited.revision,
            package.required_permissions.clone(),
            credentials,
            package.default_configuration_for_test(),
        )
        .expect_err("package selection over bundled settings");
    assert_eq!(selection.code, VulcanPluginStoreErrorCode::Conflict);
    let deactivation = store
        .deactivate("configured-plugin", edited.revision)
        .expect_err("deactivation over bundled settings");
    assert_eq!(deactivation.code, VulcanPluginStoreErrorCode::Conflict);
    assert_eq!(
        store
            .activation("configured-plugin")
            .expect("read after rejections"),
        Some(edited)
    );
}

/// A managed generation record without a credential revision and a bundled record with one are both invalid.
/// 缺少凭据修订的受管代次记录及携带凭据的内建记录均无效。
#[test]
fn credential_revision_presence_must_match_package_kind() {
    let temp = tempfile::tempdir().expect("create credential presence fixture");
    let (store, package, credentials) = fixture(temp.path(), true);
    store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            credentials.clone(),
            package.default_configuration_for_test(),
        )
        .expect("select configured package");
    let path = store.root.join("active/configured-plugin.json");
    let original = fs::read(&path).expect("read managed record");
    let mut record: serde_json::Value =
        serde_json::from_slice(&original).expect("decode managed record");
    assert!(
        record["settings"]
            .as_object_mut()
            .expect("settings section")
            .remove("credential_revision")
            .is_some(),
        "managed record format changed; the credential field may have moved"
    );
    fs::write(&path, serde_json::to_vec(&record).expect("encode record"))
        .expect("write managed record without credentials");
    store
        .activation("configured-plugin")
        .expect_err("managed record without credentials");

    let mut bundled: serde_json::Value =
        serde_json::from_slice(&original).expect("decode managed record");
    bundled["package"] = json!({"kind": "bundled"});
    bundled["approved_permissions"] = json!([]);
    fs::write(&path, serde_json::to_vec(&bundled).expect("encode record"))
        .expect("write bundled record with credentials");
    store
        .activation("configured-plugin")
        .expect_err("bundled record with credentials");
}
