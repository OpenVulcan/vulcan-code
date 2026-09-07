//! Closed source-inspection wire contracts; inspection never grants deletion permission.
//! 封闭的来源检查传输契约；检查绝不授予删除权限。
use serde_json::json;
use vulcan_service_api::{
    VmmNativeRemovalConfirmation, VmmNativeRemovalQuery, VmmNativeRemovalTarget,
};

/// Checks exact tagged selection and rejects hidden mutation or profile flags.
/// 检查准确带标签选择，拒绝隐藏的变更或画像标志。
#[test]
fn removal_inspection_rejects_undeclared_choices() {
    // Wire fields are fixed; callers cannot mix session and turn identity sets.
    // 传输字段固定，调用方不能混合会话与回合身份集合。
    let valid = json!({"vmmProjectId": 2, "selection": {"kind": "sessions", "ids": ["session-a"]}});
    let decoded: VmmNativeRemovalQuery = serde_json::from_value(valid.clone()).expect("query");
    assert_eq!(decoded.vmm_project_id, 2);
    assert_eq!(
        decoded.selection,
        VmmNativeRemovalTarget::Sessions {
            ids: vec!["session-a".to_owned()]
        }
    );
    assert_eq!(serde_json::to_value(decoded).expect("roundtrip"), valid);
    for invalid in [
        json!({"vmmProjectId": 2, "selection": {"kind": "sessions", "ids": ["a"]}, "confirmed": true}),
        json!({"vmmProjectId": 2, "selection": {"kind": "sessions", "ids": ["a"], "deleteUserProfile": true}}),
        json!({"vmmProjectId": 2, "selection": {"kind": "turn", "ids": ["vmm-turn-1"]}}),
        json!({"selection": {"kind": "sessions", "ids": ["a"]}}),
        json!({"vmmProjectId": 2, "selection": {"kind": "turns", "turnIds": ["vmm-turn-1"]}}),
    ] {
        assert!(serde_json::from_value::<VmmNativeRemovalQuery>(invalid).is_err());
    }
}

/// Rejects target replacement, missing consent and hidden profile deletion during confirmation.
/// 拒绝确认过程中的目标替换、缺少同意及隐藏画像删除。
#[test]
fn removal_confirmation_only_accepts_the_prepared_identity_and_consent() {
    // Valid confirmations contain only the server token and explicit consent.
    // 有效确认只包含服务端令牌及明确同意。
    let value = json!({"previewId": "opaque", "confirmed": true});
    let confirmation: VmmNativeRemovalConfirmation =
        serde_json::from_value(value.clone()).expect("closed confirmation");
    assert!(confirmation.confirmed);
    assert_eq!(
        serde_json::to_value(confirmation).expect("roundtrip"),
        value
    );
    for invalid in [
        json!({"previewId": "opaque"}),
        json!({"confirmed": true}),
        json!({"previewId": "opaque", "confirmed": "true"}),
        json!({"previewId": "opaque", "confirmed": true, "selection": {"kind": "sessions", "ids": ["other"]}}),
        json!({"previewId": "opaque", "confirmed": true, "deleteUserProfile": true}),
    ] {
        assert!(serde_json::from_value::<VmmNativeRemovalConfirmation>(invalid).is_err());
    }
}
