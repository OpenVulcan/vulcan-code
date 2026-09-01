//! Capture source mapping and incomplete-proof regression tests.
//! 捕获来源映射及不完整证明回归测试。

use super::*;
use vulcan_ai_contract::ContentItem;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

/// Creates a source prompt named `id`; `attached` controls one metadata-only fixture resource.
/// 创建名为`id`的来源提示词；`attached`控制一个仅元数据夹具资源。
/// Returns a canonical message without reading or creating any local file.
/// 返回规范消息，不读取或创建任何本地文件。
fn source(id: &str, attached: bool) -> ConversationMessage {
    ConversationMessage {
        id: id.into(),
        payload: ConversationPayload::UserText {
            text: format!("text-{id}"),
            attachments: if attached {
                vec![ConversationAttachment {
                    kind: ConversationAttachmentKind::Image,
                    file_name: "capture.png".into(),
                    mime_type: "image/png".into(),
                    local_path: String::new(),
                    source_path: None,
                    preview_local_path: None,
                    asset_id: None,
                    resource_path: Some("project-resource://1".into()),
                    byte_len: Some(3),
                    source_sha256: None,
                    delivery_mode: None,
                }]
            } else {
                Vec::new()
            },
        },
    }
}

/// Returns an exact public message with `role` and `text` for position and atomicity checks.
/// 返回具有`role`及`text`的精确公共消息，用于位置及原子性检查。
fn public(role: MessageRole, text: &str) -> AiMessage {
    AiMessage {
        role,
        content: vec![ContentItem::Text { text: text.into() }],
        provider_context_items: Vec::new(),
    }
}

/// Missing source authority rejects attached prompts, while text-only capture remains available.
/// 缺少来源权威时拒绝含附件提示词，而纯文本捕获仍可使用。
#[test]
fn capture_requires_authority_only_for_attached_prompts() {
    assert!(validate_capture_authority(&[source("text", false)]).is_ok());
    assert!(
        validate_capture_authority(&[source("media", true)])
            .unwrap_err()
            .contains("frozen source projection")
    );
}

/// Explicit assembly positions preserve chronological source order independently of system prefix and key order.
/// 显式组装位置保留来源时序，不受系统前缀及键顺序影响。
#[test]
fn capture_media_uses_exact_positions_not_lexical_order() -> Result<(), String> {
    for with_system in [false, true] {
        // Actual source chronology intentionally differs from BTreeMap order.
        // 真实来源时序故意不同于BTreeMap顺序。
        let sources = vec![source("z-first", true), source("a-second", true)];
        // Public assembly includes an optional independently owned system prefix.
        // 公共组装包含可选独立所有的系统前缀。
        let mut messages = Vec::new();
        if with_system {
            messages.push(public(MessageRole::System, "system"));
        }
        // Mapping is recorded alongside insertion, as in production request assembly.
        // 映射随插入记录，与生产请求组装相同。
        let mut positions = BTreeMap::new();
        for source in &sources {
            positions.insert(source.id.clone(), messages.len());
            messages.push(public(MessageRole::User, "provisional"));
        }
        // Replacements are keyed by source identity, never by current vector or lexical order.
        // 替换项以来源身份为键，绝不按当前向量或字典序关联。
        let replacements = BTreeMap::from([
            ("a-second".into(), public(MessageRole::User, "second media")),
            ("z-first".into(), public(MessageRole::User, "first media")),
        ]);
        apply_capture_media(&sources, &positions, &mut messages, replacements)?;
        assert_eq!(
            messages[positions["z-first"]],
            public(MessageRole::User, "first media")
        );
        assert_eq!(
            messages[positions["a-second"]],
            public(MessageRole::User, "second media")
        );
        if with_system {
            assert_eq!(messages[0], public(MessageRole::System, "system"));
        }
    }
    Ok(())
}

/// Incomplete, extra, wrong-role and wrong-position replacements fail before changing any earlier message.
/// 不完整、多余、错误角色及错误位置替换在修改任何较早消息之前失败。
#[test]
fn capture_media_rejects_invalid_projection_atomically() {
    // Two attached prompts make partial projection a real omission rather than valid text-only selection.
    // 两条含附件提示词使部分投影构成真实遗漏，而非合法纯文本选择。
    let sources = vec![source("first", true), source("second", true)];
    // Preserve an independent system message at position zero.
    // 在零位置保留独立系统消息。
    let original = vec![
        public(MessageRole::System, "system"),
        public(MessageRole::User, "first"),
        public(MessageRole::User, "second"),
    ];
    // The real assembled mapping points only to user messages.
    // 真实组装映射只指向用户消息。
    let positions = BTreeMap::from([("first".into(), 1), ("second".into(), 2)]);
    // A complete valid projection serves as the immutable baseline for each isolated corruption.
    // 完整有效投影用作每个隔离损坏分支的不可变基准。
    let valid = BTreeMap::from([
        ("first".into(), public(MessageRole::User, "new first")),
        ("second".into(), public(MessageRole::User, "new second")),
    ]);
    for case in 0..6 {
        // Every failure starts from the identical original request.
        // 每次失败都从相同原请求开始。
        let mut messages = original.clone();
        // Only one declared contract property is corrupted per case.
        // 每个用例仅损坏一项声明契约属性。
        let mut replacements = valid.clone();
        // Position corruption cannot be repaired by guessing a prefix offset.
        // 位置损坏不能通过猜测前缀偏移修复。
        let mut invalid_positions = positions.clone();
        match case {
            0 => {
                replacements.remove("second");
            }
            1 => {
                replacements.insert("unknown".into(), public(MessageRole::User, "bad"));
            }
            2 => {
                replacements.insert("second".into(), public(MessageRole::Assistant, "bad"));
            }
            3 => {
                invalid_positions.insert("second".into(), 0);
            }
            4 => {
                invalid_positions.insert("second".into(), 99);
            }
            _ => {
                invalid_positions.remove("second");
            }
        }
        assert!(
            apply_capture_media(&sources, &invalid_positions, &mut messages, replacements).is_err()
        );
        assert_eq!(messages, original);
    }
}
