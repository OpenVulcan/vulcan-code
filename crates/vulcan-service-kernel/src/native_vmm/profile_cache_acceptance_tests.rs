//! Real profile preparation, native clear, and delayed host publication acceptance.
//! 真实画像准备、原生清空及迟到宿主发布验收。

use super::*;
use vulcan_agent_runtime::agent_runtime::{
    conversation_hooks::{ConversationProfileOutcome, vmm_profile_sync_effect_from_report},
    vmm_hooks::{VmmHookConfigSnapshot, register_builtin_vmm_hooks},
};
use vulcan_service_api::{ServiceEvent, ServiceEventEmitter, ServiceEventSink, WorkbenchDelta};
use vulcan_workbench_model::{PendingSessionPromptSource, SessionRuntimeStateEntry};

/// Records exact production context events for profile-only consumers.
/// 为仅画像消费方记录准确生产上下文事件。
#[derive(Default)]
struct ProfileEvents {
    /// Ordered real events emitted by the committed-content observer.
    /// 已提交内容观察器发送的有序真实事件。
    events: StdMutex<Vec<ServiceEvent>>,
}

impl ServiceEventSink for ProfileEvents {
    /// Receives one service event and records it synchronously for assertions.
    /// 接收一个服务事件，并同步记录以供断言。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events.lock().expect("profile events").push(event);
    }
}

/// Runs the actual built-in profile request on its ordinary worker and native client.
/// 通过普通工作器及原生客户端运行实际内置画像请求。
/// Takes the installed host hooks, exact host project and session; returns the real dispatch outcome.
/// 接收已安装宿主 Hook、准确宿主项目及会话；返回真实派发结果。
async fn prepare(
    agent: &crate::state::AgentRuntimeBridge,
    host_project: &str,
    session_id: uuid::Uuid,
) -> ConversationProfileOutcome {
    // Release the dispatcher mutex before awaiting its private profile worker.
    // 等待专有画像工作器前释放派发器互斥锁。
    let requests = {
        agent
            .conversation_hook_runtime
            .lock()
            .expect("conversation hooks")
            .prepare_user_prompt_hook_requests(
                session_id,
                "native-profile-content",
                Some(host_project.to_owned()),
                "D:/native-profile-content-fixture".to_owned(),
                PendingSessionPromptSource::GoalContinuation,
                vulcan_hooks::SystemPromptHookPhase::FirstSession,
                "Continue.",
                &Default::default(),
                true,
            )
    };
    requests.profile.run().await
}

/// Verifies immediate cache invalidation and rejection of same-source stale results after actual project clear.
/// 验证实际项目清空后缓存即时失效及同来源过时结果被拒绝。
/// Uses existing nonempty real profiles and isolated host state; proves durable personal nodes remain unchanged.
/// 使用既有非空真实画像及隔离宿主状态；证明持久个人节点保持不变。
pub(super) async fn verify(
    client: &Arc<NativeVmmClient>,
    runtime: &NativeVmmCoreRuntime,
    maintenance: &crate::native_vmm::NativeVmmMaintenance,
    data_plane: &VmmDataPlaneHandle,
    owner_id: u64,
) {
    // Install the production built-in Hook against the already bootstrapped native client.
    // 将生产内置 Hook 安装到已经初始化的原生客户端。
    let agent = crate::tests::test_support::test_agent_runtime_bridge();
    agent
        .vmm_hook_runtime
        .replace_snapshot(VmmHookConfigSnapshot {
            global: vulcan_vmm::VmmConfigLayer {
                enabled: Some(true),
                backend_mode: Some(vulcan_vmm::VmmBackendMode::Native),
                ..Default::default()
            },
            ..Default::default()
        })
        .expect("native profile config");
    agent
        .vmm_hook_runtime
        .install_native_client(client.clone())
        .expect("native profile client");
    register_builtin_vmm_hooks(
        agent
            .conversation_hook_runtime
            .lock()
            .expect("hooks")
            .dispatcher_mut(),
        agent.vmm_hook_runtime.clone(),
    )
    .expect("built-in hooks");
    maintenance.set_native_backend_enabled(true);
    // Prepare a real combined project/user profile, then deliberately delay only its final publication.
    // 准备真实合并项目及用户画像，然后刻意仅延迟其最终发布。
    let session_id = uuid::Uuid::new_v4();
    // Save the real completed report before any maintenance changes its underlying content.
    // 在任何维护改变底层内容前保存真实已完成报告。
    let mut delayed = prepare(&agent, "bundle-owner", session_id).await;
    // Typed Hook metadata provides the authoritative node identities for this prepared baseline.
    // 类型化 Hook 元数据提供此次准备基线的权威节点身份。
    let effect =
        vmm_profile_sync_effect_from_report(delayed.report.as_ref().expect("real profile report"))
            .expect("typed profile")
            .expect("nonempty profile effect");
    assert!(
        effect
            .state
            .nodes_by_id
            .values()
            .any(|node| node.target == ProfileTarget::ProfileTargetProject)
    );
    assert!(
        effect
            .state
            .nodes_by_id
            .values()
            .any(|node| node.target == ProfileTarget::ProfileTargetUser)
    );
    // A second real preparation reads only the global user profile through a different empty project.
    // 第二次真实准备通过另一空项目仅读取全局用户画像。
    let personal_id = uuid::Uuid::new_v4();
    // Prepare the personal-only consumer through the same ordinary worker, not a copied synthetic cache.
    // 通过同一普通工作器准备仅个人消费方，不复制合成缓存。
    let personal = prepare(&agent, "bundle-empty", personal_id).await;
    // This independent result is the complete unchanged-reference cache after project clearing.
    // 此独立结果是项目清空后的完整未变化参考缓存。
    let personal_effect =
        vmm_profile_sync_effect_from_report(personal.report.as_ref().expect("personal report"))
            .expect("typed personal")
            .expect("personal effect");
    assert!(
        personal_effect
            .state
            .nodes_by_id
            .values()
            .all(|node| node.target == ProfileTarget::ProfileTargetUser)
    );
    assert!(!personal_effect.state.nodes_by_id.is_empty());
    // Preserve an actual pre-clear runtime candidate to exercise stale fallback as well as stale successful output.
    // 保留实际清空前运行态候选，同时验证过时复用及过时成功输出。
    let mut detached = SessionRuntimeStateEntry {
        vmm_profile_sync: effect.state.clone(),
        ..Default::default()
    };
    // The isolated service owns retained profile rows without opening any V1 recall document.
    // 隔离服务拥有保留画像行，不打开任何 V1 召回文档。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(session_id);
    service.agent_runtime = Some(agent.clone());
    // This profile-only consumer intentionally has no durable recall controller to open.
    // 此仅画像消费方刻意没有需要打开的持久召回控制器。
    let workspace = service.workspace_snapshot.as_mut().expect("workspace");
    workspace
        .session_runtime_state
        .insert(session_id, detached.clone());
    workspace.session_runtime_state.insert(
        personal_id,
        SessionRuntimeStateEntry {
            vmm_profile_sync: personal_effect.state.clone(),
            ..Default::default()
        },
    );
    service.workbench.selected_conversation_id = Some(session_id.to_string());
    service.workbench.context_injection =
        crate::workbench_session_panels::context_injection_snapshot_from_runtime_state(
            Some(&detached),
            None,
        );
    // Keep the service alive throughout the weak observer's synchronous callback.
    // 在弱引用观察器的同步回调期间保持服务存活。
    let state = Arc::new(StdMutex::new(service));
    // Capture only actual events emitted after the production storage commit.
    // 只捕获生产存储提交后实际发送的事件。
    let events = Arc::new(ProfileEvents::default());
    runtime
        .install_content_projection(&state, &ServiceEventEmitter::new(vec![events.clone()]))
        .expect("install native content observer");
    // Read the real durable personal records before the project-only operation.
    // 在仅项目操作之前读取真实持久个人记录。
    let before = runtime.storage_snapshot().expect("before profile clear");
    // Retain complete personal records to detect metadata changes as well as accidental deletion.
    // 保留完整个人记录，以检测元数据变化及意外删除。
    let personal_nodes = before
        .dataset
        .profile_nodes
        .iter()
        .filter(|node| node.scope_id.as_str() == "vmm-subject:1")
        .cloned()
        .collect::<Vec<_>>();
    // The production clear consumes its actual observed storage version.
    // 生产清空消费其实际观察到的存储版本。
    let preview = maintenance
        .preview_clear(owner_id)
        .await
        .expect("clear preview");
    maintenance
        .execute_clear(
            "profile-cache-project-clear",
            owner_id,
            preview.storage_version,
        )
        .await
        .expect("actual project clear");
    {
        // The commit itself resets the affected cache and leaves an unrelated valid user-only cache intact.
        // 提交本身重置受影响缓存，并保留无关有效的仅用户缓存。
        let guard = state.lock().expect("post-clear service");
        // Read the live retained rows after the synchronous commit callback, not the old detached copy.
        // 在同步提交回调后读取活动保留行，而非旧脱离副本。
        let workspace = guard.workspace_snapshot.as_ref().expect("workspace");
        assert!(
            workspace.session_runtime_state[&session_id]
                .vmm_profile_sync
                .nodes_by_id
                .is_empty()
        );
        assert!(
            workspace.session_runtime_state[&session_id]
                .vmm_profile_sync
                .base_profile_content
                .is_none()
        );
        assert_eq!(
            workspace.session_runtime_state[&personal_id].vmm_profile_sync,
            personal_effect.state
        );
        assert!(
            guard
                .workbench
                .context_injection
                .as_ref()
                .expect("selected profile panel")
                .static_profile
                .is_none()
        );
        assert!(
            guard
                .vmm_controller_registry
                .snapshots()
                .expect("no cold hydration")
                .is_empty()
        );
    }
    // Exactly the affected profile-only session emits one production context delta.
    // 只有受影响的仅画像会话发送一个生产上下文增量。
    {
        let captured = events.events.lock().expect("context events");
        assert_eq!(captured.len(), 1);
        // Event type and exact session ownership are asserted rather than inferred from event counts alone.
        // 断言事件类型及准确会话归属，而非仅从事件数量推断。
        let ServiceEvent::WorkbenchDelta(envelope) = &captured[0] else {
            panic!("workbench delta")
        };
        // Only a context projection delta satisfies the profile-only notification contract.
        // 只有上下文投影增量满足仅画像通知契约。
        let WorkbenchDelta::ContextInjectionChanged(delta) = &envelope.delta else {
            panic!("context delta")
        };
        assert_eq!(delta.conversation_id, session_id.to_string());
    }
    // The actual native binding still has the same source identity despite the content change.
    // 尽管内容改变，实际原生绑定仍具有相同来源身份。
    let publication = maintenance
        .prepare_recall_publication(Some("bundle-owner"), &session_id.to_string())
        .await
        .expect("profile publication barrier");
    assert_eq!(
        effect.state.read_source,
        Some(publication.profile_read_source().expect("unchanged source"))
    );
    assert!(
        publication
            .reconcile_profile(&mut detached, &mut delayed)
            .expect("reject stale native content")
    );
    assert!(detached.vmm_profile_sync.base_profile_content.is_none());
    assert!(
        vmm_profile_sync_effect_from_report(delayed.report.as_ref().expect("filtered report"))
            .expect("typed filtered report")
            .is_none()
    );
    assert!(
        !delayed
            .report
            .as_ref()
            .expect("filtered report")
            .system_prompt_blocks
            .iter()
            .any(|block| block.tag == "vmm_profile")
    );
    // The barrier also excludes the actual commit mutex, independently of request-side operation guards.
    // 屏障也排除实际提交互斥锁，独立于请求侧操作守卫。
    let competing = runtime.freeze_committed_content();
    tokio::pin!(competing);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut competing)
            .await
            .is_err()
    );
    drop(publication);
    drop(
        tokio::time::timeout(std::time::Duration::from_secs(5), competing)
            .await
            .expect("commit gate released"),
    );
    // Ordinary preparation now rebuilds a valid user-only baseline from preserved durable data.
    // 普通准备现在从保留的持久数据重建有效的仅用户基线。
    let mut fresh = prepare(&agent, "bundle-owner", session_id).await;
    // Inspect the actual rebuilt effect, including the regenerated personal baseline text.
    // 检查实际重建效果，包括重新生成的个人基线正文。
    let fresh_effect =
        vmm_profile_sync_effect_from_report(fresh.report.as_ref().expect("fresh report"))
            .expect("fresh typed")
            .expect("fresh user baseline");
    assert!(!fresh_effect.state.nodes_by_id.is_empty());
    assert!(
        fresh_effect
            .state
            .nodes_by_id
            .values()
            .all(|node| node.target == ProfileTarget::ProfileTargetUser)
    );
    assert!(
        fresh_effect
            .state
            .base_profile_content
            .as_ref()
            .expect("fresh baseline")
            .contains("西瓜和葡萄")
    );
    // A fresh successful result must not be overwritten in the Hook mirror when only a detached fallback was stale.
    // 当只有脱离复用候选过时时，不能覆盖 Hook 镜像中的新鲜成功结果。
    let mut stale_fallback = SessionRuntimeStateEntry {
        vmm_profile_sync: effect.state,
        ..Default::default()
    };
    // Freeze current storage again before comparing a fresh result with an old fallback candidate.
    // 比较新鲜结果与旧复用候选前，再次冻结当前存储。
    let fresh_publication = maintenance
        .prepare_recall_publication(Some("bundle-owner"), &session_id.to_string())
        .await
        .expect("fresh publication");
    assert!(
        !fresh_publication
            .reconcile_profile(&mut stale_fallback, &mut fresh)
            .expect("keep fresh Hook mirror")
    );
    assert!(
        stale_fallback
            .vmm_profile_sync
            .base_profile_content
            .is_none()
    );
    assert_eq!(
        vmm_profile_sync_effect_from_report(fresh.report.as_ref().expect("preserved fresh report"))
            .expect("typed fresh")
            .expect("fresh effect")
            .state,
        fresh_effect.state
    );
    drop(fresh_publication);
    // Re-read actual SQLite relations to prove profile preservation independently of process caches.
    // 重新读取实际 SQLite 关系，独立于进程缓存证明画像保留。
    let durable: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("durable read")
            .expect("durable snapshot"),
    )
    .expect("typed durable");
    assert_eq!(
        durable
            .dataset
            .profile_nodes
            .iter()
            .filter(|node| node.scope_id.as_str() == "vmm-subject:1")
            .cloned()
            .collect::<Vec<_>>(),
        personal_nodes
    );
    assert!(
        !durable
            .dataset
            .profile_nodes
            .iter()
            .any(|node| node.scope_id == project_scope_id(owner_id).expect("owner scope"))
    );
}
