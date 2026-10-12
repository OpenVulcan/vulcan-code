//! SQLite behavior tests for native account credential storage.
//! 原生账户凭据存储的 SQLite 行为测试。

use super::*;
use vulcan_provider_adapters::OpenVulcanRegion;

/// Breaks one field of a stored row so a single persisted constraint can be exercised.
/// 破坏存储记录的一个字段，以便单独验证一项持久化约束。
type RecordMutation = fn(&mut AiCredentialSecretRecord);

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
    let mutations: &[(&str, RecordMutation)] = &[
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
