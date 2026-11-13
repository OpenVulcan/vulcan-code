//! Shared service-collaboration fixtures for port-level tests.
//! 端口级测试共享的服务协作夹具。

use super::*;
use crate::tests::test_support::{TestHostRuntimeRoot, test_service_state_with_parent_session};
use vulcan_session_core::v1::config::MetadataDocument;
use vulcan_session_core::v1::context::{RuntimeInitializationState, RuntimeLock};
use vulcan_workspace_store::workspace_store::{CreateSessionV1Request, create_indexed_session_v1};

/// Creates two indexed sessions in one isolated database; `cross_project` selects target scope.
/// 在隔离数据库创建两个索引会话；`cross_project` 决定目标项目范围。
/// Returns the storage owner, a service invocation fixture, and the canonical target handle.
/// 返回存储所有者、服务调用夹具和规范目标句柄。
pub(super) async fn fixture(
    cross_project: bool,
) -> (TestHostRuntimeRoot, ValidatedInvocation, Arc<SessionHandle>) {
    // All persistent files are owned by this fixture, outside real application roots.
    // 所有持久文件归本夹具持有，位于真实应用数据根之外。
    let source_id = Uuid::new_v4();
    let root = TestHostRuntimeRoot::new(source_id);
    let controller = root.attached_host_runtime_handle().await;
    let source_locator = root.v1_storage_locator();
    let source = SessionHandle::new(
        source_id,
        source_locator.clone(),
        load_session_v1_for_runtime(&source_locator)
            .unwrap()
            .document,
    );
    let target_id = Uuid::new_v4();
    let project_id = if cross_project {
        "other-project"
    } else {
        source_locator.project_id()
    };
    let metadata = MetadataDocument::new(target_id.to_string(), project_id, "Wait target", 1);
    let runtime_lock = RuntimeLock {
        initialization_state: RuntimeInitializationState::FreshUninitialized,
        current_compaction_id: None,
        locked_sources: vec![],
    };
    create_indexed_session_v1(
        &controller,
        &CreateSessionV1Request {
            cleaned_history: None,
            project_id,
            storage_partition:
                vulcan_workspace_store::workspace_store::SESSION_V1_STORAGE_PARTITION,
            date_bucket: "1970-01-01",
            metadata: &metadata,
            runtime_lock: &runtime_lock,
            compactions: &[],
            events: &[],
        },
    )
    .await
    .unwrap();
    let locator = locate_optional_session_v1_storage(&controller, &target_id.to_string())
        .await
        .unwrap()
        .unwrap();
    let target = SessionHandle::new(
        target_id,
        locator.clone(),
        load_session_v1_for_runtime(&locator).unwrap().document,
    );
    let mut service = test_service_state_with_parent_session(source_id);
    service.controller_runtime = Some(controller.clone());
    let registry = service.session_runtime_registry.clone();
    registry.register(source.clone()).unwrap();
    registry.register(target.clone()).unwrap();
    let invocation = ValidatedInvocation {
        state: Arc::new(Mutex::new(service)),
        controller_runtime: controller,
        registry,
        source_session_id: source_id,
        source_handle: source,
        source_locator,
        source_turn_id: 1,
        source_api_call_id: None,
        source_tool_id: WAIT_SESSION_TOOL_ID.into(),
        operation_id: "wait-operation".into(),
    };
    (root, invocation, target)
}
