//! Replacement authority and candidate scopes are explicit subsets, never derived from read access.
//! 替代权限和候选作用域必须是显式子集，绝不由读取权限推导。
#![allow(clippy::expect_used)]

use vulcan_memorymesh_core_contracts::{
    MemoryCandidateScope, MemoryReplacementScope, ScopeAccess, ScopeId, SessionSourceId,
};

/// Builds one opaque scope fixture without embedding host hierarchy conventions.
/// 构建一个不嵌入宿主层级约定的不透明作用域用例。
fn scope(value: &str) -> ScopeId {
    ScopeId::new(value).expect("valid fixture scope")
}

/// A readable neighbor can be reviewed without being writable; explicit grants can then enable replacement.
/// 可读邻范围可以参与审核而不可写；显式授权随后可以启用替代。
#[test]
fn replacement_grants_are_explicit_subsets() {
    let access = ScopeAccess {
        write_scope: scope("new"),
        visible_scopes: vec![scope("read"), scope("old"), scope("new")],
    };
    let candidates = MemoryCandidateScope {
        scope_ids: vec![scope("old"), scope("read")],
        origin_session_source_id: Some(SessionSourceId::new("session").expect("session")),
    };
    let mut replacement = MemoryReplacementScope {
        candidates,
        authorized_supersede_scopes: Vec::new(),
    };
    assert!(replacement.validate(&access).is_ok());
    replacement.authorized_supersede_scopes = vec![scope("old")];
    assert!(replacement.validate(&access).is_ok());
    assert_eq!(
        replacement.candidates.scope_ids,
        vec![scope("old"), scope("read")]
    );
    replacement.authorized_supersede_scopes = vec![scope("new")];
    assert!(replacement.validate(&access).is_err());
    replacement.authorized_supersede_scopes = vec![scope("old"), scope("old")];
    assert!(replacement.validate(&access).is_err());
    replacement.authorized_supersede_scopes.clear();
    replacement.candidates.scope_ids.push(scope("foreign"));
    assert!(replacement.validate(&access).is_err());
    replacement.candidates.scope_ids = vec![scope("old"), scope("old")];
    assert!(replacement.validate(&access).is_err());
    replacement.candidates.scope_ids.clear();
    assert!(replacement.validate(&access).is_err());
}

/// Candidate limits are checked exactly without truncating a valid caller scope set.
/// 精确校验候选上限，不截断有效调用方作用域集合。
#[test]
fn replacement_scope_capacity_is_explicit() {
    let ids = (0..256)
        .map(|index| scope(&format!("scope-{index}")))
        .collect::<Vec<_>>();
    let mut access = ScopeAccess {
        write_scope: ids[0].clone(),
        visible_scopes: ids.clone(),
    };
    let mut replacement = MemoryReplacementScope {
        candidates: MemoryCandidateScope {
            scope_ids: ids.clone(),
            origin_session_source_id: None,
        },
        authorized_supersede_scopes: ids,
    };
    assert!(replacement.validate(&access).is_ok());
    access.visible_scopes.push(scope("extra"));
    replacement.candidates.scope_ids.push(scope("extra"));
    assert!(replacement.validate(&access).is_err());
}
