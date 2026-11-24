//! Regression coverage for the regional Step Plan credit contract.
//! 覆盖两个地区的 Step Plan 额度契约回归场景。

use super::decode_stepfun_usage;
use serde_json::{Value, json};

/// Build a subscription snapshot with the requested credits and optional reset.
/// 按指定额度与可选重置时间构建订阅快照。
fn snapshot(total: &str, remaining: &str, reset: Option<u64>) -> Value {
    json!({
        "schema_version": 1, "observed_at_ms": 1234,
        "plan": {"state": "known", "name": "Example Plan"},
        "quota": {"state": "known", "allowed": true, "reason": null, "credit_buckets": [{
            "type": "subscription", "total": total, "remaining": remaining,
            "resets_at_ms": reset, "expires_at_ms": 3000
        }]}
    })
}

/// Both regional reset shapes preserve exact credits and distinguish expiry from reset.
/// 两种地区重置格式均须保留精确额度，并区分到期和重置。
#[test]
fn regional_credits_preserve_precision_and_independent_dates() {
    for reset in [Some(2000_u64), None] {
        // Use an amount that floating-point subtraction would round incorrectly.
        // 选取浮点减法会产生舍入误差的额度。
        let result =
            decode_stepfun_usage(&snapshot("9007199254740993", "2", reset).to_string()).unwrap();
        assert_eq!(result.observed_at.as_deref(), Some("1234"));
        assert_eq!(result.plan_name.as_deref(), Some("Example Plan"));
        // A single grant must produce exactly one visible quota window.
        // 一笔授予记录应当生成一个可见额度窗口。
        let [window] = result.quota_windows.as_slice() else {
            panic!("expected one credit bucket")
        };
        assert_eq!(window.used.as_deref(), Some("9007199254740991"));
        assert_eq!(window.remaining.as_deref(), Some("2"));
        assert_eq!(window.total.as_deref(), Some("9007199254740993"));
        assert_eq!(window.resets_at, reset.map(|ms| ms.to_string()));
        assert_eq!(window.expires_at.as_deref(), Some("3000"));
    }
}

/// Exhaustion and repeated bucket types retain each distinct grant and the upstream reason.
/// 耗尽及重复类型的额度桶均保留各自的授予记录和上游原因。
#[test]
fn exhausted_account_preserves_separate_grants() {
    // Build an exhausted subscription with an explicit blocking reason.
    // 构造已耗尽且有明确阻止原因的订阅。
    let mut body = snapshot("10", "0", None);
    body["quota"]["allowed"] = json!(false);
    body["quota"]["reason"] = json!("exhausted");
    // Duplicate the source type to exercise distinct grant identity.
    // 重复同一额度来源类型以验证授予记录有独立标识。
    let buckets = body["quota"]["credit_buckets"].as_array_mut().unwrap();
    buckets.push(buckets.first().unwrap().clone());
    // Normalize the complete multi-grant response.
    // 规范化包含多个额度桶的完整响应。
    let result = decode_stepfun_usage(&body.to_string()).unwrap();
    // Separate the two returned grants for direct comparison.
    // 分别取得两笔返回的授予记录以供比较。
    let [first, second] = result.quota_windows.as_slice() else {
        panic!("expected both credit buckets")
    };
    assert_ne!(first.id, second.id);
    assert_eq!(first.remaining.as_deref(), Some("0"));
    assert_eq!(result.refresh_message.as_deref(), Some("exhausted"));
}

/// Reject unknown contracts and invalid balances instead of presenting fabricated usage.
/// 遇到未知契约或无效余额时明确报错，避免显示虚构的用量。
#[test]
fn rejects_unknown_or_inconsistent_snapshots() {
    for (pointer, invalid) in [
        ("/schema_version", json!(2)),
        ("/quota/state", json!("unknown")),
        ("/quota/credit_buckets/0/remaining", json!("11")),
        ("/quota/credit_buckets/0/remaining", json!("1.5")),
        ("/quota/credit_buckets/0/total", json!(10)),
    ] {
        // Corrupt just the wire field named by this case.
        // 每个用例只修改其指定的上游字段。
        let mut body = snapshot("10", "1", None);
        *body.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            decode_stepfun_usage(&body.to_string()).is_err(),
            "{pointer}"
        );
    }
}
