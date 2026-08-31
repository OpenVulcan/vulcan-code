//! Regression coverage for registered template forms and independent resource bounds.
//! 已注册模板形式与独立资源上限的回归覆盖。

use super::*;
use serde_json::json;

/// Verifies the closed template language renders every registered reusable form together.
/// 验证封闭模板语言可以组合渲染所有已注册的可复用形式。
#[test]
fn bounded_json_template_renders_nested_registered_forms() {
    // The source exercises body references, optional omission, array item scope, and enum lookup.
    // 来源同时覆盖正文引用、可选省略、数组项作用域与枚举查找。
    let source = json!({
        "prompt": "draw",
        "format": "jpg",
        "enabled": true,
        "rows": [
            {"name": "first", "url": "https://example.com/first"},
            {"name": "second", "url": "https://example.com/second"}
        ]
    });
    // The template combines model identity, map, enum, condition, concat, and optional reference forms.
    // 模板组合模型标识、数组映射、枚举、条件、拼接与可选引用形式。
    let template = json!({
        "model": {"$vulcan_model_id": true},
        "missing": {"$vulcan_ref": {"path": "/missing", "optional": true}},
        "mime": {
            "$vulcan_enum": {
                "path": "/format",
                "mapping": {"jpg": "image/jpeg", "png": "image/png"}
            }
        },
        "mode": {
            "$vulcan_if_equals": {
                "path": "/enabled",
                "equals": true,
                "then": "active",
                "otherwise": "inactive"
            }
        },
        "label": {
            "$vulcan_concat": [
                "prefix:",
                {"$vulcan_ref": {"path": "/prompt"}}
            ]
        },
        "fallback": {
            "$vulcan_coalesce": [
                {"$vulcan_ref": {"path": "/missing", "optional": true}},
                {"$vulcan_ref": {"path": "/prompt"}}
            ]
        },
        "present_only": {
            "$vulcan_if_present": {
                "path": "/prompt",
                "then": {"value": {"$vulcan_ref": {"path": "/prompt"}}}
            }
        },
        "absent_only": {
            "$vulcan_if_present": {
                "path": "/missing",
                "then": "must-not-render"
            }
        },
        "items": {
            "$vulcan_map": {
                "path": "/rows",
                "template": {
                    "title": {"$vulcan_ref": {"scope": "item", "path": "/name"}},
                    "url": {"$vulcan_ref": {"scope": "item", "path": "/url"}}
                }
            }
        },
        "combined_items": {
            "$vulcan_array_concat": [
                {"$vulcan_map": {"path": "/rows", "template": {"$vulcan_ref": {"scope": "item", "path": "/name"}}}},
                ["last"]
            ]
        }
    });

    // The rendered value is the exact provider-neutral wire shape.
    // 渲染值是精确的供应商无关 Wire 结构。
    let rendered = render_complete_json_template(&template, &source, "image-model")
        .expect("registered template forms should render");

    assert_eq!(
        rendered,
        json!({
            "model": "image-model",
            "mime": "image/jpeg",
            "mode": "active",
            "label": "prefix:draw",
            "fallback": "draw",
            "present_only": {"value": "draw"},
            "items": [
                {"title": "first", "url": "https://example.com/first"},
                {"title": "second", "url": "https://example.com/second"}
            ],
            "combined_items": ["first", "second", "last"]
        })
    );
}

/// Verifies template rendering fails closed on ambiguous reserved forms and excessive arrays.
/// 验证模板渲染会对含糊保留形式与超限数组保持失败关闭。
#[test]
fn bounded_json_template_rejects_ambiguous_forms_and_array_overflow() {
    // The ambiguous object mixes a registered form with an ordinary property.
    // 含糊对象把已注册形式与普通属性混在一起。
    let ambiguous = json!({
        "$vulcan_ref": {"path": "/value"},
        "extra": true
    });
    // The ambiguity error proves manifests cannot smuggle unvalidated sibling behavior.
    // 含糊错误证明 Manifest 无法夹带未经校验的同级行为。
    let ambiguity_error =
        render_complete_json_template(&ambiguous, &json!({"value": "ok"}), "model")
            .expect_err("ambiguous reserved form must fail");
    assert!(
        ambiguity_error.contains("only property"),
        "{ambiguity_error}"
    );

    // The source exceeds the fixed collection ceiling by exactly one item.
    // 来源数组恰好比固定集合上限多一个元素。
    let oversized_source = Value::Array(vec![Value::Null; JSON_TEMPLATE_MAX_ARRAY_ITEMS + 1]);
    // The map template is intentionally minimal so only the collection ceiling is exercised.
    // 映射模板刻意保持最小，以便只验证集合上限。
    let map_template = json!({
        "$vulcan_map": {
            "path": "",
            "template": {"$vulcan_ref": {"scope": "item", "path": ""}}
        }
    });
    // The overflow error is deterministic and occurs before allocating provider output.
    // 超限错误是确定性的，并在分配供应商输出前发生。
    let overflow_error = render_complete_json_template(&map_template, &oversized_source, "model")
        .expect_err("oversized source array must fail");
    assert!(overflow_error.contains("item limit"), "{overflow_error}");
}

/// Verifies template recursion and serialized output are bounded independently.
/// 验证模板递归深度与序列化输出大小分别受到硬限制。
#[test]
fn bounded_json_template_rejects_depth_and_output_overflow() {
    // The nested template starts from one scalar leaf.
    // 嵌套模板从一个标量叶节点开始。
    let mut deep_template = Value::Null;
    for _ in 0..=crate::provider_manifest::MAX_JSON_DEPTH {
        deep_template = Value::Array(vec![deep_template]);
    }
    // The depth error proves rendering cannot recurse beyond the manifest engine ceiling.
    // 深度错误证明渲染不会递归超过 Manifest 引擎上限。
    let depth_error = render_complete_json_template(&deep_template, &Value::Null, "model")
        .expect_err("deep template must fail");
    assert!(depth_error.contains("depth limit"), "{depth_error}");

    // The source string exceeds the final serialized-output ceiling after JSON encoding.
    // 来源字符串在 JSON 编码后超过最终序列化输出上限。
    let oversized_text = "x".repeat(JSON_TEMPLATE_MAX_OUTPUT_BYTES + 1);
    // The direct reference isolates the output-byte guard from collection and depth guards.
    // 直接引用将输出字节守卫与集合、深度守卫隔离。
    let output_template = json!({"$vulcan_ref": {"path": "/text"}});
    // The output error proves a small template cannot amplify an unbounded source value.
    // 输出错误证明小模板不能放大无界来源值。
    let output_error =
        render_complete_json_template(&output_template, &json!({"text": oversized_text}), "model")
            .expect_err("oversized rendered output must fail");
    assert!(output_error.contains("byte limit"), "{output_error}");
}
