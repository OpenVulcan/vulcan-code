//! Independent file handles test real environment ownership rather than bookkeeping counters.
//! 独立文件句柄测试真实环境所有权，而非记账计数器。

use super::*;
use crate::VulcanPluginRuntimeManager;

/// Creates an engine-free manager whose environment directory belongs solely to this test.
/// 创建无引擎管理器，其环境目录仅属于本测试。
/// Root selects isolated host paths; returns the ordinary production manager.
/// 根选择隔离宿主路径；返回普通生产管理器。
fn manager(root: &Path) -> VulcanPluginRuntimeManager {
    VulcanPluginRuntimeManager::without_runtime(
        root.join("bundled"),
        root.join("user"),
        root.join("bridge"),
        root.join("distribution"),
        root.join("state"),
        root.join("environments"),
    )
}

/// Proves two real shared files exclude cleanup and unrelated plugin identities remain independent.
/// 证明两个真实共享文件排除清理，且无关插件身份保持独立。
/// No inputs; checks both lock directions and preservation of the permanent lock entry.
/// 无输入；检查两种锁方向及永久锁条目的保留。
#[test]
fn environment_leases_exclude_cleanup_across_managers() {
    // Each manager independently opens the same isolated filesystem root.
    // 每个管理器独立打开相同隔离文件系统根。
    let temp = tempfile::tempdir().expect("lease fixture root");
    // The first manager owns the initial reader.
    // 第一个管理器拥有初始读取者。
    let first = manager(temp.path());
    // A separate manager cannot bypass the actual lock using separate in-memory state.
    // 独立管理器不能通过独立内存状态绕过实际锁。
    let second = manager(temp.path());
    // Two independently opened handles must coexist in shared mode.
    // 两个独立打开的句柄必须以共享模式共存。
    let one = retain_plugin_execution_environment(&first.state, "leased").expect("first reader");
    // The second shared handle must remain authoritative after the first drops.
    // 第一个释放后，第二个共享句柄仍须具有权威。
    let two = retain_plugin_execution_environment(&second.state, "leased").expect("second reader");
    assert!(lock_plugin_execution_environment_cleanup(&second.state, "leased").is_err());
    // Another plugin identity has its own independent exclusion interval.
    // 另一插件身份拥有独立排他区间。
    let unrelated = lock_plugin_execution_environment_cleanup(&second.state, "unrelated")
        .expect("unrelated plugin cleanup");
    drop(unrelated);
    drop(one);
    assert!(lock_plugin_execution_environment_cleanup(&first.state, "leased").is_err());
    drop(two);
    // Deletion ownership excludes new readers until that same file is released.
    // 删除所有权在同一文件释放前排除新读取者。
    let cleanup = lock_plugin_execution_environment_cleanup(&second.state, "leased")
        .expect("cleanup after final reader");
    assert!(retain_plugin_execution_environment(&first.state, "leased").is_err());
    drop(cleanup);
    // Reopening must reuse the permanent lock entry after all preceding owners have exited.
    // 全部前驱所有者退出后，重新打开必须复用永久锁条目。
    let later =
        retain_plugin_execution_environment(&first.state, "leased").expect("reader after cleanup");
    assert!(
        first
            .state
            .plugin_runtime_root
            .join(LEASE_DIRECTORY)
            .join("leased.lock")
            .is_file()
    );
    drop(later);
}

/// Rejects traversal and incompatible directory/file entries instead of bypassing their lock identity.
/// 拒绝路径穿越及类型不兼容的目录／文件条目，不绕过其锁身份。
/// No inputs; exercises invalid identities and two independently corrupted filesystem entries.
/// 无输入；验证无效身份及两个独立损坏的文件系统条目。
#[test]
fn environment_leases_reject_invalid_paths_and_types() {
    // Corruptions remain inside a test-owned temporary root.
    // 损坏保持在测试自有临时根内。
    let temp = tempfile::tempdir().expect("invalid lease fixture root");
    // The production manager supplies the authoritative environment root.
    // 生产管理器提供权威环境根。
    let manager = manager(temp.path());
    assert!(retain_plugin_execution_environment(&manager.state, "../escape").is_err());
    assert!(!manager.state.plugin_runtime_root.exists());
    fs::create_dir_all(&manager.state.plugin_runtime_root).expect("create environment root");
    // Both invalid entries deliberately occupy the sole declared lock location.
    // 两个无效条目有意占用唯一声明的锁位置。
    let directory = manager.state.plugin_runtime_root.join(LEASE_DIRECTORY);
    fs::write(&directory, "not a directory").expect("write invalid directory entry");
    assert!(retain_plugin_execution_environment(&manager.state, "leased").is_err());
    fs::remove_file(&directory).expect("remove invalid directory entry");
    fs::create_dir(&directory).expect("create valid lease directory");
    fs::create_dir(directory.join("leased.lock")).expect("create invalid lock file entry");
    assert!(retain_plugin_execution_environment(&manager.state, "leased").is_err());
}
