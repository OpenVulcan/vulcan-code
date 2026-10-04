//! Public remote launch cancellation and executor loss preserve the original resource receipt.
//! 公开远程启动取消及执行器销毁保留原资源回执。

use super::*;
use crate::remote_test_support::{HeldHttpServer, OBSERVE, host_lease};
use std::fs::TryLockError;

/// Creates the declared remote variant without probing or switching protocols.
/// 创建声明远程变体，不探测或切换协议。
/// Uri is the exact listener; legacy selects SSE versus HTTP; returns the frozen launch specification.
/// uri 是精确监听端点；legacy 选择 SSE 或 HTTP；返回冻结启动规格。
fn remote_spec(uri: String, legacy: bool) -> McpTransportLaunchSpec {
    if legacy {
        McpTransportLaunchSpec::Sse {
            endpoint: "remote-resource-test".into(),
            uri,
            bearer_token: None,
            headers: Default::default(),
            oauth: None,
        }
    } else {
        McpTransportLaunchSpec::Http {
            endpoint: "remote-resource-test".into(),
            uri,
            bearer_token: None,
            headers: Default::default(),
            oauth: None,
        }
    }
}

/// Cancelling public initialization releases the exact listener/client lease despite missing headers.
/// 即使响应头缺失，取消公开初始化也释放精确监听器及客户端租约。
/// No inputs; tests both declared remote protocols against actual held response sockets.
/// 无输入；以实际阻挡响应套接字测试两种声明远程协议。
#[tokio::test]
async fn cancelled_remote_initialization_releases_actual_resources() {
    for legacy in [false, true] {
        // The server reports request arrival but intentionally never sends response headers.
        // 服务器报告请求到达，但故意始终不发送响应头。
        let mut server = HeldHttpServer::start(None).await;
        let (witness, resources, release) = host_lease();
        let specification = remote_spec(server.uri.clone(), legacy);
        let initializing =
            tokio::spawn(async move { specification.launch_with_resources(None, resources).await });
        server.request_line().await;
        assert!(matches!(
            witness.as_file().try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        initializing.abort();
        assert!(
            initializing
                .await
                .err()
                .expect("initialization must remain pending")
                .is_cancelled()
        );
        tokio::time::timeout(OBSERVE, release.wait())
            .await
            .expect("cancelled remote release deadline")
            .expect("original remote release");
        witness
            .as_file()
            .try_lock()
            .expect("cancelled launch released its real host lease");
    }
}

/// The original release receipt remains usable when the entire initiating executor is destroyed.
/// 整个发起执行器销毁后，原释放回执仍可使用。
/// No inputs; a second executor observes actual host lease release for HTTP and SSE.
/// 无输入；第二个执行器观察 HTTP 及 SSE 实际宿主租约释放。
#[test]
fn executor_loss_releases_remote_initializing_resources() {
    for legacy in [false, true] {
        // Keep the independent lease witness and receipt outside the doomed executor.
        // 将独立租约见证及回执保留在即将销毁执行器之外。
        let (witness, resources, release) = host_lease();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("initializing remote runtime");
        // The actual initialization cannot finish because the real server withholds headers.
        // 真实服务器阻挡响应头，实际初始化无法结束。
        let (server, initializing) = runtime.block_on(async {
            let mut server = HeldHttpServer::start(None).await;
            let specification = remote_spec(server.uri.clone(), legacy);
            let initializing =
                tokio::spawn(
                    async move { specification.launch_with_resources(None, resources).await },
                );
            server.request_line().await;
            (server, initializing)
        });
        assert!(matches!(
            witness.as_file().try_lock(),
            Err(TryLockError::WouldBlock)
        ));
        drop(initializing);
        drop(runtime);
        // No work from the destroyed executor can execute on this observer runtime.
        // 已销毁执行器的工作无法在此观察执行器执行。
        let observer = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("remote release observer");
        observer.block_on(async {
            tokio::time::timeout(OBSERVE, release.wait())
                .await
                .expect("executor loss release deadline")
                .expect("original executor loss receipt");
        });
        witness
            .as_file()
            .try_lock()
            .expect("executor loss released its real host lease");
        drop(server);
    }
}
