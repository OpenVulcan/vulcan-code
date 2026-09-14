//! Native PostAction execution shared by direct and revocable host-delivery entry points.
//! 直接入口与可撤销宿主投递入口共用的原生 PostAction 执行。

use super::*;

impl NativeVmmClient {
    /// Executes the frozen business request after exact source and optional generation admission.
    /// 在准确来源及可选代次准入后执行冻结业务请求。
    /// Takes direct or queued host input; once admitted, completes existing atomic intake without cancelling accepted analysis.
    /// 接收直接或排队宿主输入；一旦准入即完成既有原子入库，不取消已接受分析。
    pub(super) async fn post_action_admitted(
        &self,
        request: PostActionRequest,
        permit: Option<&vulcan_vmm::VmmWritebackPermit>,
    ) -> Result<PostActionResponse, VmmClientError> {
        let request_fingerprint = pending_analysis_request_fingerprint(&request)?;
        let now = now_ns()?;
        let subject = subject_id(request.user_id)?;
        let session = session_source_id(&request.session_id)?;
        // Admission and its read guard cover the queue and Core commit without resetting existing state.
        // 准入及其读取守卫覆盖队列与 Core 提交，不重置既有状态。
        let (_operation_guard, access) = self
            .admit_memory_session(
                &request.host_project_id,
                request.project_id,
                &subject,
                &session,
                request.expected_source.as_ref(),
                permit,
            )
            .await?;
        eprintln!(
            "[native-vmm][postaction] stage=session_admitted session={} operation={}",
            request.session_id, request.operation_id,
        );
        // A paused session reports its state without queueing or recording another turn.
        // 暂停会话只报告状态，不入队或记录新回合。
        let session_status = self.memory_session_status(&subject, &access.write_scope, &session)?;
        if !session_status.allows_runtime_memory() {
            eprintln!(
                "[native-vmm][postaction] stage=session_paused session={} status={:?}",
                request.session_id, session_status,
            );
            return Ok(native_post_action_response(session_status));
        }
        // Freeze reduction and Core intake together; later settings changes must not split one admission.
        // 同时冻结缩减和 Core 入库，后续设置变化不得拆散一次准入。
        let (memory_policy, vector_space_id, intake_snapshot_id) = self
            .runtime
            .active_memory_context(&request.host_project_id)
            .await
            .map_err(native_runtime_error)?;
        let timeline = post_action_timeline(&request, now, &memory_policy.postaction_text)?;
        let input_kind = if request.timeline.is_empty() {
            PostActionInputKind::SingleTurn
        } else {
            PostActionInputKind::Timeline
        };
        let caller_operation = stable_or_generated_operation("post-action", &request.operation_id);
        let intake_identity = mutation_identity(
            caller_operation.as_str(),
            MutationSourceKind::HostRpc,
            "native-vmm:post-action",
            now,
        )?;
        let intake_mutation_identity =
            derive_mutation_identity(&intake_identity, "intake").map_err(identifier_error)?;
        let project_scope = access.write_scope.clone();
        let subject_scope = subject_scope_id(request.user_id)?;
        let mut analysis_visible_scopes = access.visible_scopes.clone();
        analysis_visible_scopes.push(subject_scope.clone());
        let analysis_operation = derived_operation_id(caller_operation.as_str(), "analysis-1");
        let payload = NativePendingPostActionPayload {
            host_project_id: request.host_project_id.clone(),
            intake: PostActionIntakeCommand {
                input_kind,
                subject_id: subject.clone(),
                scope_access: access,
                session_source_id: session.clone(),
                vector_space_id: vector_space_id.clone(),
                timeline,
                completed_at_ns: now,
                mutation_identity: intake_identity,
            },
            analysis: NativePendingAnalysisTemplate {
                replacement_scope: MemoryReplacementScope {
                    candidates: MemoryCandidateScope {
                        scope_ids: analysis_visible_scopes.clone(),
                        origin_session_source_id: None,
                    },
                    authorized_supersede_scopes: vec![project_scope.clone()],
                },
                subject_id: subject,
                scope_access: ScopeAccess {
                    write_scope: project_scope.clone(),
                    visible_scopes: analysis_visible_scopes,
                },
                profile_sections: vec![
                    ProfileViewSection {
                        scope_id: project_scope,
                        role: SectionRole::Environment,
                        display_label: String::from("Project"),
                    },
                    ProfileViewSection {
                        scope_id: subject_scope,
                        role: SectionRole::Subject,
                        display_label: String::from("User"),
                    },
                ],
                session_source_id: session,
                vector_space_id,
                analysis_generation: 1,
                mutation_identity: mutation_identity(
                    analysis_operation.as_str(),
                    MutationSourceKind::BackgroundWorker,
                    "native-vmm:post-action-analysis",
                    now,
                )?,
            },
        };
        let payload_json = serde_json::to_string(&payload).map_err(|error| {
            VmmClientError::new(
                "native_analysis_queue",
                format!("serialize native VMM analysis job failed: {error}"),
            )
        })?;
        let queued = self
            .data_plane
            .enqueue_pending_analysis_job(&NativeVmmPendingAnalysisJob {
                operation_id: caller_operation.clone(),
                request_fingerprint: request_fingerprint.clone(),
                payload_json,
                intake_mutation_operation_id: intake_mutation_identity.operation_id.to_string(),
                turn_id: None,
                state: String::from(VMM_PENDING_ANALYSIS_INTAKING),
                attempt_count: 0,
                next_attempt_at_unix_ms: 0,
                last_error: None,
            })
            .await
            .map_err(|error| VmmClientError::new("native_analysis_queue", error))?;
        eprintln!(
            "[native-vmm][postaction] stage=durable_queue session={} operation={} state={}",
            request.session_id, caller_operation, queued.state,
        );
        if pending_analysis_job_is_accepted(&queued) {
            if queued.state == VMM_PENDING_ANALYSIS_READY {
                self.pending_analysis_notify.notify_one();
            }
            return Ok(native_post_action_response(session_status));
        }
        let payload: NativePendingPostActionPayload = serde_json::from_str(&queued.payload_json)
            .map_err(|error| {
                VmmClientError::new(
                    "native_analysis_queue",
                    format!("parse native VMM analysis job failed: {error}"),
                )
            })?;
        let result = match self
            .runtime
            .run_with_snapshot(
                CoreCommand::PostActionIntake(payload.intake.clone()),
                intake_snapshot_id,
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                if let Err(recovery_error) = self.runtime.recover_storage_mutations().await {
                    return Err(VmmClientError::new(
                        "native_analysis_queue_recovery",
                        format!(
                            "native VMM intake failed ({error}) and storage recovery failed: {recovery_error}"
                        ),
                    ));
                }
                let recovered = self
                    .data_plane
                    .load_pending_analysis_job(&caller_operation)
                    .await
                    .map_err(|recovery_error| {
                        VmmClientError::new("native_analysis_queue_recovery", recovery_error)
                    })?;
                if recovered
                    .as_ref()
                    .is_some_and(pending_analysis_job_is_accepted)
                {
                    self.pending_analysis_notify.notify_one();
                    return Ok(native_post_action_response(session_status));
                }
                self.data_plane
                    .delete_pending_analysis_intake(&caller_operation, &request_fingerprint)
                    .await
                    .map_err(|cleanup_error| {
                        VmmClientError::new("native_analysis_queue_cleanup", cleanup_error)
                    })?;
                return Err(core_client_error(error));
            }
        };
        let vulcan_memorymesh_core::CommandResult::PostActionAccepted(result) = result else {
            return Err(unexpected_result("PostActionAccepted"));
        };
        eprintln!(
            "[native-vmm][postaction] stage=core_intake session={} operation={} turn_created={}",
            request.session_id,
            caller_operation,
            result.turn_id.is_some(),
        );
        if let Some(turn_id) = result.turn_id.as_ref() {
            let queued = self
                .data_plane
                .load_pending_analysis_job(&caller_operation)
                .await
                .map_err(|error| VmmClientError::new("native_analysis_queue", error))?
                .ok_or_else(|| {
                    VmmClientError::new(
                        "native_analysis_queue",
                        "native VMM intake committed without a durable analysis job",
                    )
                })?;
            if queued.state != VMM_PENDING_ANALYSIS_READY
                || queued.turn_id.as_deref() != Some(turn_id.as_str())
            {
                return Err(VmmClientError::new(
                    "native_analysis_queue",
                    "native VMM intake commit did not atomically bind its analysis job",
                ));
            }
            self.pending_analysis_notify.notify_one();
        } else {
            self.data_plane
                .complete_pending_analysis_job(&caller_operation, &request_fingerprint)
                .await
                .map_err(|error| VmmClientError::new("native_analysis_queue", error))?;
        }
        Ok(native_post_action_response(session_status))
    }
}
