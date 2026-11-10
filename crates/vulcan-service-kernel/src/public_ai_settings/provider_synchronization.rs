//! Provider Manifest synchronization, staged publication, and cancellation compensation.
//! 供应商 Manifest 同步、暂存发布与取消补偿。

use std::sync::Arc;
use vulcan_provider_adapters::{
    CredentialDirectoryVisibility, PersistedAiConfiguration, PublicAiRuntime,
};
use vulcan_service_api::ServiceEventEmitter;

use super::{
    public_ai_runtime_and_configuration,
    publication::{publish_public_ai_workbench, publish_public_ai_workbench_with_commit},
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Activates every affected connection catalog and captures the exact published runtime image.
/// 激活每个受影响连接目录并捕获精确的已发布运行时镜像。
///
/// # Parameters
/// 参数
/// `runtime` owns catalog state; `configuration` supplies exact connections; `provider_ids` is the newly activated set; `transaction` retains catalog writer ownership.
/// `runtime` 持有目录状态；`configuration` 提供精确连接；`provider_ids` 是新激活集合；`transaction` 保持目录写入所有权。
///
/// # Returns
/// 返回值
/// The exact post-activation image used as compensation's compare-and-swap guard.
/// 返回作为补偿比较交换护栏的精确激活后镜像。
async fn activate_synced_provider_catalogs(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    provider_ids: &std::collections::BTreeSet<String>,
    transaction: &vulcan_provider_adapters::ProviderCatalogActivationTransaction,
) -> Result<vulcan_provider_adapters::ProviderRuntimeActivationSnapshot, String> {
    // connectionIds is the complete deterministic projection set for the affected providers.
    // connectionIds 是受影响供应商的完整确定性投影集合。
    let connection_ids = configuration
        .connection_graph
        .connections
        .values()
        .filter(|connection| provider_ids.contains(&connection.provider_id))
        .map(|connection| connection.id.clone())
        .collect::<Vec<_>>();
    runtime
        .activate_staged_manifest_catalogs_in_transaction(
            configuration,
            &connection_ids,
            transaction,
        )
        .await
}

/// Closed result of a failed provider projection publication transaction.
/// 供应商投影发布事务失败后的封闭结果。
#[derive(Debug)]
enum ProviderProjectionPublicationFailure {
    /// Every disk, staged-manifest, runtime, and Workbench before-image was restored exactly.
    /// 每份磁盘、暂存 Manifest、运行时与工作台前镜像均已精确恢复。
    Compensated(String),
    /// At least one compensation failed, so continuing could publish from an uncertain authority.
    /// 至少一项补偿失败，继续执行可能从状态不确定的权威发布。
    Uncertain(String),
}

/// Restores one staged provider activation when its asynchronous publication future is cancelled.
/// 在异步发布 Future 被取消时恢复一次已暂存供应商激活。
struct StagedProviderActivationCancellationGuard<'a> {
    /// Service owning the durable Manifest transaction and compensation methods.
    /// 持有持久化 Manifest 事务与补偿方法的服务。
    service: &'a crate::provider_manifest_sync::ProviderManifestSyncService,
    /// Runtime owning the staged manifest catalog that must match the durable before-image.
    /// 持有必须与持久化前镜像一致的暂存 Manifest 目录的运行时。
    runtime: &'a PublicAiRuntime,
    /// Reports identifying the staged activation for provider-scoped telemetry.
    /// 标识已暂存激活并用于逐供应商遥测的报告。
    reports: &'a [crate::provider_manifest_sync::ProviderSyncReport],
    /// Exact provider-scoped before-images restored on cancellation.
    /// 在取消时恢复的精确逐供应商前镜像。
    snapshots: &'a [crate::provider_manifest_sync::ProviderManifestActivationSnapshot],
    /// Armed state retained only while an asynchronous cancellation point can still be crossed.
    /// 仅在仍可能越过异步取消点时保持的启用状态。
    armed: bool,
}

impl<'a> StagedProviderActivationCancellationGuard<'a> {
    /// Arms cancellation compensation for one exact staged activation set.
    /// 为一组精确已暂存激活启用取消补偿。
    ///
    /// # Parameters
    /// 参数
    /// `service`, `runtime`, `reports`, and `snapshots` are the complete transaction authority.
    /// `service`、`runtime`、`reports` 与 `snapshots` 构成完整事务权威。
    ///
    /// # Returns
    /// 返回值
    /// An armed guard that restores before-images if dropped before `disarm`.
    /// 返回一个已启用护栏；若在 `disarm` 前被丢弃则恢复前镜像。
    fn new(
        service: &'a crate::provider_manifest_sync::ProviderManifestSyncService,
        runtime: &'a PublicAiRuntime,
        reports: &'a [crate::provider_manifest_sync::ProviderSyncReport],
        snapshots: &'a [crate::provider_manifest_sync::ProviderManifestActivationSnapshot],
    ) -> Self {
        Self {
            service,
            runtime,
            reports,
            snapshots,
            armed: true,
        }
    }

    /// Disarms automatic compensation after execution has left every asynchronous cancellation point.
    /// 在执行离开全部异步取消点后停用自动补偿。
    ///
    /// # Parameters
    /// 参数
    /// `self` is the cancellation guard after the final asynchronous wait.
    /// `self` 是最后一个异步等待结束后的取消守卫。
    ///
    /// # Returns
    /// 返回值
    /// No value; subsequent drop leaves explicit synchronous completion in control.
    /// 无返回值；后续析构将控制权保留给显式同步收尾。
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagedProviderActivationCancellationGuard<'_> {
    /// Restores staged disk/runtime ownership on asynchronous cancellation.
    /// 在异步取消时恢复已暂存磁盘与运行时所有权。
    ///
    /// # Parameters
    /// 参数
    /// `self` retains the exact staged activation before-images until disarmed.
    /// `self` 在停用前持有精确暂存激活前镜像。
    ///
    /// # Returns
    /// 返回值
    /// No value; an armed guard attempts restoration and preserves the recovery journal on failure.
    /// 无返回值；启用状态尝试恢复，失败时保留恢复日志。
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // restoreResult is intentionally best-effort in Drop; an incomplete runtime restore keeps
        // the Applying journal for deterministic startup recovery and the process remains fail-closed.
        // restoreResult 在 Drop 中有意采用尽力恢复；运行时恢复不完整时会保留 Applying 日志，
        // 供启动确定性恢复，且当前进程保持失败关闭。
        let restore_result = self
            .service
            .restore_activation_snapshots(self.runtime, self.snapshots);
        if restore_result.is_ok() {
            self.service.record_compensated_rollback_events(
                self.runtime,
                self.reports,
                "activation_cancelled",
            );
        }
    }
}

/// Validates and extracts every exact Manifest activation before-image required for publication.
/// 校验并提取发布所需的每份精确 Manifest 激活前镜像。
///
/// # Parameters
/// 参数
/// `reports` are provider-scoped activation outcomes and `provider_ids` is the exact projection set.
/// `reports` 是逐供应商激活结果，`provider_ids` 是精确投影集合。
///
/// # Returns
/// 返回值
/// The complete ordered before-image list, or an uncertain contract failure that forbids publication.
/// 返回完整有序前镜像列表；若契约状态不确定则返回禁止发布的失败。
fn required_provider_activation_snapshots(
    reports: &[crate::provider_manifest_sync::ProviderSyncReport],
    provider_ids: &std::collections::BTreeSet<String>,
) -> Result<
    Vec<crate::provider_manifest_sync::ProviderManifestActivationSnapshot>,
    ProviderProjectionPublicationFailure,
> {
    let report_provider_ids = reports
        .iter()
        .map(|report| report.provider_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if reports.is_empty()
        || report_provider_ids.len() != reports.len()
        || &report_provider_ids != provider_ids
    {
        return Err(ProviderProjectionPublicationFailure::Uncertain(
            String::from("Provider activation reports do not match the exact projection set."),
        ));
    }
    reports
        .iter()
        .map(|report| {
            if report.outcome != "activated" {
                return Err(ProviderProjectionPublicationFailure::Uncertain(
                    String::from("Provider projection received a non-activation report."),
                ));
            }
            let snapshot = report.activation_snapshot.clone().ok_or_else(|| {
                ProviderProjectionPublicationFailure::Uncertain(String::from(
                    "Provider activation report is missing its exact before-image.",
                ))
            })?;
            if snapshot.provider_id != report.provider_id {
                return Err(ProviderProjectionPublicationFailure::Uncertain(
                    String::from(
                        "Provider activation before-image identity does not match its report.",
                    ),
                ));
            }
            Ok(snapshot)
        })
        .collect()
}

/// Reconciles one signed model generation and durably records every manual-model takeover.
/// 对账一个签名模型代际，并持久记录每一项手工模型接管。
///
/// # Parameters
/// 参数
/// `configuration` is the transactional candidate and `manifest` is its exact signed authority.
/// `configuration` 是事务候选配置；`manifest` 是对应的精确签名权威。
///
/// # Returns
/// 返回值
/// Notices for the current UI publication; the same identities remain in configuration until acknowledged.
/// 返回供当前界面发布的提示；相同身份会保留在配置中，直到界面确认。
fn reconcile_signed_models_and_record_notices(
    configuration: &mut PersistedAiConfiguration,
    manifest: &vulcan_provider_adapters::provider_manifest::ResolvedProviderManifest,
) -> Vec<vulcan_service_api::StartupModelReconciliationNotice> {
    let provider_id = manifest.descriptor.id.as_str();
    let notices = configuration
        .reconcile_signed_manual_model_collisions(manifest)
        .into_iter()
        .map(|model_id| {
            configuration.record_pending_system_model_takeover_notice(provider_id, &model_id);
            vulcan_service_api::StartupModelReconciliationNotice {
                provider_id: provider_id.to_string(),
                model_id,
            }
        })
        .collect::<Vec<_>>();
    configuration.reconcile_signed_model_inventory(manifest);
    notices
}

/// Restores staged Manifest ownership before any runtime generation was published.
/// 在任何运行时代际发布前恢复已暂存 Manifest 的所有权。
///
/// # Parameters
/// 参数
/// `service` and `runtime` own the durable and staged layers; `reports` identify telemetry;
/// `snapshots` are exact before-images; and `primary` is the preparation failure.
/// `service` 与 `runtime` 持有持久层和暂存层；`reports` 标识遥测；`snapshots` 是精确前镜像；
/// `primary` 是准备阶段失败。
///
/// # Returns
/// 返回值
/// A compensated provider-local failure only when every before-image is restored, otherwise uncertain.
/// 仅当每份前镜像都恢复时返回逐供应商已补偿失败，否则返回状态不确定。
fn compensate_unpublished_provider_activation(
    service: &crate::provider_manifest_sync::ProviderManifestSyncService,
    runtime: &PublicAiRuntime,
    reports: &[crate::provider_manifest_sync::ProviderSyncReport],
    snapshots: &[crate::provider_manifest_sync::ProviderManifestActivationSnapshot],
    primary: String,
) -> ProviderProjectionPublicationFailure {
    let manifest_restore = service.restore_activation_snapshots(runtime, snapshots);
    if manifest_restore.is_ok() {
        service.record_compensated_rollback_events(
            runtime,
            reports,
            "activation_publication_failed",
        );
        ProviderProjectionPublicationFailure::Compensated(primary)
    } else {
        ProviderProjectionPublicationFailure::Uncertain(format!(
            "{primary}; Manifest activation before-image restore: {manifest_restore:?}"
        ))
    }
}

/// Publishes post-sync projections or restores every exact activation before-image on failure.
/// 发布同步后投影，或在失败时恢复每份精确激活前镜像。
///
/// # Parameters
/// 参数
/// `service` owns the durable store, `reports` retain per-activation before-images, and
/// `provider_ids` is the exact projection set.
/// `service` 持有持久存储，`reports` 保留逐激活前镜像，`provider_ids` 是精确投影集合。
///
/// # Returns
/// 返回值
/// Success only after the new generation is fully visible; failures distinguish an exact
/// provider-local compensation from an uncertain shared state that must stop the batch.
/// 仅当新代际完全可见后返回成功；失败会区分精确逐供应商补偿与必须中止批次的共享状态不确定。
async fn publish_synced_provider_projections_or_restore(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    service: &crate::provider_manifest_sync::ProviderManifestSyncService,
    reports: &[crate::provider_manifest_sync::ProviderSyncReport],
    provider_ids: &std::collections::BTreeSet<String>,
) -> Result<(), ProviderProjectionPublicationFailure> {
    let manifest_snapshots = required_provider_activation_snapshots(reports, provider_ids)?;
    // cancellationGuard owns exact compensation across every following asynchronous wait.
    // cancellationGuard 在后续每个异步等待点期间持有精确补偿责任。
    let mut cancellation_guard = StagedProviderActivationCancellationGuard::new(
        service,
        runtime,
        reports,
        &manifest_snapshots,
    );
    // configurationTransaction prevents settings writes from crossing configuration capture,
    // publication, or compensation for this exact provider activation.
    // configurationTransaction 防止设置写入穿越当前精确供应商激活的配置捕获、发布或补偿。
    let _configuration_transaction = begin_public_ai_configuration_transaction().await;
    // configuration is captured only after the provider before-images are proven complete.
    // configuration 仅在供应商前镜像被证明完整后捕获。
    let configuration = match runtime.configuration_snapshot() {
        Ok(configuration) => configuration,
        Err(primary) => {
            // No asynchronous cancellation point remains before explicit compensation completes.
            // 显式补偿完成前已不存在异步取消点。
            cancellation_guard.disarm();
            return Err(compensate_unpublished_provider_activation(
                service,
                runtime,
                reports,
                &manifest_snapshots,
                format!("Provider configuration snapshot failed before projection: {primary}"),
            ));
        }
    };
    // The staged after-images are signed, exact provider authorities. Reconcile only the
    // candidate copy; durable user definitions are removed after manifest commit succeeds.
    // 暂存后镜像是经过签名的精确供应商权威。先只对候选副本对账；清单提交成功后才持久移除用户定义。
    let mut candidate_configuration = configuration.clone();
    let mut model_reconciliation_notices = Vec::new();
    for snapshot in &manifest_snapshots {
        let Some(manifest) = snapshot
            .staged_manifest_mutation
            .staged_manifest(snapshot.provider_id.as_str())
        else {
            cancellation_guard.disarm();
            return Err(compensate_unpublished_provider_activation(
                service,
                runtime,
                reports,
                &manifest_snapshots,
                format!(
                    "Staged manifest `{}` disappeared before model reconciliation.",
                    snapshot.provider_id
                ),
            ));
        };
        model_reconciliation_notices.extend(reconcile_signed_models_and_record_notices(
            &mut candidate_configuration,
            &manifest,
        ));
    }
    // catalogTransaction remains owned until success or every compensation/republish step ends.
    // catalogTransaction 持续持有到成功或全部补偿/重新发布步骤结束。
    let catalog_transaction = runtime.begin_catalog_activation_transaction().await;
    // runtimeBeforeImage is the single exact catalog/generation snapshot before batch publication.
    // runtimeBeforeImage 是批量发布前唯一的精确目录与代际快照。
    let runtime_before_image = match runtime.capture_activation_snapshot() {
        Ok(snapshot) => snapshot,
        Err(primary) => {
            // Explicit synchronous compensation owns all remaining work for this failure.
            // 本次失败的全部剩余工作由显式同步补偿持有。
            cancellation_guard.disarm();
            return Err(compensate_unpublished_provider_activation(
                service,
                runtime,
                reports,
                &manifest_snapshots,
                format!("Provider runtime before-image capture failed: {primary}"),
            ));
        }
    };
    // publication includes durable last-checked bookkeeping so an `activated` report is never
    // returned before its operator-visible success state exists.
    // publication 包含持久化最近检查簿记，使 `activated` 报告绝不会在操作员可见成功状态
    // 存在前返回。
    // runtimeAfterImage exists only after the isolated activation crossed its single publication
    // boundary. A preparation conflict leaves the main runtime untouched and therefore needs no
    // runtime compensation.
    // runtimeAfterImage 仅在隔离激活越过唯一发布边界后存在；准备冲突不会触碰主运行时，
    // 因而无需运行时补偿。
    // activationResult is the final asynchronous cancellation boundary in this transaction.
    // activationResult 是本事务中的最后一个异步取消边界。
    let activation_result = activate_synced_provider_catalogs(
        runtime,
        &candidate_configuration,
        provider_ids,
        &catalog_transaction,
    )
    .await;
    cancellation_guard.disarm();
    let mut configuration_persistence_attempted = false;
    let (runtime_after_image, publication) = match activation_result {
        Ok(after_image) => {
            // publication keeps browser state unchanged while check-state persistence and journal
            // commit remain fallible. Once that closure succeeds, the Workbench swap is infallible.
            // publication 在检查状态持久化与日志提交仍可失败时保持浏览器状态不变；闭包成功后，
            // 工作台交换不再失败。
            let publication = publish_public_ai_workbench_with_commit(
                app,
                state,
                runtime,
                &candidate_configuration,
                || {
                    // Check state remains inside the Applying before-image until it is durable;
                    // only then may the journal cross its commit boundary.
                    // 检查状态在持久化完成前始终处于 Applying 前镜像保护内；之后日志才可越过提交边界。
                    service.record_sync_check_reports(reports)?;
                    service.commit_activation_snapshots(&manifest_snapshots)?;
                    if candidate_configuration != configuration {
                        configuration_persistence_attempted = true;
                        vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                            &candidate_configuration,
                            &crate::service_state_access::public_ai_configuration_app_data_root(),
                        )?;
                    }
                    Ok(())
                },
            )
            .map(|_| ());
            (Some(after_image), publication)
        }
        Err(error) => (None, Err(error)),
    };
    match publication {
        Ok(()) => {
            service.record_committed_activation_events(runtime, reports);
            if !model_reconciliation_notices.is_empty()
                && let Ok(mut guard) = state.lock()
            {
                guard
                    .startup
                    .model_reconciliation_notices
                    .extend(model_reconciliation_notices);
                let startup = guard.startup.clone();
                drop(guard);
                crate::emit_service_event_with_trace(
                    app,
                    "startup_snapshot_emit",
                    vulcan_service_api::ServiceEvent::Startup(Box::new(startup)),
                );
            }
            Ok(())
        }
        Err(primary) => {
            let configuration_restore = if configuration_persistence_attempted {
                vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                    &configuration,
                    &crate::service_state_access::public_ai_configuration_app_data_root(),
                )
            } else {
                Ok(())
            };
            let manifest_restore =
                service.restore_activation_snapshots(runtime, &manifest_snapshots);
            let runtime_restore = runtime_after_image.as_ref().map_or(Ok(()), |after_image| {
                runtime.restore_activation_snapshot(&runtime_before_image, after_image)
            });
            let workbench_republish = match (&runtime_restore, runtime_after_image.is_some()) {
                (Ok(()), true) => {
                    publish_public_ai_workbench(app, state, runtime, &configuration).map(|_| ())
                }
                (Ok(()), false) => Ok(()),
                (Err(_), _) => Err(String::from(
                    "Old-generation Workbench projection was not republished because runtime compensation failed.",
                )),
            };
            let compensation_complete = configuration_restore.is_ok()
                && manifest_restore.is_ok()
                && runtime_restore.is_ok()
                && workbench_republish.is_ok();
            if compensation_complete {
                service.record_compensated_rollback_events(
                    runtime,
                    reports,
                    "activation_publication_failed",
                );
            }
            let diagnostic = format!(
                "Provider manifest projection publication failed: {primary}; configuration compensation: {configuration_restore:?}; manifest compensation: {manifest_restore:?}; runtime compensation: {runtime_restore:?}; old-generation Workbench republication: {workbench_republish:?}"
            );
            if compensation_complete {
                Err(ProviderProjectionPublicationFailure::Compensated(
                    diagnostic,
                ))
            } else {
                Err(ProviderProjectionPublicationFailure::Uncertain(diagnostic))
            }
        }
    }
}

/// Converts one fully compensated activation into a provider-local rejected sync report.
/// 把一次已完整补偿的激活转换为逐供应商拒绝同步报告。
///
/// # Parameters
/// 参数
/// `report` is the staged activation identity and `diagnostic` is the local compensation detail.
/// `report` 是已暂存激活身份，`diagnostic` 是本地补偿详情。
///
/// # Returns
/// 返回值
/// A serializable rejection with no retained before-image and one bounded public diagnostic.
/// 返回不再保留前镜像且仅含一条有界公开诊断的可序列化拒绝结果。
fn compensated_provider_projection_report(
    mut report: crate::provider_manifest_sync::ProviderSyncReport,
    diagnostic: &str,
) -> crate::provider_manifest_sync::ProviderSyncReport {
    report.outcome = "rejected";
    report.failure_kind = Some(String::from("activation_publication_failed"));
    report.failure_message =
        Some(crate::provider_manifest_sync::bounded_public_manifest_diagnostic(diagnostic));
    report.activation_snapshot = None;
    report
}

/// Finalizes one provider sync report as an isolated activation and check-state transaction.
/// 将一份供应商同步报告作为隔离的激活与检查状态事务完成。
///
/// # Parameters
/// 参数
/// `app` and `state` own the visible projection, `runtime` owns the immutable generation,
/// `service` owns durable Manifest layers, and `report` is the exact provider-scoped outcome.
/// `app` 与 `state` 持有可见投影，`runtime` 持有不可变代际，`service` 持有持久 Manifest 层，
/// `report` 是精确的逐供应商结果。
///
/// # Returns
/// 返回值
/// The same report only after its activation or non-activation check state is durably finalized.
/// 仅在对应激活或非激活检查状态持久完成后返回同一报告。
async fn finalize_provider_sync_report(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    service: &crate::provider_manifest_sync::ProviderManifestSyncService,
    mut report: crate::provider_manifest_sync::ProviderSyncReport,
) -> Result<crate::provider_manifest_sync::ProviderSyncReport, String> {
    if report.outcome != "activated" {
        if let Err(error) = service.record_sync_check_reports(std::slice::from_ref(&report)) {
            // primaryFailureKind preserves a candidate rejection when one already exists; a
            // successful remote outcome instead becomes an explicit check-state rejection.
            // primaryFailureKind 在候选已被拒绝时保留其分类；成功远端结果则转为明确的
            // check-state 拒绝。
            let primary_failure_kind = report
                .failure_kind
                .clone()
                .unwrap_or_else(|| String::from("check_state_publication_failed"));
            report.outcome = "rejected";
            report.failure_kind = Some(primary_failure_kind);
            report.failure_message = Some(
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str()),
            );
            report.activation_snapshot = None;
        }
        return Ok(report);
    }

    // providerIds deliberately contains one member so failure compensation cannot roll back an
    // unrelated provider that already committed earlier in the shared Index batch.
    // providerIds 有意只包含一个成员，使失败补偿不会回滚共享 Index 批次中较早提交的无关供应商。
    let provider_ids = std::collections::BTreeSet::from([report.provider_id.clone()]);
    // publicationResult owns the complete provider-scoped activation transaction.
    // publicationResult 持有完整的逐供应商激活事务。
    let publication_result = publish_synced_provider_projections_or_restore(
        app,
        state,
        runtime,
        service,
        std::slice::from_ref(&report),
        &provider_ids,
    )
    .await;
    match publication_result {
        Ok(()) => Ok(report),
        Err(ProviderProjectionPublicationFailure::Compensated(primary)) => {
            // rejectedReport closes this provider transaction without suppressing later providers
            // from the same authenticated Index snapshot (plan §17.4).
            // rejectedReport 封闭当前供应商事务，同时不压制同一认证 Index 快照中的后续
            // 供应商（方案 §17.4）。
            report = compensated_provider_projection_report(report, primary.as_str());
            if service
                .record_sync_check_reports(std::slice::from_ref(&report))
                .is_err()
            {
                report.failure_message = Some(String::from(
                    "Activation publication was rolled back, and its check-state update could not be persisted.",
                ));
            }
            Ok(report)
        }
        Err(ProviderProjectionPublicationFailure::Uncertain(primary)) => {
            // failureState is best-effort observability after an uncertain compensation; the
            // batch must stop regardless because catalog authority can no longer be proven.
            // failureState 是补偿状态不确定后的尽力可观测性；无论其结果如何都必须中止批次，
            // 因为目录权威已无法证明。
            let failure_state = service.record_sync_check_failures(
                std::slice::from_ref(&report.provider_id),
                "activation_publication_failed",
            );
            let primary =
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(primary.as_str());
            if failure_state.is_err() {
                Err(format!(
                    "{primary} Check-state publication also failed after compensation became uncertain."
                ))
            } else {
                Err(primary)
            }
        }
    }
}

/// Bounds and redacts one internal failure before it crosses a Provider Manifest RPC boundary.
/// 在内部失败越过 Provider Manifest RPC 边界前进行有界化与脱敏。
///
/// # Parameters
/// 参数
/// `error` is the exact internal diagnostic, which may contain local paths or oversized text.
/// `error` 是可能包含本地路径或超长文本的精确内部诊断。
///
/// # Returns
/// 返回值
/// A UTF-8-safe public diagnostic that retains no detected local path or secret assignment.
/// 返回 UTF-8 安全的公开诊断，且不保留检测到的本地路径或秘密赋值。
fn bounded_provider_manifest_rpc_error(error: String) -> String {
    crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
}

/// Runs the production provider-manifest sync call chain for every system provider.
/// 为每个系统供应商执行生产清单同步调用链。
///
/// The sync service remains unconfigured until the external confirmation gate opens; the
/// production call chain exists and reports `not_configured` without guessing any remote
/// endpoint (plan §7.2/§17.3).
/// 同步服务在外部确认门开启前保持未配置；生产调用链存在并报告 `not_configured`，
/// 绝不猜测任何远端端点（方案 §7.2/§17.3）。
///
/// # Parameters
/// 参数
/// `app` publishes the workbench and `state` owns the injected sync service and runtime.
/// `app` 发布工作台，`state` 持有注入的同步服务与运行时。
///
/// # Returns
/// 返回值
/// Provider-scoped finalized reports, or a bounded batch failure when shared authority is uncertain.
/// 返回逐供应商已完成报告，或共享权威不确定时的有界批次失败。
pub(crate) async fn service_sync_provider_manifests(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<Vec<crate::provider_manifest_sync::ProviderSyncReport>, String> {
    let (runtime, _initial_configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    // The injected sync service is reused instead of constructing an empty-key/None-URL
    // service per RPC call (plan §R7.13).
    // 复用注入的同步服务，而不是每次 RPC 调用构造空密钥/None URL 的服务（方案 §R7.13）。
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    // Keep the single-writer guard through projection/compensation; releasing it after the
    // download would allow another trigger to replace the staged manifest before this call
    // publishes its matching catalogs and report.
    // 持有单写者守卫直到投影/补偿完成；若下载后立即释放，另一个触发器可能在本次调用
    // 发布匹配目录与报告前替换暂存清单。
    let _activation_guard = service.begin_activation_transaction().await;
    // providers includes every identity pinned by the trusted bundled index, including a
    // provider-local failed baseline that still requires periodic recovery checks.
    // providers 包含可信内置 Index 固定的全部身份，包括仍需周期恢复检查的逐供应商失败基线。
    let providers = runtime
        .system_manifest_provider_ids()
        .map_err(bounded_provider_manifest_rpc_error)?
        .into_iter()
        .collect::<Vec<_>>();
    // syncAllWithReportHandler fetches the signed Index exactly once, but completes each provider
    // before the next candidate is staged. This preserves both shared-index efficiency and the
    // independent transaction requirement (plan §17.4/§R7.7).
    // syncAllWithReportHandler 只获取一次签名 Index，但会在暂存下一候选前完成当前供应商；
    // 这样同时保留共享 Index 效率与独立事务要求（方案 §17.4/§R7.7）。
    // reportsResult retains top-level Index/network failures long enough to persist a classified
    // last-checked failure for every targeted provider.
    // reportsResult 保留顶层 Index/网络失败，以便为每个目标供应商持久化分类最近检查失败。
    let reports = match service
        .sync_all_with_report_handler(runtime.as_ref(), &providers, |report| {
            // Each future owns stable clones so it can await configuration and projection locks
            // without borrowing the synchronization loop's stack frame.
            // 每个 Future 持有稳定克隆，因此可等待配置与投影锁，而不借用同步循环栈帧。
            let app = app.clone();
            let state = state.clone();
            let runtime = Arc::clone(&runtime);
            let service = service.clone();
            async move {
                finalize_provider_sync_report(&app, &state, runtime.as_ref(), &service, report)
                    .await
            }
        })
        .await
    {
        Ok(reports) => reports,
        Err(crate::provider_manifest_sync::ProviderSyncBatchFailure::Remote(primary)) => {
            // checkStateResult is independent observability compensation and never hides primary.
            // checkStateResult 是独立可观测性补偿且绝不隐藏主错误。
            let check_state_result =
                service.record_sync_check_failures(&providers, "index_check_failed");
            let primary =
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(primary.as_str());
            return if check_state_result.is_err() {
                Err(format!(
                    "Provider manifest synchronization failed: {primary} Check-state publication also failed."
                ))
            } else {
                Err(format!(
                    "Provider manifest synchronization failed: {primary}"
                ))
            };
        }
        Err(crate::provider_manifest_sync::ProviderSyncBatchFailure::Projection(primary)) => {
            // The provider handler already attempted exact compensation and provider-scoped
            // failed-check publication; previously committed providers remain active.
            // 逐供应商处理器已经尝试精确补偿与供应商级失败检查发布；此前已提交供应商保持活动。
            return Err(
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(primary.as_str()),
            );
        }
    };
    Ok(reports)
}

/// Returns the typed status snapshot for every system provider manifest (plan §R10).
/// 返回全部系统供应商清单的 typed 状态快照（方案 §R10）。
///
/// # Parameters
/// 参数
/// `state` owns the runtime and sync service whose paired generation is observed.
/// `state` 持有要观测配对代际的运行时与同步服务。
///
/// # Returns
/// 返回值
/// A typed `Vec<ProviderManifestStatus>` instead of a temporary JSON value.
/// 返回 typed `Vec<ProviderManifestStatus>`，而不是临时 JSON 值。
pub(crate) async fn service_get_provider_manifest_status(
    state: SharedServiceBridgeState,
) -> Result<Vec<crate::provider_manifest_sync::ProviderManifestStatus>, String> {
    let (runtime, _configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    // activationGuard prevents the status reader from mixing store layers with a runtime
    // generation while synchronization or rollback is between their publication steps.
    // activationGuard 防止状态读取者在同步或回滚的发布步骤之间混合存储层与运行时代际。
    let _activation_guard = service.begin_activation_transaction().await;
    // configurationGuard closes the independent race where a settings transaction installs a
    // new configuration before publishing its matching model-catalog generation.
    // configurationGuard 关闭设置事务先安装新配置、后发布配对模型目录代际的独立竞争窗口。
    let _configuration_guard = begin_public_ai_configuration_transaction().await;
    // Read account membership under the same guards used to assemble manifest diagnostics.
    // 在组装清单诊断的同一组保护锁内读取账号归属。
    let configuration = runtime
        .configuration_snapshot()
        .map_err(bounded_provider_manifest_rpc_error)?;
    // Use the effective manifest policy, including any reviewed bundled restriction on cached layers.
    // 使用生效清单策略，包括对缓存层生效的随包限制。
    let providers = runtime
        .effective_providers(&configuration)
        .map_err(bounded_provider_manifest_rpc_error)?;
    service
        .provider_manifest_status_all(runtime.as_ref())
        .map(|mut statuses| {
            statuses.retain(|status| {
                providers
                    .provider(&status.provider_id)
                    .is_none_or(|provider| {
                        provider.credential_directory_visibility
                            != CredentialDirectoryVisibility::ExistingAccountsOnly
                            || configuration
                                .connection_graph
                                .has_provider_connection(&provider.id)
                    })
            });
            statuses
        })
        .map_err(bounded_provider_manifest_rpc_error)
}

/// Previews one signed provider manifest candidate without mutating any local layer or state.
/// 预览一个签名供应商 Manifest 候选，且不修改任何本地层或状态。
///
/// # Parameters
/// 参数
/// `state` owns the immutable runtime and configured sync service; `provider_id` is exact.
/// `state` 持有不可变运行时与已配置同步服务；`provider_id` 为精确标识。
///
/// # Returns
/// 返回值
/// A verified typed semantic diff, or a controlled signed-index/network failure.
/// 返回已验证的类型化语义差异，或受控签名 Index/网络失败。
pub(crate) async fn service_preview_provider_manifest(
    state: SharedServiceBridgeState,
    provider_id: String,
) -> Result<crate::provider_manifest_sync::ProviderManifestCandidatePreview, String> {
    // runtime is the single immutable comparison authority captured for this preview.
    // runtime 是为本次预览捕获的唯一不可变比较权威。
    let (runtime, _configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    crate::provider_manifest_sync::require_trusted_system_manifest_provider(
        runtime.as_ref(),
        provider_id.as_str(),
    )
    .map_err(bounded_provider_manifest_rpc_error)?;
    // service reuses the exact production trust store and secure HTTP client.
    // service 复用精确的生产信任存储与安全 HTTP 客户端。
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    // baseline is captured while both writers are excluded, then the guards are released before
    // remote I/O so a slow preview cannot block activation or settings changes.
    // baseline 在排除两类写者时捕获，随后在远端 I/O 前释放守卫，避免慢速预览阻塞激活或设置变更。
    let baseline = {
        let _activation_guard = service.begin_activation_transaction().await;
        let _configuration_guard = begin_public_ai_configuration_transaction().await;
        service
            .capture_preview_baseline(runtime.as_ref(), provider_id.as_str())
            .map_err(|error| {
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
            })?
    };
    service
        .preview_provider_manifest(baseline, provider_id.as_str())
        .await
        .map_err(|error| {
            crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
        })
}

/// Rolls back one provider's active manifest to its previous layer (plan §R10).
/// 把一个供应商的 active 清单回滚到 previous 层（方案 §R10）。
///
/// # Parameters
/// 参数
/// `app` publishes the workbench; `state` owns the sync service; `provider_id` selects one trusted system provider.
/// `app` 发布工作台；`state` 持有同步服务；`provider_id` 选择一个可信系统供应商。
///
/// # Returns
/// 返回值
/// A typed rollback report with the restored revision.
/// 返回带恢复修订号的 typed 回滚报告。
pub(crate) async fn service_rollback_provider_manifest(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_id: String,
) -> Result<crate::provider_manifest_sync::ProviderRollbackReport, String> {
    let (runtime, _initial_configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    crate::provider_manifest_sync::require_trusted_system_manifest_provider(
        runtime.as_ref(),
        provider_id.as_str(),
    )
    .map_err(bounded_provider_manifest_rpc_error)?;
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    let _activation_guard = service.begin_activation_transaction().await;
    // configuration_transaction serializes rollback publication with every settings mutation.
    // configuration_transaction 把回滚发布与每次设置修改串行化。
    let configuration_transaction = begin_public_ai_configuration_transaction().await;
    // configuration is the latest durable/runtime snapshot at the rollback activation boundary.
    // configuration 是回滚激活边界上的最新持久化与运行时快照。
    let configuration = runtime
        .configuration_snapshot()
        .map_err(bounded_provider_manifest_rpc_error)?;
    let report = service
        .rollback_provider_manifest(runtime.as_ref(), &provider_id)
        .map_err(|error| {
            crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
        })?;
    // Reconcile the exact rollback generation before validating its model catalog.
    // 在校验模型目录前，按照精确的回滚代际清理模型归属。
    let Some(manifest) = report
        .activation_snapshot
        .staged_manifest_mutation
        .staged_manifest(&provider_id)
    else {
        service.abort_rollback(runtime.as_ref(), &report)?;
        return Err(String::from("Staged rollback manifest is missing."));
    };
    let mut candidate_configuration = configuration.clone();
    let model_reconciliation_notices =
        reconcile_signed_models_and_record_notices(&mut candidate_configuration, &manifest);
    // Both synchronous publication callbacks share the persistence compensation state.
    // 两个同步发布回调共享持久化补偿状态。
    let configuration_persistence_attempted = std::sync::atomic::AtomicBool::new(false);
    let result = service
        .finalize_rollback_projection(
            runtime.as_ref(),
            &candidate_configuration,
            report,
            |commit| {
                publish_public_ai_workbench_with_commit(
                    &app,
                    &state,
                    runtime.as_ref(),
                    &candidate_configuration,
                    || {
                        commit()?;
                        if candidate_configuration != configuration {
                            configuration_persistence_attempted.store(true, std::sync::atomic::Ordering::Relaxed);
                            vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                                &candidate_configuration,
                                &crate::service_state_access::public_ai_configuration_app_data_root(),
                            )?;
                        }
                        Ok(())
                    },
                )
                .map(|_| ())
            },
            || {
                if configuration_persistence_attempted.load(std::sync::atomic::Ordering::Relaxed) {
                    vulcan_provider_config::config::save_public_ai_connections_with_custom_manifests(
                        &configuration,
                        &crate::service_state_access::public_ai_configuration_app_data_root(),
                    )?;
                }
                publish_public_ai_workbench(&app, &state, runtime.as_ref(), &configuration)
                    .map(|_| ())
            },
        )
        .await;
    if result.is_ok()
        && !model_reconciliation_notices.is_empty()
        && let Ok(mut guard) = state.lock()
    {
        guard
            .startup
            .model_reconciliation_notices
            .extend(model_reconciliation_notices);
        let startup = guard.startup.clone();
        drop(guard);
        crate::emit_service_event_with_trace(
            &app,
            "startup_snapshot_emit",
            vulcan_service_api::ServiceEvent::Startup(Box::new(startup)),
        );
    }
    drop(configuration_transaction);
    result.map_err(|error| {
        crate::provider_manifest_sync::bounded_public_manifest_diagnostic(error.as_str())
    })
}

/// Synchronizes exactly one provider through the reviewed production flow (plan §R10).
/// 通过已审核生产流程同步精确一个供应商（方案 §R10）。
///
/// # Parameters
/// 参数
/// `app` publishes the workbench; `state` owns the sync service; `provider_id` selects one trusted system provider.
/// `app` 发布工作台；`state` 持有同步服务；`provider_id` 选择一个可信系统供应商。
///
/// # Returns
/// 返回值
/// A typed sync report scoped to the requested provider.
/// 返回限定到所请求供应商的 typed 同步报告。
pub(crate) async fn service_sync_provider_manifest(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    provider_id: String,
) -> Result<crate::provider_manifest_sync::ProviderSyncReport, String> {
    let (runtime, _initial_configuration) =
        public_ai_runtime_and_configuration(&state).map_err(bounded_provider_manifest_rpc_error)?;
    crate::provider_manifest_sync::require_trusted_system_manifest_provider(
        runtime.as_ref(),
        provider_id.as_str(),
    )
    .map_err(bounded_provider_manifest_rpc_error)?;
    let service = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        guard
            .provider_manifest_sync
            .clone()
            .ok_or_else(|| String::from("Provider manifest sync service is not ready."))?
    };
    let _activation_guard = service.begin_activation_transaction().await;
    // providerIds retains the exact single target for classified top-level failure bookkeeping.
    // providerIds 保留用于分类顶层失败簿记的精确单一目标。
    let provider_ids = vec![provider_id.clone()];
    // report records a checked failure even when Index verification or network I/O returns Err.
    // report 即使 Index 校验或网络 I/O 返回 Err 也会记录已检查失败。
    let report = match service.sync_one(runtime.as_ref(), &provider_id).await {
        Ok(report) => report,
        Err(primary) => {
            // failureState persists only a stable classification, never the URL-bearing diagnostic.
            // failureState 仅持久化稳定分类，绝不持久化可能含 URL 的诊断。
            let failure_state =
                service.record_sync_check_failures(&provider_ids, "index_check_failed");
            let primary =
                crate::provider_manifest_sync::bounded_public_manifest_diagnostic(primary.as_str());
            return if failure_state.is_err() {
                Err(format!(
                    "Provider manifest synchronization failed: {primary} Check-state publication also failed."
                ))
            } else {
                Err(format!(
                    "Provider manifest synchronization failed: {primary}"
                ))
            };
        }
    };
    finalize_provider_sync_report(&app, &state, runtime.as_ref(), &service, report)
        .await
        .map_err(bounded_provider_manifest_rpc_error)
}

#[cfg(test)]
mod tests;
