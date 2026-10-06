//! Admission generations owned by the existing atomic VMM hook snapshot.
//! 由既有原子 VMM Hook 快照拥有的准入代次。

use super::*;
use vulcan_vmm::{VmmMemorySource, VmmWritebackPermit};

/// One host session's exact source and revocable delivery generation.
/// 一个宿主会话的准确来源及可撤销投递代次。
#[derive(Clone, Debug)]
struct WritebackAdmission {
    /// Stable calling host project, including the explicitly empty projectless scope.
    /// 稳定调用方宿主项目，包含明确为空的无项目作用域。
    host_project_id: String,
    /// Authoritative optional workspace used to resolve project configuration.
    /// 用于解析项目配置的权威可选工作区。
    workspace_path: Option<String>,
    /// Exact owner frozen before dispatch.
    /// 派发前冻结的准确归属。
    source: VmmMemorySource,
    /// Shared revocation authority, never reconstructed from mirrored queue data.
    /// 共享撤销权限，绝不从镜像队列数据重建。
    permit: VmmWritebackPermit,
}

/// Non-serializable live admission registry preserved across configuration snapshot replacement.
/// 配置快照替换时保留的不可序列化活动准入登记表。
#[derive(Clone, Debug, Default)]
pub struct VmmWritebackAdmissions {
    /// Exact session-keyed current generations; revoked generations remain distinguishable until replaced.
    /// 按准确会话键索引的当前代次；撤销代次在替换前始终可区分。
    sessions: BTreeMap<String, WritebackAdmission>,
}

impl VmmWritebackAdmissions {
    /// Removes one terminated session and revokes every outstanding clone before releasing its registry entry.
    /// 移除已终止会话，并在释放登记项之前撤销全部尚存克隆。
    pub(super) fn remove_session(&mut self, session_id: &str) {
        if let Some(admission) = self.sessions.remove(session_id) {
            admission.permit.revoke();
        }
    }
}

/// Resolves only write ownership, so changing timeouts, labels or native defaults does not cancel existing bindings.
/// 仅解析写入归属，修改超时、标签或原生默认规则不取消既有绑定。
fn configured_source(
    snapshot: &VmmHookConfigSnapshot,
    session_id: &str,
    workspace_path: Option<&str>,
    native_source: &VmmMemorySource,
) -> Result<Option<VmmMemorySource>, String> {
    // Project and session overrides follow the same declared precedence as actual hook dispatch.
    // 项目及会话覆盖遵循与实际 Hook 派发相同的声明优先级。
    let config = VmmEffectiveConfig::resolve(
        &snapshot.global,
        workspace_path.and_then(|path| snapshot.projects_by_workspace.get(path)),
        snapshot.sessions.get(session_id),
    );
    match config.backend_mode {
        VmmBackendMode::Native => Ok(match native_source {
            VmmMemorySource::Native { .. } => Some(native_source.clone()),
            VmmMemorySource::ExternalGrpc { .. } => None,
        }),
        VmmBackendMode::ExternalGrpc => {
            // Incomplete external identity cannot authorize replay of an earlier configured source.
            // 不完整外置身份不能授权重放此前已配置来源。
            let (Some(user_id), Some(project_id)) = (config.user_id, config.project_id) else {
                return Ok(None);
            };
            VmmMemorySource::external(
                &VmmTransportConfig::from_effective(&config),
                user_id,
                project_id,
            )
            .map(Some)
            .map_err(|error| error.to_string())
        }
    }
}

/// Revokes changed owners and prunes stale queue projections under the configuration write lock.
/// 在配置写锁内撤销已变化归属，并清理旧队列投影。
pub(super) fn reconcile_writeback_admissions(snapshot: &mut VmmHookConfigSnapshot) {
    // Invalid current configuration is not a fallback route; it removes permission to replay old work.
    // 无效当前配置不是回退路由；它撤销重放旧工作的权限。
    for (session_id, admission) in &snapshot.writeback_admissions.sessions {
        if !matches!(configured_source(snapshot, session_id, admission.workspace_path.as_deref(), &admission.source), Ok(Some(source)) if source == admission.source)
        {
            admission.permit.revoke();
        }
    }
    snapshot.outboxes.retain(|session_id, entries| {
        entries.retain(|entry| {
            snapshot
                .writeback_admissions
                .sessions
                .get(session_id)
                .is_some_and(|admission| {
                    admission.permit.ensure_active().is_ok()
                        && admission.permit.id() == entry.permit_id
                        && entry.request.expected_source.as_ref() == Some(&admission.source)
                })
        });
        !entries.is_empty()
    });
}

impl VmmHookRuntimeHandle {
    /// Filters a process-local UI mirror against the existing admission owner without creating permissions.
    /// 根据既有准入归属方过滤进程内界面镜像，不创建权限。
    /// Takes exact session and mirrored entries; returns only live same-generation, same-source pending work.
    /// 接收准确会话及镜像条目；仅返回活动且同代次、同来源的待处理工作。
    pub fn current_writeback_outbox(
        &self,
        session_id: SessionId,
        entries: &[VmmWritebackOutboxEntry],
    ) -> Result<Vec<VmmWritebackOutboxEntry>, String> {
        let state = self.read_state("filter VMM writeback mirror")?;
        let admission = state
            .writeback_admissions
            .sessions
            .get(&session_id.to_string());
        Ok(entries
            .iter()
            .filter(|entry| {
                admission.is_some_and(|admission| {
                    admission.permit.id() == entry.permit_id
                        && admission.permit.ensure_active().is_ok()
                        && entry.request.expected_source.as_ref() == Some(&admission.source)
                })
            })
            .cloned()
            .collect())
    }

    /// Admits a resolved write source under the same lock as configuration and pending-queue projection.
    /// 在与配置及待处理队列投影相同的锁内准入已解析写入来源。
    /// Takes exact invocation ownership and source; returns its current permit or explicit changed-configuration rejection.
    /// 接收准确调用归属及来源；返回当前许可或明确配置变化拒绝。
    pub fn admit_writeback(
        &self,
        invocation: &HookInvocation,
        source: &VmmMemorySource,
    ) -> Result<VmmWritebackPermit, String> {
        // A completed-turn request already has canonical session and optional project/workspace payloads.
        // 完成回合请求已经具有规范会话以及可选项目/工作区载荷。
        let session_id = invocation_session_id(invocation)
            .ok_or_else(|| "VMM writeback admission requires a session ID.".to_owned())?;
        let host_project_id =
            payload_string(&invocation.payload.value, "project_id").unwrap_or_default();
        let workspace_path = payload_string(&invocation.payload.value, "workspace_path");
        let mut state = self.write_state("admit one VMM writeback generation")?;
        // Resolve readiness again under the admission lock after asynchronous source lookup.
        // 异步查询来源后，在准入锁内重新解析就绪状态。
        let config = VmmEffectiveConfig::resolve(
            &state.global,
            workspace_path
                .as_deref()
                .and_then(|path| state.projects_by_workspace.get(path)),
            state.sessions.get(&session_id),
        );
        // Local enable overrides cannot admit new writes after the application service has stopped.
        // 应用服务停止后，本地启用覆盖不能准入新写入。
        if !VmmEffectiveConfig::resolve(&state.global, None, None).enabled
            || config.diagnose_writeback_flow(true) != VmmConfigDiagnostic::Ready
        {
            return Err("VMM writeback configuration is no longer ready.".to_owned());
        }
        if configured_source(&state, &session_id, workspace_path.as_deref(), source)?.as_ref()
            != Some(source)
        {
            return Err("VMM writeback source changed during preparation.".to_owned());
        }
        if let Some(existing) = state.writeback_admissions.sessions.get(&session_id) {
            if existing.source == *source
                && existing.host_project_id == host_project_id
                && existing.workspace_path == workspace_path
                && existing.permit.ensure_active().is_ok()
            {
                return Ok(existing.permit.clone());
            }
            existing.permit.revoke();
        }
        // A fresh generation never adopts a revoked generation's pending entries, even at the same destination.
        // 新代次绝不接管撤销代次的待处理条目，即使目的地相同。
        let permit = VmmWritebackPermit::new().map_err(|error| error.to_string())?;
        state.writeback_admissions.sessions.insert(
            session_id,
            WritebackAdmission {
                host_project_id,
                workspace_path,
                source: source.clone(),
                permit: permit.clone(),
            },
        );
        Ok(permit)
    }

    /// Revokes all native writeback generations owned by one host project after a successful owner change.
    /// 成功改变写入归属后，撤销一个宿主项目拥有的全部原生写回代次。
    /// Takes a stable host project ID; does not touch accepted VMM records or another project's generations.
    /// 接收稳定宿主项目 ID；不触碰已接受 VMM 记录或其他项目代次。
    pub fn revoke_native_project_writebacks(
        &self,
        host_project_id: &str,
        previous_project_id: u64,
    ) -> Result<(), String> {
        let mut state = self.write_state("revoke native project writebacks")?;
        for admission in state.writeback_admissions.sessions.values() {
            if admission.host_project_id == host_project_id
                && matches!(admission.source, VmmMemorySource::Native { project_id } if project_id == previous_project_id)
            {
                admission.permit.revoke();
            }
        }
        reconcile_writeback_admissions(&mut state);
        Ok(())
    }

    /// Filters delayed effects and holds the admission read lock throughout synchronous publication.
    /// 过滤迟到效果，并在同步发布全过程持有准入读锁。
    /// Callers acquire their outer service lock first and must not refresh this hook snapshot inside the callback.
    /// 调用方先取得外层服务锁，且不得在回调内部刷新本 Hook 快照。
    pub fn with_current_writeback_effects<R>(
        &self,
        session_id: SessionId,
        effects: Vec<VmmWritebackHookEffect>,
        publish: impl FnOnce(Vec<VmmWritebackHookEffect>) -> R,
    ) -> Result<R, String> {
        let state = self.read_state("publish VMM writeback effects")?;
        let current = state
            .writeback_admissions
            .sessions
            .get(&session_id.to_string());
        let accepted = effects
            .into_iter()
            .filter(|effect| match effect {
                VmmWritebackHookEffect::Enqueue {
                    permit_id, request, ..
                } => current.is_some_and(|admission| {
                    admission.permit.id() == *permit_id
                        && admission.permit.ensure_active().is_ok()
                        && request.expected_source.as_ref() == Some(&admission.source)
                }),
                VmmWritebackHookEffect::SessionStatus { permit_id, .. } => {
                    current.is_some_and(|admission| {
                        admission.permit.id() == *permit_id
                            && admission.permit.ensure_active().is_ok()
                    })
                }
                // Exact entry IDs include their generation and cannot remove or update a newer generation's work.
                // 准确条目 ID 包含其代次，不会移除或更新较新代次的工作。
                VmmWritebackHookEffect::Cancelled { .. }
                | VmmWritebackHookEffect::Flushed { .. }
                | VmmWritebackHookEffect::Failed { .. } => true,
            })
            .collect();
        Ok(publish(accepted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_hooks::{HookContext, HookPayload};

    /// Builds an exact completed-turn invocation for one host-owned session.
    /// 为一个宿主拥有的会话构建准确完成回合调用。
    fn invocation(session: SessionId, host: &str) -> HookInvocation {
        HookInvocation {
            registration: vmm_hook_registration(
                VMM_POST_ACTION_HOOK_ID,
                HOOK_CONVERSATION_TURN_COMPLETED,
                HookMode::Effect,
            ),
            context: HookContext::new("writeback-admission-test", HOOK_CONVERSATION_TURN_COMPLETED),
            payload: HookPayload {
                value: json!({ "session_id": session.to_string(), "project_id": host, "workspace_path": format!("D:/{host}") }),
            },
        }
    }

    /// Creates a ready synthetic external configuration without contacting the endpoint.
    /// 创建就绪的合成外置配置，不联系端点。
    fn external(target: &str) -> VmmConfigLayer {
        VmmConfigLayer {
            backend_mode: Some(VmmBackendMode::ExternalGrpc),
            enabled: Some(true),
            grpc_target: Some(target.to_owned()),
            user_id: Some(1),
            project_id: Some(7),
            ..Default::default()
        }
    }

    /// Uses the same external identity constructor as actual dispatch.
    /// 使用与实际派发相同的外置身份构造器。
    fn source(layer: &VmmConfigLayer) -> VmmMemorySource {
        VmmMemorySource::external(
            &VmmTransportConfig::from_effective(&VmmEffectiveConfig::resolve(layer, None, None)),
            1,
            7,
        )
        .expect("source")
    }

    /// Freezes one process-local queue entry for a real admitted generation.
    /// 为真实准入代次冻结一条进程内队列项。
    fn queued(permit: &VmmWritebackPermit, source: &VmmMemorySource) -> VmmWritebackOutboxEntry {
        VmmWritebackOutboxEntry {
            permit_id: permit.id(),
            id: format!("old-{}", permit.id()),
            request: vulcan_vmm::PostActionRequest {
                expected_source: Some(source.clone()),
                ..Default::default()
            },
            attempt_count: 1,
            last_error: Some("offline".to_owned()),
            created_at: "1".to_owned(),
            updated_at: "1".to_owned(),
        }
    }

    /// A configured A-to-B-to-A switch cancels old work even with no intervening completed turn.
    /// 即使中间没有完成回合，配置 A 到 B 再到 A 也会取消旧工作。
    #[test]
    fn source_roundtrip_rejects_old_mirrors_and_delayed_effects() {
        let session = SessionId::from_u128(71);
        let layer = external("127.0.0.1:50051");
        let source = source(&layer);
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
            global: layer.clone(),
            ..Default::default()
        });
        let invocation = invocation(session, "host-a");
        let old = runtime
            .admit_writeback(&invocation, &source)
            .expect("old admission");
        let entry = queued(&old, &source);
        runtime
            .replace_session_projection(
                session,
                &Default::default(),
                std::slice::from_ref(&entry),
                None,
            )
            .expect("mirror");
        // Timeout tuning is not a destination change and must preserve legitimate pending work.
        // 调整超时不是目的地变化，必须保留合法待处理工作。
        let mut tuned = layer.clone();
        tuned.handshake_timeout_ms = Some(45_000);
        runtime
            .replace_global_projection(&tuned)
            .expect("tune timeout");
        assert!(old.ensure_active().is_ok());
        assert_eq!(
            runtime
                .outbox_for_invocation(&invocation)
                .expect("queue")
                .len(),
            1
        );
        runtime
            .replace_global_projection(&external("127.0.0.1:50052"))
            .expect("switch away");
        runtime
            .replace_global_projection(&layer)
            .expect("switch back");
        assert!(old.ensure_active().is_err());
        assert!(
            runtime
                .outbox_for_invocation(&invocation)
                .expect("cancelled queue")
                .is_empty()
        );
        // Replacing the entire mirror cannot recreate a revoked permit or refill its old queue.
        // 替换整个镜像不能重新创建撤销许可，也不能填回其旧队列。
        let mut stale = VmmHookConfigSnapshot {
            global: layer,
            ..Default::default()
        };
        stale
            .outboxes
            .insert(session.to_string(), vec![entry.clone()]);
        runtime
            .replace_snapshot(stale)
            .expect("replace stale mirror");
        let current = runtime
            .admit_writeback(&invocation, &source)
            .expect("new admission");
        assert_ne!(current.id(), old.id());
        runtime
            .replace_session_projection(
                session,
                &Default::default(),
                std::slice::from_ref(&entry),
                None,
            )
            .expect("stale session mirror");
        assert!(
            runtime
                .outbox_for_invocation(&invocation)
                .expect("no resurrection")
                .is_empty()
        );
        let delayed = vec![
            // A live generation cannot authorize an entry whose frozen destination disagrees.
            // 活动代次不能授权冻结目的地不一致的条目。
            VmmWritebackHookEffect::Enqueue {
                permit_id: current.id(),
                request: vulcan_vmm::PostActionRequest {
                    expected_source: Some(VmmMemorySource::Native { project_id: 7 }),
                    ..entry.request.clone()
                },
                error: "wrong source".to_owned(),
            },
            VmmWritebackHookEffect::Enqueue {
                permit_id: old.id(),
                request: entry.request.clone(),
                error: "late".to_owned(),
            },
            VmmWritebackHookEffect::SessionStatus {
                permit_id: old.id(),
                status: vulcan_vmm::SessionMemoryStatus::Active,
            },
            VmmWritebackHookEffect::Enqueue {
                permit_id: current.id(),
                request: entry.request,
                error: "new failure".to_owned(),
            },
        ];
        runtime.with_current_writeback_effects(session, delayed, |effects| {
            assert_eq!(effects.len(), 1);
            assert!(matches!(&effects[0], VmmWritebackHookEffect::Enqueue { permit_id, .. } if *permit_id == current.id()));
            assert!(runtime.state.try_write().is_err(), "publication must retain admission read lock");
        }).expect("publish current only");
    }

    /// Rejects new admission after configuration is disabled during source preparation.
    /// 来源准备期间配置被禁用后，拒绝新的准入。
    #[test]
    fn disabled_configuration_cannot_admit_prepared_writeback() {
        // Keep the exact destination unchanged so readiness is the only rejection reason.
        // 保持准确目的地不变，确保就绪状态是唯一拒绝原因。
        let mut layer = external("127.0.0.1:50051");
        let source = source(&layer);
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
            global: layer.clone(),
            ..Default::default()
        });
        let invocation = invocation(SessionId::from_u128(73), "host-a");
        layer.enabled = Some(false);
        runtime.replace_global_projection(&layer).expect("disable");
        // Retained session behavior must not override the application service shutdown.
        // 保留的会话行为不能覆盖应用服务关闭。
        runtime
            .replace_session_projection(
                SessionId::from_u128(73),
                &VmmConfigLayer {
                    enabled: Some(true),
                    ..Default::default()
                },
                &[],
                None,
            )
            .expect("retained enabled session override");
        assert_eq!(
            runtime
                .admit_writeback(&invocation, &source)
                .expect_err("disabled admission"),
            "VMM writeback configuration is no longer ready."
        );
        assert!(
            runtime
                .read_state("inspect rejected admission")
                .expect("snapshot")
                .writeback_admissions
                .sessions
                .is_empty()
        );
    }

    /// Native rebinding revokes only the old exact owner and never another host sharing that project.
    /// 原生重绑仅撤销此前准确归属，绝不撤销共享该项目的其他宿主。
    #[test]
    fn native_revocation_is_owner_exact_and_session_removal_is_terminal() {
        let runtime = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
            global: VmmConfigLayer {
                backend_mode: Some(VmmBackendMode::Native),
                enabled: Some(true),
                ..Default::default()
            },
            ..Default::default()
        });
        let a = SessionId::from_u128(81);
        let b = SessionId::from_u128(82);
        let first = VmmMemorySource::native(7).expect("first owner");
        let a_permit = runtime
            .admit_writeback(&invocation(a, "a"), &first)
            .expect("a");
        let b_permit = runtime
            .admit_writeback(&invocation(b, "b"), &first)
            .expect("b");
        runtime
            .revoke_native_project_writebacks("a", 7)
            .expect("rebind a");
        assert!(a_permit.ensure_active().is_err());
        assert!(b_permit.ensure_active().is_ok());
        let replacement = runtime
            .admit_writeback(
                &invocation(a, "a"),
                &VmmMemorySource::native(12).expect("new owner"),
            )
            .expect("new a");
        runtime
            .revoke_native_project_writebacks("a", 7)
            .expect("late old-owner notification");
        assert!(replacement.ensure_active().is_ok());
        runtime
            .remove_session_projection(a)
            .expect("remove session");
        assert!(replacement.ensure_active().is_err());
        assert!(b_permit.ensure_active().is_ok());
    }
}
