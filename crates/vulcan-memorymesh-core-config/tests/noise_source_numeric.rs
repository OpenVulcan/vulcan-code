//! Compares Noise scoring with bit-exact observations from the original VMM processor.
//! 对照原始VMM处理器的精确位模式观察验证噪声评分。
#![allow(clippy::expect_used)]

use vulcan_memorymesh_core_config::{
    ConfigBundle, ConfigRegistry, NoisePrototypeRow, ResourceLayer,
};
use vulcan_memorymesh_core_contracts::Role;

/// Checks a real VMM normalized vector, an inclusive threshold, and its next f64 value.
/// 检查真实VMM归一化向量、包含边界的阈值及其下一个双精度值；无参数，断言报告错误。
#[test]
fn source_numeric_threshold_and_audit_are_exact() {
    let vector = vec![f32::from_bits(0x3f13_cd3a); 3];
    let expected = f64::from_bits(0x3fef_ffff_e000_0000);
    for threshold in [expected, f64::from_bits(expected.to_bits() + 1)] {
        let mut layer = ResourceLayer::default();
        layer.insert("noise_rules/common.json", format!(r#"{{"language":"common","version":"1.0.0","categories":[{{"name":"numeric","targets":["user"],"threshold":{threshold:?},"phrases":["prototype"]}}]}}"#).into_bytes()).expect("resource");
        layer
            .insert(
                "noise_rules/zh-CN.json",
                br#"{"language":"zh-CN","version":"1.0.0","categories":[]}"#.to_vec(),
            )
            .expect("resource");
        let snapshot = ConfigRegistry::new()
            .expect("registry")
            .register(ConfigBundle {
                user_layer: layer,
                ..ConfigBundle::default()
            })
            .expect("snapshot");
        let rows = [NoisePrototypeRow {
            category: "numeric".into(),
            phrase: "prototype".into(),
            vector: vector.clone(),
            threshold: 0.0,
        }];
        let decision = snapshot
            .noise_detector
            .semantic_decision(Role::User, &vector, &rows);
        assert_eq!(
            decision.confidence.to_bits(),
            expected.to_bits(),
            "source audit score"
        );
        assert_eq!(
            decision.is_noise,
            expected >= threshold,
            "source threshold precision"
        );
    }
}

/// Category overrides retain VMM's nonpositive default sentinel and positive values above one.
/// 类别覆盖保留VMM非正数取默认值及大于一正数的行为；全局配置范围不变。
#[test]
fn source_category_threshold_overrides_preserve_range() {
    for threshold in [-1.0, 0.0, 1.000_000_02, 1.1] {
        let mut layer = ResourceLayer::default();
        layer.insert("noise_rules/common.json", format!(r#"{{"language":"common","version":"1.0.0","categories":[{{"name":"numeric","threshold":{threshold:?},"phrases":["prototype"]}}]}}"#).into_bytes()).expect("resource");
        layer
            .insert(
                "noise_rules/zh-CN.json",
                br#"{"language":"zh-CN","version":"1.0.0","categories":[]}"#.to_vec(),
            )
            .expect("resource");
        let snapshot = ConfigRegistry::new()
            .expect("registry")
            .register(ConfigBundle {
                user_layer: layer,
                ..ConfigBundle::default()
            })
            .expect("source category override");
        let expected: f64 = if threshold > 0.0 { threshold } else { 0.88 };
        assert_eq!(
            snapshot.noise_detector.semantic_phrase_targets(Role::User)[0]
                .threshold
                .to_bits(),
            expected.to_bits()
        );
    }
}
