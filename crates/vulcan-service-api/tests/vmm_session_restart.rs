//! Verifies the closed restart request contract and exact response wire shape.
//! 验证封闭重新启用请求契约及准确响应传输结构。
use serde_json::json;
use vulcan_service_api::{
    VmmMemoryActionResponse, VmmNativeSessionRestartConfirmation, VmmNativeSessionRestartPreview,
    VmmNativeSessionRestartSelection,
};

/// Rejects missing approval and replacement targets instead of accepting ambiguous confirmations.
/// 拒绝缺失同意或替换目标，不接受有歧义的确认。
#[test]
fn restart_confirmation_does_not_accept_replacement_targets() {
    for invalid in [
        json!({"previewId":"preview"}),
        json!({"previewId":"preview", "confirmed":true, "vmmProjectId":9}),
        json!({"previewId":"preview", "confirmed":true, "sessionSourceIds":["replacement"]}),
    ] {
        assert!(serde_json::from_value::<VmmNativeSessionRestartConfirmation>(invalid).is_err());
    }
    let confirmation: VmmNativeSessionRestartConfirmation =
        serde_json::from_value(json!({"previewId":"preview", "confirmed":true}))
            .expect("explicit confirmation");
    assert!(confirmation.confirmed);
    assert_eq!(confirmation.preview_id, "preview");
    assert!(
        serde_json::from_value::<VmmNativeSessionRestartSelection>(
            json!({"vmmProjectId":7,"sessionSourceIds":["exact"],"archived":false})
        )
        .is_err()
    );
}

/// Preserves the typed response discriminant and camel-case exact IDs consumed by the frontend.
/// 保留前端消费的类型化响应判别字段及驼峰准确 ID。
#[test]
fn restart_preview_roundtrips_the_declared_response() {
    let response =
        VmmMemoryActionResponse::NativeSessionRestartPreview(VmmNativeSessionRestartPreview {
            preview_id: "preview".to_owned(),
            storage_version: 12,
            selection: VmmNativeSessionRestartSelection {
                vmm_project_id: 7,
                session_source_ids: vec!["exact".to_owned()],
            },
        });
    let wire = serde_json::to_value(&response).expect("serialize preview");
    assert_eq!(
        wire,
        json!({"kind":"native_session_restart_preview", "value":{"previewId":"preview","storageVersion":12,"selection":{"vmmProjectId":7,"sessionSourceIds":["exact"]}}})
    );
    let decoded: VmmMemoryActionResponse =
        serde_json::from_value(wire.clone()).expect("decode preview");
    assert_eq!(serde_json::to_value(decoded).expect("roundtrip"), wire);
}
