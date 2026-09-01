//! Boundary, persistence and historical-identity tests for the shared media policy.
//! 共享媒体策略的边界、持久化及历史身份测试。

use super::*;

/// Keeps current interface hard limits effective for both historical delivery forms and never reuploads history.
/// 两种历史投递形式均保持当前接口硬限制生效，且绝不重新上传历史。
#[test]
fn historical_delivery_respects_current_upload_hard_limit() -> Result<(), String> {
    for origin in [
        MediaDeliveryOrigin::HistoricalInline,
        MediaDeliveryOrigin::HistoricalUpload {
            original_target_matches: true,
            reference_usable: true,
        },
    ] {
        // The exact four-byte source exceeds only the new interface's three-byte hard bound.
        // 精确四字节来源仅超过新接口的三字节硬边界。
        let mut planner = MediaDeliveryPlanner::new(
            MediaDeliveryPolicy::default(),
            MediaDeliveryPurpose::Conversation,
        )?;
        assert_eq!(
            planner.plan(MediaDeliveryFacts {
                kind: ResourceKind::Image,
                raw_bytes: 4,
                source_available: true,
                target_supported: true,
                target_max_bytes: None,
                inline_supported: true,
                upload: MediaUploadAvailability::Available { max_bytes: Some(3) },
                origin,
            })?,
            MediaDeliveryDecision::Attachment {
                reason: MediaDeliveryRejection::UploadSizeLimit
            }
        );
        assert_eq!(planner.encoded_bytes_reserved(), 0);
    }
    Ok(())
}
use crate::PersistedAiConfiguration;
use serde_json::json;
use vulcan_ai_contract::ResourceKind;

/// Builds verified new video facts of `raw_bytes`; returns no-upload, inline-capable input.
/// 构建大小为 `raw_bytes` 的已验证新视频事实，返回无上传且支持内联的输入。
fn video(raw_bytes: u64) -> MediaDeliveryFacts {
    MediaDeliveryFacts {
        kind: ResourceKind::Video,
        raw_bytes,
        source_available: true,
        target_supported: true,
        target_max_bytes: None,
        inline_supported: true,
        upload: MediaUploadAvailability::Absent,
        origin: MediaDeliveryOrigin::NewIntent,
    }
}

/// Plans one isolated resource using `policy`, `purpose` and `facts`; returns the exact action.
/// 使用 `policy`、`purpose`、`facts` 规划一个独立资源，返回精确动作。
fn decide(
    policy: MediaDeliveryPolicy,
    purpose: MediaDeliveryPurpose,
    facts: MediaDeliveryFacts,
) -> MediaDeliveryDecision {
    MediaDeliveryPlanner::new(policy, purpose)
        .expect("media policy fixture or operation must be valid")
        .plan(facts)
        .expect("media policy fixture or operation must be valid")
}

/// Verifies the sixteen defaults and separate aggregate budgets; returns normally on success.
/// 验证十六项默认值及独立累计预算；成功时正常返回。
#[test]
fn defaults_cover_all_kinds_and_purposes() {
    for kind in [
        ResourceKind::Image,
        ResourceKind::Audio,
        ResourceKind::Video,
        ResourceKind::File,
    ] {
        assert_eq!(
            MediaDeliveryPolicy::default().conversation.thresholds(kind),
            MediaInlineThresholds {
                with_upload_max_bytes: 0,
                without_upload_max_bytes: 524_288
            }
        );
        assert_eq!(
            MediaDeliveryPolicy::default()
                .independent_extension
                .thresholds(kind),
            MediaInlineThresholds {
                with_upload_max_bytes: 0,
                without_upload_max_bytes: 10_485_760
            }
        );
    }
    assert_eq!(
        MediaDeliveryPolicy::default()
            .conversation
            .max_total_encoded_bytes,
        2_097_152
    );
    assert_eq!(
        MediaDeliveryPolicy::default()
            .independent_extension
            .max_total_encoded_bytes,
        16_777_216
    );
}

/// Exercises every raw threshold independently at equality and one byte over; returns on success.
/// 对每项原始阈值独立验证等于及超出一字节的边界；成功时返回。
#[test]
fn sixteen_thresholds_are_independent_and_inclusive() {
    // Distinct values expose accidental sharing between kind, purpose and upload mode.
    // 不同数值能够暴露类型、用途和上传模式之间的意外共享。
    let mut encoded = serde_json::to_value(MediaDeliveryPolicy::default())
        .expect("media policy fixture or operation must be valid");
    for (purpose_index, purpose_name) in
        ["conversation", "independent_extension"].iter().enumerate()
    {
        for (kind_index, kind_name) in ["image", "audio", "video", "file"].iter().enumerate() {
            encoded[purpose_name][kind_name]["with_upload_max_bytes"] =
                json!(11 + purpose_index * 100 + kind_index * 10);
            encoded[purpose_name][kind_name]["without_upload_max_bytes"] =
                json!(12 + purpose_index * 100 + kind_index * 10);
        }
    }
    // Typed settings preserve all sixteen exact values across serialization.
    // 强类型设置在序列化往返后保留全部十六个精确数值。
    let policy: MediaDeliveryPolicy =
        serde_json::from_value(encoded).expect("media policy fixture or operation must be valid");
    for (purpose_index, purpose) in [
        MediaDeliveryPurpose::Conversation,
        MediaDeliveryPurpose::IndependentExtension,
    ]
    .into_iter()
    .enumerate()
    {
        for (kind_index, kind) in [
            ResourceKind::Image,
            ResourceKind::Audio,
            ResourceKind::Video,
            ResourceKind::File,
        ]
        .into_iter()
        .enumerate()
        {
            for has_upload in [false, true] {
                // The selected threshold is intentionally computed independently of the policy getter.
                // 有意独立于策略读取方法计算选中的阈值。
                let boundary = (if has_upload { 11 } else { 12 })
                    + purpose_index as u64 * 100
                    + kind_index as u64 * 10;
                // Verified upload availability is the only distinction in these inputs.
                // 这些输入仅在已验证上传可用性方面存在差异。
                let facts = MediaDeliveryFacts {
                    kind,
                    upload: if has_upload {
                        MediaUploadAvailability::Available {
                            max_bytes: Some(1000),
                        }
                    } else {
                        MediaUploadAvailability::Absent
                    },
                    ..video(boundary)
                };
                assert_eq!(
                    decide(policy, purpose, facts),
                    MediaDeliveryDecision::Inline {
                        encoded_bytes: media_base64_encoded_len(boundary)
                            .expect("media policy fixture or operation must be valid")
                    }
                );
                assert_eq!(
                    decide(
                        policy,
                        purpose,
                        MediaDeliveryFacts {
                            raw_bytes: boundary + 1,
                            ..facts
                        }
                    ),
                    if has_upload {
                        MediaDeliveryDecision::Upload
                    } else if purpose == MediaDeliveryPurpose::Conversation {
                        MediaDeliveryDecision::Attachment {
                            reason: MediaDeliveryRejection::InlineThreshold,
                        }
                    } else {
                        MediaDeliveryDecision::Reject {
                            reason: MediaDeliveryRejection::InlineThreshold,
                        }
                    }
                );
            }
        }
    }
}

/// Verifies disabled inline and upload-interface precedence; returns normally on success.
/// 验证内联禁用和上传接口限制优先级；成功时正常返回。
#[test]
fn zero_disables_inline_and_upload_hard_limit_cannot_be_bypassed() {
    // The default real-upload branch always uploads, even an empty resource.
    // 默认真实上传分支始终上传，即使资源为空。
    let mut facts = video(0);
    facts.upload = MediaUploadAvailability::Available {
        max_bytes: Some(10),
    };
    assert_eq!(
        decide(
            MediaDeliveryPolicy::default(),
            MediaDeliveryPurpose::Conversation,
            facts
        ),
        MediaDeliveryDecision::Upload
    );
    // Raising inline preference cannot bypass the selected upload interface's smaller bound.
    // 提高内联偏好不能绕过选中上传接口的更小上限。
    let mut policy = MediaDeliveryPolicy::default();
    policy.conversation.video.with_upload_max_bytes = 100;
    facts.raw_bytes = 11;
    assert_eq!(
        decide(policy, MediaDeliveryPurpose::Conversation, facts),
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::UploadSizeLimit
        }
    );
    facts.upload = MediaUploadAvailability::Invalid;
    assert_eq!(
        decide(policy, MediaDeliveryPurpose::Conversation, facts),
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::UploadContractInvalid
        }
    );
    facts.upload = MediaUploadAvailability::Absent;
    facts.raw_bytes = 0;
    policy.conversation.video.without_upload_max_bytes = 0;
    assert_eq!(
        decide(policy, MediaDeliveryPurpose::Conversation, facts),
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::InlineThreshold
        }
    );
}

/// Verifies extension size rejection remains independent of conversation limits; returns on success.
/// 验证扩展大小拒绝独立于会话限制；成功时返回。
#[test]
fn conversation_and_extension_fail_differently() {
    assert!(matches!(
        decide(
            MediaDeliveryPolicy::default(),
            MediaDeliveryPurpose::Conversation,
            video(524_289)
        ),
        MediaDeliveryDecision::Attachment { .. }
    ));
    assert_eq!(
        decide(
            MediaDeliveryPolicy::default(),
            MediaDeliveryPurpose::IndependentExtension,
            video(10_485_760)
        ),
        MediaDeliveryDecision::Inline {
            encoded_bytes: 13_981_016
        }
    );
    assert_eq!(
        decide(
            MediaDeliveryPolicy::default(),
            MediaDeliveryPurpose::IndependentExtension,
            video(10_485_761)
        ),
        MediaDeliveryDecision::Reject {
            reason: MediaDeliveryRejection::InlineThreshold
        }
    );
}

/// Verifies exact aggregate reservations and upload selection without double charging; returns on success.
/// 验证精确累计预留与上传选择不会重复计费；成功时返回。
#[test]
fn aggregate_budget_is_encoded_and_only_inline_reserves_it() {
    // Two one-byte resources each require four Base64 bytes, not one combined encoding.
    // 两个一字节资源各需四个 Base64 字节，不能合并计算编码长度。
    let mut policy = MediaDeliveryPolicy::default();
    policy.conversation.max_total_encoded_bytes = 8;
    policy.conversation.video.with_upload_max_bytes = 100;
    // This planner represents one request and therefore accumulates reservations.
    // 此规划器代表一个请求，因此累计预留量。
    let mut planner = MediaDeliveryPlanner::new(policy, MediaDeliveryPurpose::Conversation)
        .expect("media policy fixture or operation must be valid");
    assert_eq!(
        planner
            .plan(video(1))
            .expect("media policy fixture or operation must be valid"),
        MediaDeliveryDecision::Inline { encoded_bytes: 4 }
    );
    assert_eq!(
        planner
            .plan(video(1))
            .expect("media policy fixture or operation must be valid"),
        MediaDeliveryDecision::Inline { encoded_bytes: 4 }
    );
    assert_eq!(
        planner
            .plan(video(1))
            .expect("media policy fixture or operation must be valid"),
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::EncodedBudget
        }
    );
    assert_eq!(
        planner
            .plan(MediaDeliveryFacts {
                upload: MediaUploadAvailability::Available { max_bytes: None },
                ..video(1)
            })
            .expect("media policy fixture or operation must be valid"),
        MediaDeliveryDecision::Upload
    );
    assert_eq!(planner.encoded_bytes_reserved(), 8);
    assert_eq!(media_base64_encoded_len(u64::MAX), None);
    assert_eq!(media_base64_encoded_len(0), Some(0));
    assert_eq!(media_base64_encoded_len(3), Some(4));
    assert_eq!(media_base64_encoded_len(4), Some(8));
}

/// Verifies A-to-B-to-A upload restoration for every media kind and B capability; returns on success.
/// 对每种媒体类型及 B 能力验证 A 到 B 到 A 的上传恢复；成功时返回。
#[test]
fn uploaded_history_never_reuploads_or_becomes_inline_on_another_model() {
    for kind in [
        ResourceKind::Image,
        ResourceKind::Audio,
        ResourceKind::Video,
        ResourceKind::File,
    ] {
        for b_supported in [false, true] {
            for b_upload in [
                MediaUploadAvailability::Absent,
                MediaUploadAvailability::Available { max_bytes: None },
            ] {
                // A's original identity survives B's temporary attachment-only projection.
                // A 的原始身份不受 B 临时仅附件投影影响。
                let original = MediaDeliveryFacts {
                    kind,
                    origin: MediaDeliveryOrigin::HistoricalUpload {
                        original_target_matches: true,
                        reference_usable: true,
                    },
                    ..video(600_000)
                };
                assert_eq!(
                    decide(
                        MediaDeliveryPolicy::default(),
                        MediaDeliveryPurpose::Conversation,
                        original
                    ),
                    MediaDeliveryDecision::RestoreOriginalUpload
                );
                assert_eq!(
                    decide(
                        MediaDeliveryPolicy::default(),
                        MediaDeliveryPurpose::Conversation,
                        MediaDeliveryFacts {
                            target_supported: b_supported,
                            upload: b_upload,
                            origin: MediaDeliveryOrigin::HistoricalUpload {
                                original_target_matches: false,
                                reference_usable: true
                            },
                            ..original
                        }
                    ),
                    MediaDeliveryDecision::Attachment {
                        reason: MediaDeliveryRejection::HistoricalTargetChanged
                    }
                );
                assert_eq!(
                    decide(
                        MediaDeliveryPolicy::default(),
                        MediaDeliveryPurpose::Conversation,
                        original
                    ),
                    MediaDeliveryDecision::RestoreOriginalUpload
                );
                assert_eq!(
                    decide(
                        MediaDeliveryPolicy::default(),
                        MediaDeliveryPurpose::Conversation,
                        MediaDeliveryFacts {
                            origin: MediaDeliveryOrigin::HistoricalUpload {
                                original_target_matches: true,
                                reference_usable: false
                            },
                            ..original
                        }
                    ),
                    MediaDeliveryDecision::Attachment {
                        reason: MediaDeliveryRejection::OriginalReferenceUnavailable
                    }
                );
            }
        }
    }
}

/// Verifies historical inline ignores new thresholds but obeys current hard bounds; returns on success.
/// 验证历史内联忽略新阈值，但服从当前硬限制；成功时返回。
#[test]
fn historical_inline_survives_upload_first_and_threshold_changes() {
    // All new inline preferences are disabled without erasing historical delivery identity.
    // 禁用全部新内联偏好不会擦除历史投递身份。
    let mut policy = MediaDeliveryPolicy::default();
    policy.conversation.video.without_upload_max_bytes = 0;
    // Even B's real upload contract does not upgrade old inline delivery to an upload.
    // 即使 B 存在真实上传契约，也不会将旧内联投递升级为上传。
    let original = MediaDeliveryFacts {
        origin: MediaDeliveryOrigin::HistoricalInline,
        upload: MediaUploadAvailability::Available { max_bytes: None },
        ..video(600_000)
    };
    assert_eq!(
        decide(policy, MediaDeliveryPurpose::Conversation, original),
        MediaDeliveryDecision::Inline {
            encoded_bytes: 800_000
        }
    );
    assert_eq!(
        decide(
            policy,
            MediaDeliveryPurpose::Conversation,
            MediaDeliveryFacts {
                inline_supported: false,
                ..original
            }
        ),
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::InlineUnsupported
        }
    );
    assert_eq!(
        decide(policy, MediaDeliveryPurpose::Conversation, original),
        MediaDeliveryDecision::Inline {
            encoded_bytes: 800_000
        }
    );
    assert_eq!(
        decide(
            policy,
            MediaDeliveryPurpose::Conversation,
            MediaDeliveryFacts {
                target_max_bytes: Some(100),
                ..original
            }
        ),
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::TargetSizeLimit
        }
    );
    assert_eq!(
        decide(
            policy,
            MediaDeliveryPurpose::Conversation,
            MediaDeliveryFacts {
                source_available: false,
                ..original
            }
        ),
        MediaDeliveryDecision::Attachment {
            reason: MediaDeliveryRejection::SourceUnavailable
        }
    );
    assert!(
        MediaDeliveryPlanner::new(policy, MediaDeliveryPurpose::IndependentExtension)
            .expect("media policy fixture or operation must be valid")
            .plan(original)
            .is_err()
    );
}

/// Verifies missing settings stay absent, explicit values persist, and invalid writes are atomic.
/// 验证缺失设置保持缺失、显式值持久化及无效写入的原子性。
#[test]
fn persisted_policy_is_sparse_validated_and_does_not_rewrite_defaults() {
    // Existing configurations retain the absence of the new optional field.
    // 既有配置保留新可选字段的缺失状态。
    let mut configuration = PersistedAiConfiguration::default();
    assert!(
        serde_json::to_value(&configuration)
            .expect("media policy fixture or operation must be valid")
            .get("media_delivery_policy")
            .is_none()
    );
    assert!(
        !configuration
            .migrate_to_current_schema()
            .expect("media policy fixture or operation must be valid")
    );
    assert_eq!(
        configuration.effective_media_delivery_policy(),
        MediaDeliveryPolicy::default()
    );
    assert!(configuration.media_delivery_policy.is_none());
    // An explicit save preserves zero and every unrelated owner field.
    // 显式保存保留零值及所有无关所有者字段。
    let mut policy = MediaDeliveryPolicy::default();
    policy.independent_extension.audio.with_upload_max_bytes = 1234;
    configuration
        .set_media_delivery_policy(policy)
        .expect("media policy fixture or operation must be valid");
    assert_eq!(
        serde_json::from_value::<PersistedAiConfiguration>(
            serde_json::to_value(&configuration)
                .expect("media policy fixture or operation must be valid")
        )
        .expect("media policy fixture or operation must be valid"),
        configuration
    );
    // Invalid versions and host-sized limits cannot partially mutate the configuration.
    // 无效版本和超过宿主限制的值不能部分修改配置。
    let saved = configuration.clone();
    policy.version += 1;
    assert!(configuration.set_media_delivery_policy(policy).is_err());
    assert_eq!(configuration, saved);
    policy.version = MEDIA_DELIVERY_POLICY_VERSION;
    policy.conversation.image.with_upload_max_bytes = MEDIA_INLINE_RAW_HARD_LIMIT_BYTES + 1;
    assert!(configuration.set_media_delivery_policy(policy).is_err());
    assert_eq!(configuration, saved);
    configuration.media_delivery_policy = Some(policy);
    assert!(configuration.migrate_to_current_schema().is_err());
}

/// Rejects malformed JSON and detects every settings revision; returns normally on success.
/// 拒绝格式错误的 JSON 并检测每项设置修订；成功时正常返回。
#[test]
fn strict_settings_reject_ambiguous_json_and_fingerprint_changes() {
    for invalid in [
        json!(-1),
        json!(1.5),
        json!("512"),
        json!(null),
        json!(u64::MAX),
    ] {
        // Deserialization or validation must reject every invalid threshold representation.
        // 反序列化或校验必须拒绝每种无效阈值表示。
        let mut encoded = serde_json::to_value(MediaDeliveryPolicy::default())
            .expect("media policy fixture or operation must be valid");
        encoded["conversation"]["image"]["with_upload_max_bytes"] = invalid;
        assert!(
            serde_json::from_value::<MediaDeliveryPolicy>(encoded)
                .map_or(true, |policy| policy.validate().is_err())
        );
    }
    // Unknown fields are errors rather than silent spelling corrections.
    // 未知字段属于错误，不能静默修正拼写。
    let mut encoded = serde_json::to_value(MediaDeliveryPolicy::default())
        .expect("media policy fixture or operation must be valid");
    encoded["conversation"]["image"]["with_upload_max_byte"] = json!(1);
    assert!(serde_json::from_value::<MediaDeliveryPolicy>(encoded).is_err());
    // Stable serialization yields stable fingerprints, while separate settings change them.
    // 稳定序列化产生稳定指纹，而独立设置变化会改变指纹。
    let original = MediaDeliveryPolicy::default();
    assert_eq!(
        original
            .fingerprint()
            .expect("media policy fixture or operation must be valid"),
        MediaDeliveryPolicy::default()
            .fingerprint()
            .expect("media policy fixture or operation must be valid")
    );
    for purpose in ["conversation", "independent_extension"] {
        for kind in ["image", "audio", "video", "file"] {
            for mode in ["with_upload_max_bytes", "without_upload_max_bytes"] {
                // Each of the sixteen fields participates in the frozen revision.
                // 十六个字段中的每一个都参与冻结修订。
                let mut changed = serde_json::to_value(original)
                    .expect("media policy fixture or operation must be valid");
                changed[purpose][kind][mode] = json!(777);
                assert_ne!(
                    serde_json::from_value::<MediaDeliveryPolicy>(changed)
                        .expect("media policy fixture or operation must be valid")
                        .fingerprint()
                        .expect("media policy fixture or operation must be valid"),
                    original
                        .fingerprint()
                        .expect("media policy fixture or operation must be valid")
                );
            }
        }
    }
}
