//! Exercises malformed post-commit acknowledgements through synchronous, asynchronous, and recovery paths.
//! 通过同步、异步及恢复路径验证提交后畸形确认的处理。

#![allow(clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use vulcan_memorymesh_core::{
    AsyncHostDriver, ChatCompactCommand, CommandHandle, CommandStatus, CommandStep, CoreCommand,
    CoreError, EffectId, EffectRequest, EffectResult, ErrorCode, HostDriver, HostEffect,
    HostEffectResult, IdempotencyKey, InMemoryStorage, MemoryMeshCore, MutationIdentity,
    MutationOutcome, MutationSourceKind, OperationId, ScopeAccess, ScopeId, SessionSourceId,
    StoragePort, SubjectId, request_fingerprint,
};

/// Selects a protocol failure after the Host has already persisted its receipt.
/// 选择在宿主已经持久化收据之后发生的协议故障。
#[derive(Clone, Copy)]
enum AckFailure {
    /// Substitutes a foreign request identity.
    /// 替换为外来请求身份。
    Identity,
    /// Substitutes the wrong typed capability result.
    /// 替换为错误的强类型能力结果。
    Kind,
    /// Exceeds the pre-transition recoverability bound.
    /// 超出转换前可恢复性上限。
    Size,
}

/// Persists through the reference ledger before corrupting the acknowledgement.
/// 通过参考账本持久化后损坏确认。
struct CorruptingHost {
    /// Owns the single committed batch and its exact durable outcome.
    /// 拥有单一已提交批次及其精确持久结果。
    storage: InMemoryStorage,
    /// Selects the acknowledgement failure injected by this Host.
    /// 选择此宿主注入的确认故障。
    failure: AckFailure,
    /// Counts external writes so the test can reject blind replay.
    /// 统计外部写入次数，使测试可以拒绝盲重放。
    writes: usize,
}

impl HostDriver for CorruptingHost {
    /// Commits exactly one requested mutation and returns an unusable acknowledgement.
    /// 提交恰好一个请求变更并返回不可用的确认。
    fn execute_effect(&mut self, request: EffectRequest) -> Result<EffectResult, CoreError> {
        let HostEffect::StorageMutation(batch) = request.effect else {
            panic!("compact fixture must emit only a storage mutation");
        };
        self.writes += 1;
        let outcome = self.storage.mutate(&batch)?;
        assert!(matches!(outcome, MutationOutcome::Committed { .. }));
        Ok(match self.failure {
            AckFailure::Identity => EffectResult {
                effect_id: EffectId::new("foreign-effect").expect("valid identity"),
                result: HostEffectResult::StorageMutation(outcome),
            },
            AckFailure::Kind => EffectResult {
                effect_id: request.effect_id,
                result: HostEffectResult::Rerank(vec![vulcan_memorymesh_core::RerankEntry {
                    input_index: 0,
                    score: 1.0,
                }]),
            },
            AckFailure::Size => EffectResult {
                effect_id: request.effect_id,
                result: HostEffectResult::StorageMutation(MutationOutcome::Rejected {
                    reason: "x".repeat(16_777_217),
                }),
            },
        })
    }
}

impl AsyncHostDriver for CorruptingHost {
    /// Uses the identical Host failure while completing the test future immediately.
    /// 使用同一宿主故障，并立即完成测试 Future。
    fn execute_effect<'a>(
        &'a mut self,
        request: EffectRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EffectResult, CoreError>> + Send + 'a>> {
        Box::pin(async move { HostDriver::execute_effect(self, request) })
    }
}

/// Builds one valid mutating command with the exact active snapshot fingerprint.
/// 使用精确活跃快照指纹构造有效变更命令。
fn compact_command(core: &MemoryMeshCore) -> CoreCommand {
    let scope = ScopeId::new("scope").expect("scope");
    let mut command = CoreCommand::ChatCompact(ChatCompactCommand {
        subject_id: SubjectId::new("subject").expect("subject"),
        scope_access: ScopeAccess {
            write_scope: scope.clone(),
            visible_scopes: vec![scope],
        },
        session_source_id: SessionSourceId::new("session").expect("session"),
        compacted_at_ns: 10,
        mutation_identity: MutationIdentity {
            operation_id: OperationId::new("operation").expect("operation"),
            idempotency_key: IdempotencyKey::new("key").expect("key"),
            request_fingerprint: "0".repeat(64),
            source_kind: MutationSourceKind::HostRpc,
            request_source_id: "ack-test".to_owned(),
            observed_at_ns: 10,
        },
    });
    let fingerprint = request_fingerprint(
        &command,
        core.active_snapshot().expect("snapshot").id.as_str(),
    );
    if let CoreCommand::ChatCompact(payload) = &mut command {
        payload.mutation_identity.request_fingerprint = fingerprint;
    }
    command
}

/// Checks both public runners retain the exact reconciliation key after every malformed acknowledgement.
/// 检查两个公共执行器在每种畸形确认后均保留精确对账键。
#[test]
fn automatic_runners_preserve_malformed_mutation_acknowledgements() {
    for asynchronous in [false, true] {
        for failure in [AckFailure::Identity, AckFailure::Kind, AckFailure::Size] {
            let core = MemoryMeshCore::new().expect("Core");
            let command = compact_command(&core);
            let mut host = CorruptingHost {
                storage: InMemoryStorage::new(),
                failure,
                writes: 0,
            };
            let result = if asynchronous {
                let mut future = Box::pin(core.run_to_completion_async(command, &mut host));
                let Poll::Ready(result) = future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                else {
                    panic!("immediate Host must resolve without an executor");
                };
                result
            } else {
                core.run_to_completion(command, &mut host)
            };
            let error = result.expect_err("malformed acknowledgement cannot report success");
            assert_eq!(error.code, ErrorCode::StorageResultUncertain);
            assert!(error.side_effect_possible);
            assert_eq!(host.writes, 1, "must not replay the write");
            let handle = CommandHandle {
                command_id: error.command_id.expect("retained command"),
                config_snapshot_id: error.config_snapshot_id.expect("retained snapshot"),
            };
            let outcome = host
                .storage
                .reconcile(&error.idempotency_key.expect("ledger key"))
                .expect("durable committed receipt");

            // An invalid reconciliation payload must not consume uncertainty or its pending state.
            // 无效对账载荷不能消耗不确定状态及其待处理状态。
            let before = core
                .command_diagnostics(&handle)
                .expect("uncertain diagnostics");
            let too_large = core.reconcile(
                &handle,
                MutationOutcome::Rejected {
                    reason: "x".repeat(16_777_217),
                },
            );
            assert_eq!(
                too_large.expect_err("oversized reconciliation").code,
                ErrorCode::EffectMismatch
            );
            let after = core.command_diagnostics(&handle).expect("still retained");
            assert_eq!(before, after);
            assert_eq!(after.status, CommandStatus::ResultUncertain);

            let checkpoint = core
                .export_checkpoint(&handle)
                .expect("uncertain checkpoint");
            let fresh = MemoryMeshCore::new().expect("fresh Core instance");
            let (restored, _) = fresh
                .restore_checkpoint(&checkpoint)
                .expect("restore uncertain");
            assert!(matches!(
                fresh
                    .reconcile(&restored, outcome.clone())
                    .expect("fresh reconciliation"),
                CommandStep::Complete(_)
            ));
            assert!(matches!(
                core.reconcile(&handle, outcome)
                    .expect("original reconciliation"),
                CommandStep::Complete(_)
            ));
            fresh.release_command(&restored).expect("release restored");
            core.release_command(&handle).expect("release original");
        }
    }
}
