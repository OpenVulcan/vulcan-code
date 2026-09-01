//! Real TOML persistence tests for sparse, bounded media settings.
//! 稀疏且有界媒体设置的真实 TOML 持久化测试。

use super::*;
use vulcan_provider_adapters::{MediaDeliveryPolicy, PersistedAiConfiguration};

/// Verifies real persistence, reload and invalid-write atomicity in an isolated directory.
/// 在隔离目录验证真实持久化、重新加载及无效写入原子性。
#[test]
fn media_delivery_policy_round_trips_without_rewriting_missing_defaults() {
    // Temporary paths isolate every journal and provider source from user configuration.
    // 临时路径使每份日志及供应商来源与用户配置隔离。
    let directory = tempfile::tempdir().expect("media policy fixture or operation must be valid");
    // The exact settings path and matching store are the same pair used by production persistence.
    // 精确设置路径与配对存储使用生产持久化相同的组合。
    let path = directory.path().join("ai-connections.toml");
    // The store keeps any recoverable transaction inside the temporary directory.
    // 存储将所有可恢复事务限定在临时目录内。
    let store = crate::manifest_store::ManifestStoreLayout::new(directory.path());
    // An old configuration intentionally contains no new media policy field.
    // 旧配置有意不包含新媒体策略字段。
    let mut configuration = PersistedAiConfiguration::default();
    save_public_ai_connections_at_path(&configuration, &path, &store)
        .expect("media policy fixture or operation must be valid");
    // Reading effective defaults must not rewrite the exact persisted bytes.
    // 读取生效默认值不能改写精确持久化字节。
    let original_bytes =
        std::fs::read(&path).expect("media policy fixture or operation must be valid");
    // The normal loader exercises migration, schema checks and custom-provider hydration.
    // 正常加载器执行迁移、结构校验和自定义供应商补水。
    let loaded = load_public_ai_connections_file_from_path(&path)
        .expect("media policy fixture or operation must be valid");
    assert!(loaded.configuration.media_delivery_policy.is_none());
    assert_eq!(
        loaded.configuration.effective_media_delivery_policy(),
        MediaDeliveryPolicy::default()
    );
    assert_eq!(
        std::fs::read(&path).expect("media policy fixture or operation must be valid"),
        original_bytes
    );
    // Explicit configuration preserves separate purpose fields and exact disabled values.
    // 显式配置保留独立用途字段及精确禁用值。
    let mut policy = MediaDeliveryPolicy::default();
    policy.conversation.file.without_upload_max_bytes = 0;
    policy.independent_extension.audio.with_upload_max_bytes = 1234;
    configuration
        .set_media_delivery_policy(policy)
        .expect("media policy fixture or operation must be valid");
    configuration.usage_refresh_interval_seconds = 601;
    save_public_ai_connections_at_path(&configuration, &path, &store)
        .expect("media policy fixture or operation must be valid");
    assert_eq!(
        load_public_ai_connections_file_from_path(&path)
            .expect("media policy fixture or operation must be valid")
            .configuration,
        configuration
    );
    // Invalid direct assignment must be rejected before a transaction modifies the previous file.
    // 无效直接赋值必须在事务修改原文件之前被拒绝。
    let saved_bytes =
        std::fs::read(&path).expect("media policy fixture or operation must be valid");
    policy.version = 999;
    configuration.media_delivery_policy = Some(policy);
    assert!(save_public_ai_connections_at_path(&configuration, &path, &store).is_err());
    assert_eq!(
        std::fs::read(&path).expect("media policy fixture or operation must be valid"),
        saved_bytes
    );
    configuration.reset_media_delivery_policy();
    save_public_ai_connections_at_path(&configuration, &path, &store)
        .expect("media policy fixture or operation must be valid");
    assert!(
        load_public_ai_connections_file_from_path(&path)
            .expect("media policy fixture or operation must be valid")
            .configuration
            .media_delivery_policy
            .is_none()
    );
    assert_eq!(configuration.usage_refresh_interval_seconds, 601);
}

/// Verifies the TOML boundary rejects fractional, negative, unknown and incomplete settings.
/// 验证 TOML 边界拒绝小数、负数、未知字段及不完整设置。
#[test]
fn media_delivery_policy_toml_rejects_invalid_values() {
    // The real serializer provides the authoritative field names for the malformed fixtures.
    // 真实序列化器为无效夹具提供权威字段名称。
    let file = PublicAiConnectionsFile {
        schema_version: 1,
        configuration: PersistedAiConfiguration {
            media_delivery_policy: Some(MediaDeliveryPolicy::default()),
            ..PersistedAiConfiguration::default()
        },
    };
    // Every replacement changes an actually serialized field rather than guessing a TOML path.
    // 每次替换都修改实际序列化字段，而不是猜测 TOML 路径。
    let valid =
        toml::to_string_pretty(&file).expect("media policy fixture or operation must be valid");
    assert!(valid.contains("with_upload_max_bytes = 0"));
    for replacement in [
        "with_upload_max_bytes = -1",
        "with_upload_max_bytes = 1.5",
        "with_upload_max_byte = 0",
        "with_upload_max_bytes = 50331649",
    ] {
        assert!(
            parse_public_ai_connections_text(&valid.replacen(
                "with_upload_max_bytes = 0",
                replacement,
                1
            ))
            .is_err()
        );
    }
}
