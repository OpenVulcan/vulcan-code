//! Task-local filesystem isolation for real public-AI configuration save tests.
//! 真实公共 AI 配置保存测试的任务局部文件系统隔离。

tokio::task_local! {
    /// Exact private configuration root owned by the currently scoped test future.
    /// 当前测试协程作用域持有的精确私有配置根目录。
    pub(crate) static PUBLIC_AI_CONFIGURATION_ROOT: std::path::PathBuf;
}

/// Resolve an explicitly scoped test root without changing process environment or peer tasks.
/// 解析明确设置的测试作用域根目录，不改变进程环境或其他任务。
/// No input; returns the current test root, or None outside an explicitly isolated save test.
/// 无输入；返回当前测试根目录，未进入明确隔离保存测试时返回 None。
pub(crate) fn current_root() -> Option<std::path::PathBuf> {
    PUBLIC_AI_CONFIGURATION_ROOT.try_with(Clone::clone).ok()
}

/// Verify that a scoped override neither leaks into spawned tasks nor survives scope completion.
/// 验证作用域覆盖既不泄漏到派生任务，也不在作用域结束后残留。
/// No input or output; assertions exercise the asynchronous task-local isolation contract.
/// 无输入输出；断言验证异步任务局部隔离契约。
#[tokio::test]
async fn configuration_root_scope_does_not_leak_to_peer_tasks() {
    // No file is created; the path only identifies this independently scoped test.
    // 不创建文件；路径仅标识这个独立作用域测试。
    let root =
        std::env::temp_dir().join(format!("public-ai-config-scope-{}", uuid::Uuid::new_v4()));
    assert!(current_root().is_none());
    PUBLIC_AI_CONFIGURATION_ROOT
        .scope(root.clone(), async {
            assert_eq!(current_root(), Some(root));
            assert!(
                tokio::spawn(async { current_root() })
                    .await
                    .expect("peer task")
                    .is_none()
            );
        })
        .await;
    assert!(current_root().is_none());
}
