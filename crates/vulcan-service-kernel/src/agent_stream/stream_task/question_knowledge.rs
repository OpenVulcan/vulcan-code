//! Current host authorization for the captured child knowledge ceiling.
//! 对捕获的子知识访问上限应用当前宿主授权。

use super::*;
use vulcan_session_core::v1::child_execution::ChildKnowledgeAccess;
use vulcan_session_core::v1::config::KnowledgeRetrievalMode;

/// Resolves `captured` access under the canonical `project_id` using current service authorization.
/// 使用当前服务授权，在规范 `project_id` 下解析 `captured` 访问范围。
/// Returns an explicit context even when empty, preventing ordinary-session inheritance.
/// 即使为空也返回显式上下文，防止继承普通会话设置。
pub(crate) async fn resolve_child_question_knowledge(
    state: &SharedServiceBridgeState,
    project_id: &str,
    captured: Option<&ChildKnowledgeAccess>,
) -> Result<ExecutionKnowledgeAccessContext, String> {
    // Validate project ownership before querying any authorization service.
    // 查询任何授权服务之前校验项目归属。
    let mut access = intersect_child_knowledge(project_id, captured, &BTreeSet::new())?;
    if captured.is_some_and(|ceiling| !ceiling.library_ids.is_empty()) {
        // Clone the host port before awaiting; never hold the service mutex across I/O.
        // 等待之前克隆宿主端口；绝不跨 I/O 持有服务互斥锁。
        let runtime = state
            .lock()
            .map_err(|error| error.to_string())?
            .knowledge_runtime
            .clone();
        if let Some(runtime) = runtime {
            // All current grants are considered, while the original ceiling determines selection.
            // 考察全部当前授权，最终选择由原访问上限限定。
            let authorized = runtime
                .authorized_library_ids(project_id, false)
                .await?
                .into_iter()
                .collect();
            access = intersect_child_knowledge(project_id, captured, &authorized)?;
        }
    }
    access.knowledge_search_handler = Some(build_knowledge_search_handler(state.clone()));
    Ok(access)
}

/// Intersects the original ceiling with `authorized` for exactly `project_id`.
/// 为精确 `project_id` 计算原访问上限与 `authorized` 的交集。
/// Returns a stable ordered context or an ownership error; absent capture grants no library.
/// 返回稳定有序上下文或归属错误；没有捕获记录时不授予任何知识库。
fn intersect_child_knowledge(
    project_id: &str,
    captured: Option<&ChildKnowledgeAccess>,
    authorized: &BTreeSet<String>,
) -> Result<ExecutionKnowledgeAccessContext, String> {
    if captured.is_some_and(|ceiling| ceiling.project_id != project_id) {
        return Err(String::from(
            "Captured child knowledge access belongs to a different project.",
        ));
    }
    // An empty explicit result suppresses workspace defaults in the generic stream launcher.
    // 显式空结果阻止通用流启动器应用工作区默认值。
    let library_ids = captured
        .map(|ceiling| {
            ceiling
                .library_ids
                .intersection(authorized)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    // Preserve the exact policy, including an unspecified mode.
    // 保留精确策略，包括未指定模式。
    let retrieval_mode = captured
        .and_then(|ceiling| ceiling.retrieval_mode.as_ref())
        .map(|mode| {
            match mode {
                KnowledgeRetrievalMode::RawBm25 => "raw_bm25",
                KnowledgeRetrievalMode::DisplayBm25 => "display_bm25",
                KnowledgeRetrievalMode::DisplayHybrid => "display_hybrid",
            }
            .to_owned()
        });
    Ok(ExecutionKnowledgeAccessContext {
        project_id: project_id.to_owned(),
        library_ids,
        retrieval_mode,
        knowledge_search_handler: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves revocation and new grants cannot widen the original child selection.
    /// 证明撤销及新增授权均不会扩大原子代理选择。
    #[test]
    fn child_question_knowledge_intersects_current_grants() {
        // The original child had two libraries, of which one was revoked.
        // 原子代理拥有两个知识库，其中一个已被撤销。
        let captured = ChildKnowledgeAccess {
            project_id: "project".into(),
            library_ids: BTreeSet::from(["retained".into(), "revoked".into()]),
            retrieval_mode: Some(KnowledgeRetrievalMode::DisplayHybrid),
        };
        // A newly granted library belongs only to current defaults, not the captured child.
        // 新授权知识库仅属于当前默认范围，不属于捕获的子代理。
        let authorized = BTreeSet::from(["retained".into(), "new".into()]);
        // The intersection retains original policy and one still-authorized library.
        // 交集保留原策略及仍然授权的一个知识库。
        let access = intersect_child_knowledge("project", Some(&captured), &authorized).unwrap();
        assert_eq!(access.library_ids, vec!["retained"]);
        assert_eq!(access.retrieval_mode.as_deref(), Some("display_hybrid"));
        assert!(intersect_child_knowledge("other", Some(&captured), &authorized).is_err());
        assert!(
            intersect_child_knowledge("project", Some(&captured), &BTreeSet::new())
                .unwrap()
                .library_ids
                .is_empty()
        );
        assert!(
            intersect_child_knowledge("project", None, &authorized)
                .unwrap()
                .library_ids
                .is_empty()
        );
    }
}
