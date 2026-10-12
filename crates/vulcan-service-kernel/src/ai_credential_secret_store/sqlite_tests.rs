//! SQLite behavior tests for native account storage and independent credential schema versions.
//! 原生账户存储与独立凭据结构版本的 SQLite 行为测试。

use super::*;
use vulcan_provider_adapters::OpenVulcanRegion;
use vulcan_workspace_store::{
    data_plane::{HostRuntimeMetadata, prepare_application_schema_for_test},
    local_database_runtime::LogicalDatabase,
};

/// Takes no inputs and returns synthetic native material for storage tests without network access.
/// 无需参数，返回存储测试使用的合成原生凭据，不访问网络。
fn native_secret() -> CredentialSecretEnvelope {
    CredentialSecretEnvelope::new(CredentialSecretMaterial::OpenVulcanAuth {
        access_token: "a".repeat(43),
        authorization_id: uuid::Uuid::new_v4().to_string(),
        region: OpenVulcanRegion::China,
        expires_at_unix_ms: 2_000_000_000_000,
        rotation_pending: false,
    })
    .expect("synthetic native credential")
}

/// With no inputs, verifies old tables stay untouched while new logins survive repeated initialization.
/// 无需参数，验证旧表保持原样且新登录能跨重复初始化保留，成功时正常返回。
#[tokio::test]
async fn old_credential_table_is_ignored_without_blocking_native_login() {
    // directory and controller bind exclusively to a private, initially empty database.
    // directory 和 controller 仅绑定私有且初始为空的数据库。
    let directory = tempfile::tempdir().expect("private database directory");
    let controller = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: directory.path().to_path_buf(),
        sqlite_path: directory.path().join("app.sqlite3"),
        sqlite_binding_id: String::from("old-credential-table"),
        system_statistics_sqlite_path: directory.path().join("usage.sqlite3"),
        system_statistics_sqlite_binding_id: String::from("old-credential-table-usage"),
    });
    // connection seeds a preexisting table whose CHECK rejects the new material kind.
    // connection 预置旧表，其 CHECK 约束拒绝新的凭据材料类别。
    let connection = controller
        .local
        .connect(LogicalDatabase::AppState)
        .expect("private connection");
    connection.execute_batch(
        "CREATE TABLE ai_credential_secrets (
             secret_reference TEXT PRIMARY KEY,
             material_kind TEXT NOT NULL CHECK(material_kind IN (
                 'api_key', 'static_bearer', 'open_ai_codex_oauth', 'claude_code_oauth',
                 'google_oauth', 'mini_max_oauth', 'kimi_coding_plan_oauth'
             )),
             secret_value TEXT NOT NULL
         );
         INSERT INTO ai_credential_secrets VALUES ('retained-reference', 'api_key', 'synthetic-old-key');
         CREATE TABLE app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO app_metadata VALUES ('retained-setting', 'unchanged');"
    ).expect("seed old credential constraints");
    // old_schema is evidence that initialization neither alters nor replaces the old table.
    // old_schema 用于验证初始化没有修改或替换旧表。
    let old_schema: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'ai_credential_secrets'",
            [],
            |row| row.get(0),
        )
        .expect("old schema");
    prepare_application_schema_for_test(&controller)
        .await
        .expect("initialize beside old table");
    // store must use its declared version, even when the same reference exists in the old table.
    // 即使相同引用存在于旧表，store 也必须使用其声明的版本。
    let store = SqliteAiCredentialSecretStore::new(controller.clone());
    assert!(
        store
            .read_optional_secret("retained-reference")
            .await
            .expect("ignore old reference")
            .is_none()
    );
    // original is an explicit new login; it does not derive any field from the old row.
    // original 是显式的新登录，不从旧记录推导任何字段。
    let original = native_secret();
    store
        .create_secret("retained-reference", &original)
        .await
        .expect("persist explicit new login");
    prepare_application_schema_for_test(&controller)
        .await
        .expect("repeat initialization");
    assert!(
        store
            .read_secret("retained-reference")
            .await
            .expect("retain new login")
            == original
    );
    store
        .delete_secret("retained-reference")
        .await
        .expect("delete only current login");
    assert!(
        store
            .read_optional_secret("retained-reference")
            .await
            .expect("do not revive old login")
            .is_none()
    );
    assert_eq!(
        connection
            .query_row::<String, _, _>(
                "SELECT sql FROM sqlite_master WHERE name = 'ai_credential_secrets'",
                [],
                |row| row.get(0)
            )
            .expect("unchanged old schema"),
        old_schema
    );
    assert_eq!(connection.query_row::<String, _, _>(
        "SELECT secret_value FROM ai_credential_secrets WHERE secret_reference = 'retained-reference'", [], |row| row.get(0)
    ).expect("unchanged old row"), "synthetic-old-key");
    assert_eq!(
        connection
            .query_row::<String, _, _>(
                "SELECT value FROM app_metadata WHERE key = 'retained-setting'",
                [],
                |row| row.get(0)
            )
            .expect("retained application setting"),
        "unchanged"
    );
}

/// With no inputs, verifies SQLite rejects incomplete native records and foreign authorization markers.
/// 无需参数，验证 SQLite 拒绝不完整原生记录及错误类别的授权标记，成功时正常返回。
#[tokio::test]
async fn native_credential_sqlite_constraints_reject_invalid_rows() {
    // fixture uses the same DDL and bound insert as the actual login path.
    // fixture 使用实际登录路径的相同建表定义与绑定插入。
    let fixture = repair_tests::CredentialRepairFixture::new().await;
    // valid is the control record proving failures below come from each isolated field mutation.
    // valid 是控制记录，用于证明下列失败分别来自单个字段变更。
    let valid = credential_record_from_envelope("native-constraint", &native_secret())
        .expect("valid record");
    create_ai_credential_secret_record(&fixture.store.controller, &valid)
        .await
        .expect("valid native row");
    fixture
        .store
        .delete_secret(&valid.secret_reference)
        .await
        .expect("remove control row");
    // mutations bypass envelope validation so the persisted constraints are tested directly.
    // mutations 绕过封装校验，直接验证持久化约束。
    let mutations: &[(&str, fn(&mut AiCredentialSecretRecord))] = &[
        ("missing access token", |row| row.access_token = None),
        ("blank access token", |row| {
            row.access_token = Some(String::from(" "))
        }),
        ("missing device", |row| row.provider_device_id = None),
        ("blank device", |row| {
            row.provider_device_id = Some(String::from(" "))
        }),
        ("missing region", |row| row.provider_region = None),
        ("unknown region", |row| {
            row.provider_region = Some(String::from("other"))
        }),
        ("missing expiry", |row| row.expires_at_unix_ms = None),
        ("nonpositive expiry", |row| row.expires_at_unix_ms = Some(0)),
        ("foreign refresh token", |row| {
            row.refresh_token = Some(String::from("refresh"))
        }),
        ("foreign issue time", |row| row.issued_at_unix_ms = Some(1)),
        ("foreign authorization", |row| {
            row.authorization_kind = Some(String::from("browser_callback"))
        }),
        ("unknown authorization", |row| {
            row.authorization_kind = Some(String::from("other"))
        }),
        ("unknown material", |row| {
            row.material_kind = String::from("other")
        }),
    ];
    for (label, mutate) in mutations {
        // invalid changes exactly one storage field of the control record.
        // invalid 只改变控制记录的一个存储字段。
        let mut invalid = valid.clone();
        mutate(&mut invalid);
        assert!(
            create_ai_credential_secret_record(&fixture.store.controller, &invalid)
                .await
                .is_err(),
            "SQLite accepted {label}"
        );
        assert!(fixture.record(&valid.secret_reference).await.is_none());
    }
    // foreign_fence proves allowing rotation_pending cannot grant that state to ordinary API keys.
    // foreign_fence 验证允许 rotation_pending 不会让普通 API Key 获得该状态。
    let mut foreign_fence = credential_record_from_envelope(
        "foreign-fence",
        &CredentialSecretEnvelope::new_api_key(String::from("synthetic-key")).expect("key fixture"),
    )
    .expect("key record");
    foreign_fence.authorization_kind = Some(String::from("rotation_pending"));
    assert!(
        create_ai_credential_secret_record(&fixture.store.controller, &foreign_fence)
            .await
            .is_err()
    );
}
