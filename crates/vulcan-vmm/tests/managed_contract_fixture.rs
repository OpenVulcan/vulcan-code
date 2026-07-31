//! Cross-language managed-contract fixture validation.
//! 跨语言托管契约固定夹具校验。

use vulcan_vmm::ManagedConfig;

/// Verifies the shared version-one fixture loses no fields in the Rust protocol mirror.
/// 验证共享的一版固定夹具经过 Rust 协议镜像往返后不会丢失字段。
#[test]
fn managed_contract_fixture_round_trips_semantically() {
    let body = include_str!("fixtures/vulcan-managed-contract-v1.json");
    let expected: serde_json::Value =
        serde_json::from_str(body).expect("managed contract fixture must contain valid JSON");
    let manifest: ManagedConfig =
        serde_json::from_str(body).expect("managed contract fixture must match the Rust mirror");
    let actual = serde_json::to_value(manifest)
        .expect("managed contract fixture must serialize after typed decoding");

    assert_eq!(actual, expected);
}
