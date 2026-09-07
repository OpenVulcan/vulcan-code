//! Exact per-command configuration selection independent of another project's active default.
//! 独立于其他项目活跃默认值的逐命令精确配置选择。

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use vulcan_memorymesh_core::*;

/// Creates a valid read-only command; `query` distinguishes calls without changing configuration identity.
/// 创建有效的只读命令；`query` 区分调用但不改变配置身份。
fn search_command(query: &str) -> CoreCommand {
    CoreCommand::MemorySearch(MemorySearchCommand {
        subject_id: SubjectId::new("subject").expect("subject"),
        scope_access: ScopeAccess {
            write_scope: ScopeId::new("scope").expect("scope"),
            visible_scopes: vec![ScopeId::new("scope").expect("scope")],
        },
        query: query.to_owned(),
        limit: 1,
        vector_space_id: VectorSpaceId::new("fixture-vector").expect("vector"),
        boundary: CandidateQueryBoundary {
            candidate_scope: None,
            session_recall_boundary: None,
        },
        now_ns: 10,
    })
}

/// Builds a project policy with a distinctive `dimension`; `name` may later be registered again.
/// 构造具有独特 `dimension` 的项目策略；`name` 可以随后再次注册。
fn bundle(name: &str, dimension: u32) -> ConfigBundle {
    // bundle disables hybrid/rerank so the fixture needs no incidental capabilities.
    // bundle 关闭混合及重排，使夹具无需无关能力。
    let mut bundle = ConfigBundle {
        name: name.to_owned(),
        embedding_dimension: dimension,
        ..Default::default()
    };
    bundle.memory_policy.retrieval_policy.hybrid_enabled = false;
    bundle.memory_policy.retrieval_policy.rerank_top_n = 0;
    bundle
}

/// Records embedding dimensions and returns an empty reference store result without waiting for external services.
/// 记录嵌入维度并返回空参考存储结果，不等待外部服务。
#[derive(Default)]
struct ImmediateHost {
    /// Captures dimensions requested from the frozen snapshot, not from the active default.
    /// 捕获冻结快照请求的维度，而不是活跃默认值的维度。
    dimensions: Vec<u32>,
    /// Owns an empty deterministic store for normal downstream Core reads.
    /// 持有空确定性存储，供普通下游 Core 读取。
    storage: InMemoryStorage,
}

impl AsyncHostDriver for ImmediateHost {
    /// Executes `request` immediately and preserves its ID; returns only typed embedding/storage results.
    /// 立即执行 `request` 并保留其 ID；仅返回强类型嵌入及存储结果。
    fn execute_effect<'a>(
        &'a mut self,
        request: EffectRequest,
    ) -> Pin<Box<dyn Future<Output = Result<EffectResult, CoreError>> + Send + 'a>> {
        Box::pin(async move {
            // result follows the effect variant instead of guessing which call should occur next.
            // result 遵循效果变体，而不是猜测下一次应该发生哪种调用。
            let result = match request.effect {
                HostEffect::Embedding(effect) => {
                    self.dimensions.push(effect.expected_dimension);
                    HostEffectResult::Embedding(EmbeddingBatchResult {
                        vectors: effect
                            .inputs
                            .iter()
                            .map(|_| vec![1.0; effect.expected_dimension as usize])
                            .collect(),
                        result_indices: Vec::new(),
                        dropped: Vec::new(),
                    })
                }
                HostEffect::StorageQuery(query) => {
                    HostEffectResult::StorageQuery(self.storage.query(&query)?)
                }
                _ => panic!("read-only fixture requested an unexpected effect"),
            };
            Ok(EffectResult {
                effect_id: request.effect_id,
                result,
            })
        })
    }
}

/// Verifies old IDs survive named replacement, active defaults stay unchanged, and unknown IDs never fall back.
/// 验证旧 ID 在同名替换后仍有效、活跃默认值不被改变、未知 ID 绝不回退。
#[test]
fn explicit_snapshot_selection_isolated_from_active_and_replaced_registrations() {
    // core holds two projects and a later replacement of the first project's named policy.
    // core 持有两个项目，以及首个项目同名策略的后续替换版本。
    let core = MemoryMeshCore::new().expect("core");
    let first = core.register_config(bundle("project-a", 2)).expect("first");
    let second = core
        .register_config(bundle("project-b", 3))
        .expect("second");
    let replacement = core
        .register_config(bundle("project-a", 4))
        .expect("replacement");
    core.activate_config("project-b")
        .expect("activate other project");
    assert_ne!(first.id, replacement.id);
    // selected is deliberately not the currently active or newest named snapshot.
    // selected 刻意不使用当前活跃或同名最新快照。
    for selected in [&first, &replacement, &second] {
        let (handle, step) = core
            .start_with_snapshot(search_command("frozen policy"), &selected.id)
            .expect("exact start");
        assert_eq!(handle.config_snapshot_id, selected.id);
        let CommandStep::Effects(effects) = step else {
            panic!("embedding expected");
        };
        let HostEffect::Embedding(effect) = &effects[0].effect else {
            panic!("embedding expected");
        };
        assert_eq!(effect.expected_dimension, selected.embedding_dimension);
    }
    assert_eq!(core.active_snapshot().expect("active").id, second.id);
    // host proves the async convenience runner also uses the old requested ID all the way to completion.
    // host 证明异步便捷入口直到完成也使用所请求的旧 ID。
    let mut host = ImmediateHost::default();
    {
        let mut future = Box::pin(core.run_to_completion_with_snapshot_async(
            search_command("complete old policy"),
            &first.id,
            &mut host,
        ));
        let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("immediate fixture must complete");
        };
        assert!(
            matches!(result.expect("completed"), CommandResult::MemorySearch(hits) if hits.is_empty())
        );
    }
    assert_eq!(host.dimensions, vec![2]);
    assert_eq!(
        core.active_snapshot().expect("active unchanged").id,
        second.id
    );
    let missing = ConfigSnapshotId::new("missing-snapshot").expect("snapshot id");
    assert_eq!(
        core.start_with_snapshot(search_command("unknown snapshot"), &missing)
            .expect_err("must not fall back")
            .code,
        ErrorCode::SnapshotNotFound
    );
}
