//! Task-local destination isolation for real global Provider configuration save tests.
//! 真实全局 Provider 配置保存测试的任务局部目标隔离。

tokio::task_local! {
    /// Exact private file path owned by the current explicitly scoped save future.
    /// 当前明确设置作用域的保存协程持有的精确私有文件路径。
    pub(crate) static PROVIDER_CONFIGURATION_PATH: std::path::PathBuf;
}

/// Read the current test destination without modifying process environment or peer tasks.
/// 读取当前测试目标，不修改进程环境或其他任务。
/// No input; returns the scoped path or None when no test override was explicitly installed.
/// 无输入；返回作用域路径，未明确安装测试覆盖时返回 None。
pub(crate) fn current_path() -> Option<std::path::PathBuf> {
    PROVIDER_CONFIGURATION_PATH.try_with(Clone::clone).ok()
}

/// Prove the destination is restored after nesting and does not leak to spawned peer tasks.
/// 证明目标在嵌套后恢复，且不会泄漏到派生的其他任务。
/// No input or output; assertions verify the scope lifetime without creating any files.
/// 无输入输出；断言验证作用域生命周期，不创建任何文件。
#[tokio::test]
async fn provider_configuration_path_scope_is_task_local_and_restores_parent() {
    // Both paths are test identities only; this test performs no persistence.
    // 两个路径仅作为测试身份；本测试不执行持久化。
    let parent = std::env::temp_dir().join(format!("provider-scope-{}", uuid::Uuid::new_v4()));
    // A distinct nested destination must not replace the enclosing scope permanently.
    // 不同的嵌套目标不得永久替换外层作用域。
    let child = parent.join("nested.json");
    assert!(current_path().is_none());
    PROVIDER_CONFIGURATION_PATH
        .scope(parent.clone(), async {
            assert_eq!(current_path(), Some(parent.clone()));
            PROVIDER_CONFIGURATION_PATH
                .scope(child.clone(), async {
                    assert_eq!(current_path(), Some(child));
                    assert!(
                        tokio::spawn(async { current_path() })
                            .await
                            .expect("peer task")
                            .is_none()
                    );
                })
                .await;
            assert_eq!(current_path(), Some(parent));
        })
        .await;
    assert!(current_path().is_none());
}
