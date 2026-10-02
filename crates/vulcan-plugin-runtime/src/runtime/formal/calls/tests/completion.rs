//! Actual original supervision completion wakes observers without releasing unretired work.
//! 真实原监督完成唤醒观测者，不释放未退役工作。

use super::*;

/// Run real lifecycle work with live and cancelled callers; verify unpolled notifications retain actual completion.
/// 使用活动与已取消调用者运行真实生命周期工作；验证未轮询通知保留真实完成。
/// Returns only after original supervision, physical retirement and acknowledged registry reclamation complete.
/// 仅在原监督、物理退役及已确认注册表回收完成之后返回。
#[test]
fn formal_completion_wakes_original_observers_after_supervision_and_retirement() {
    for cancel_waiter in [false, true] {
        // Own the existing real package, manager and native callback recorder.
        // 持有既有真实包、管理器及原生回调记录器。
        let fixture = Fixture::new();
        fixture
            .manager
            .begin_enabling("hook-plugin")
            .expect("begin actual lifecycle enabling");
        // Keep the same actual executor independent from the core's original native work.
        // 保持同一实际执行器，独立于核心原生工作。
        let runtime = executor();
        // Retain the original manager through the public lifecycle call.
        // 在公开生命周期调用期间保留原管理器。
        let manager = fixture.manager.clone();
        // This original waiter never fabricates a result or completes another caller's work.
        // 此原等待者绝不伪造结果，也不完成其他调用者的工作。
        let mut waiter = runtime
            .spawn(async move { manager.invoke_lifecycle("hook-plugin", Event::Enable).await });
        runtime.block_on(async {
            fixture.entered().await;
            // Borrow the exact registered call whose actual native callback is still blocked.
            // 借用真实原生回调仍被阻塞的精确已登记调用。
            let original = fixture.admitted().await;
            assert!(original.lock().expect("active original state").supervised);
            assert!(!original.lock().expect("active original state").retired);
            // Both futures exist before release; one remains unpolled when completion may be published.
            // 两个未来都在释放之前存在；完成可能发布时，其中一个尚未被轮询。
            let first = original.finished.notified();
            // Broadcast completion must also reach this independent existing observer.
            // 广播完成也必须到达此独立既有观测者。
            let second = original.finished.notified();
            if cancel_waiter {
                waiter.abort();
                assert!(
                    (&mut waiter)
                        .await
                        .expect_err("original waiter was cancelled")
                        .is_cancelled()
                );
            }
            fixture.events.release();
            tokio::time::timeout(WAIT, async {
                first.await;
                second.await;
            })
            .await
            .expect("real original supervision wakes both existing observers");
            {
                // Observe actual terminal evidence after notification, without marking it on the test's behalf.
                // 通知之后观察真实终态证据，不代表测试自行标记终态。
                let state = original.lock().expect("completed original state");
                assert!(
                    !state.supervised,
                    "notification cannot precede original supervision end"
                );
                assert!(
                    state.retired,
                    "notification cannot substitute physical retirement"
                );
                assert!(
                    state.result.is_some(),
                    "notification must retain the real result"
                );
            }
            if !cancel_waiter {
                waiter
                    .await
                    .expect("original waiter joined")
                    .expect("actual lifecycle completed");
            }
            fixture.empty().await;
        });
        fixture
            .manager
            .request_formal_runtime_close()
            .expect("close original parent");
        runtime.block_on(async {
            tokio::time::timeout(WAIT, async {
                while !fixture
                    .manager
                    .poll_formal_runtime_closed()
                    .expect("physical parent close")
                {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("original parent physically closed");
        });
    }
}
