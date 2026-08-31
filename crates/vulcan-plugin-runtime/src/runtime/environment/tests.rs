//! Private-environment publication integrity regression.
//! 私有环境发布完整性回归。

use super::super::tests::{test_manager, write_user_plugin_package};
use std::collections::BTreeSet;
use std::fs;

/// Verifies a user package changed after authorization cannot seed an execution environment.
/// 验证授权后发生变化的用户插件包不能生成执行环境。
#[test]
fn user_plugin_environment_rejects_post_authorization_mutation() {
    // temp owns the complete isolated installation and runtime fixture.
    // temp 持有完整的隔离安装与运行时测试环境。
    let temp = tempfile::tempdir().expect("create user plugin mutation directory");
    // runtimeRoot is the deterministic Vulcan runtime root used by the fixture manager.
    // runtimeRoot 是测试管理器使用的确定性 Vulcan 运行时根目录。
    let runtime_root = temp.path().join("runtime");
    // vulcanPluginRoot keeps the bundled root present without adding a competing plugin id.
    // vulcanPluginRoot 保持 Vulcan 插件根存在，同时不加入冲突插件标识。
    let vulcan_plugin_root = runtime_root.join("plugins");
    // sourcePackage is the package generation approved by the installation transaction.
    // sourcePackage 是安装事务批准的插件包代次。
    let source_package = temp.path().join("source").join("local-plugin");
    fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
    write_user_plugin_package(&source_package, "local-plugin");
    // manager owns catalog, authorization, and private environment state for the fixture.
    // manager 持有测试目录、授权与私有环境状态。
    let manager = test_manager(&runtime_root, vulcan_plugin_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load initial plugin catalog");
    manager
        .install_local_user_plugin(
            &source_package,
            &manager
                .inspect_local_user_plugin(&source_package)
                .expect("inspect local user plugin")
                .content_fingerprint,
            Vec::new(),
            false,
        )
        .expect("install local user plugin");
    // disabled keeps the installed package valid without starting its runtime.
    // disabled 让已安装包保持有效，同时不启动其运行时。
    let disabled = BTreeSet::from([String::from("local-plugin")]);
    manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load authorized user plugin generation");
    // fingerprint is the exact authorized generation directory name expected by publication.
    // fingerprint 是发布过程预期使用的精确授权代次目录名。
    let fingerprint = manager.snapshot().plugins["local-plugin"]
        .content_fingerprint
        .clone();
    // installedBundle is deliberately changed after the catalog captured its SHA-256 identity.
    // installedBundle 在目录捕获 SHA-256 身份后被刻意修改。
    let installed_bundle = runtime_root.join("user_plugins/local-plugin/runtime/plugin.bundle.lua");
    fs::write(&installed_bundle, "return { mutated = true }\n")
        .expect("mutate installed plugin bundle");

    // error proves the private copy is rehashed before publication or execution.
    // error 证明私有副本会在发布或执行前重新计算哈希。
    let error = manager
        .prepare_plugin_environment("local-plugin")
        .expect_err("reject changed authorized package");
    assert!(
        error
            .to_string()
            .contains("changed after catalog validation")
    );
    assert!(
        !runtime_root
            .join("plugin_runtime/local-plugin/1.0.0")
            .join(fingerprint)
            .exists()
    );
}
