//! Public domain resource-bound contract tests.
//! 公共领域资源边界契约测试。

use vulcan_memorymesh_core_contracts::{MAX_VISIBLE_SCOPES, ScopeAccess, ScopeId};

/// Verifies one command cannot carry an unbounded visible-scope precedence list.
/// 验证一个命令不能携带无界的可见作用域优先级列表。
#[test]
fn scope_access_rejects_oversized_visible_scope_lists() {
    let write_scope = ScopeId::new("scope-0").expect("write scope must be valid");
    let visible_scopes = (0..=MAX_VISIBLE_SCOPES)
        .map(|index| {
            ScopeId::new(format!("scope-{index}"))
                .expect("generated visible scope identity must be valid")
        })
        .collect();
    let error = ScopeAccess {
        write_scope,
        visible_scopes,
    }
    .validate()
    .expect_err("oversized visible scopes must fail");
    assert_eq!(error, "visible scopes exceed the supported limit");
}
