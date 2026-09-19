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

/// Verifies readiness is read-only and bound to the exact published package generation.
/// 验证就绪检查是只读的，并绑定精确发布的插件包代次。
#[test]
fn user_plugin_environment_requires_explicit_preparation() {
    // temp isolates the package, runtime root, and environment marker.
    // temp 隔离插件包、运行时根目录与环境标记。
    let temp = tempfile::tempdir().expect("create plugin environment fixture");
    // runtime_root is the manager-owned root for this single test generation.
    // runtime_root 是本测试单个代次的管理器自有根目录。
    let runtime_root = temp.path().join("runtime");
    // bundled_root remains empty so the installed user package is selected uniquely.
    // bundled_root 保持空目录，使已安装用户插件得到唯一选取。
    let bundled_root = runtime_root.join("plugins");
    // source_package is the exact approved package later copied to private storage.
    // source_package 是随后复制到私有存储的精确已批准插件包。
    let source_package = temp.path().join("source").join("local-plugin");
    fs::create_dir_all(&bundled_root).expect("create bundled root");
    write_user_plugin_package(&source_package, "local-plugin");
    // manager owns the catalog and immutable environment publication.
    // manager 持有目录与不可变环境发布。
    let manager = test_manager(&runtime_root, bundled_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load initial catalog");
    // preview binds installation to the package content fingerprint.
    // preview 将安装绑定到插件包内容指纹。
    let preview = manager
        .inspect_local_user_plugin(&source_package)
        .expect("inspect package");
    manager
        .install_local_user_plugin(
            &source_package,
            &preview.content_fingerprint,
            Vec::new(),
            false,
        )
        .expect("install package without enabling");
    // disabled is the persisted policy projected into the test catalog.
    // disabled 是投影到测试目录的持久化策略。
    let disabled = BTreeSet::from([String::from("local-plugin")]);
    manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load disabled plugin");
    assert!(
        !manager
            .plugin_environment_ready("local-plugin")
            .expect("inspect missing environment")
    );
    manager
        .prepare_plugin_environment("local-plugin")
        .expect("prepare environment explicitly");
    assert!(
        manager
            .plugin_environment_ready("local-plugin")
            .expect("inspect prepared environment")
    );
}

/// Verifies preparation rejects a published marker when a declared MCP package executable is absent.
/// 验证声明的 MCP 包入口缺失时，即使环境标记已发布，准备过程仍会拒绝该环境。
#[test]
fn user_plugin_environment_preparation_requires_declared_mcp_bin() {
    // temp isolates the candidate package, authorization, and private environment.
    // temp 隔离候选插件包、授权状态与私有环境。
    let temp = tempfile::tempdir().expect("create missing MCP bin fixture");
    // runtime_root is the only manager-owned storage root for this test.
    // runtime_root 是本测试唯一的管理器自有存储根。
    let runtime_root = temp.path().join("runtime");
    // bundled_root is empty so the user-installed package has one unique identity.
    // bundled_root 保持空目录，使用户插件包具有唯一身份。
    let bundled_root = runtime_root.join("plugins");
    // source_package is the exact package inspected before installation.
    // source_package 是安装前检查的精确插件包。
    let source_package = temp.path().join("source").join("local-plugin");
    fs::create_dir_all(&bundled_root).expect("create bundled root");
    write_user_plugin_package(&source_package, "local-plugin");
    // manifest_path selects the only package declaration changed for this negative case.
    // manifest_path 选择本负例唯一修改的插件包声明。
    let manifest_path = source_package.join("plugin.yaml");
    // manifest declares a syntactically valid stdio server with no installed package bin.
    // manifest 声明语法有效但没有实际安装包入口的 stdio 服务。
    let manifest = fs::read_to_string(&manifest_path)
        .expect("read source manifest")
        .replace("install_mode: bundled", "install_mode: registry")
        + "permissions:\n  - process_node\n  - process_session\n  - dependency_registry_network\nmcp:\n  servers:\n    - id: missing-bin\n      name: Missing Bin\n      scope: global\n      transport:\n        type: stdio\n        launcher:\n          type: node_package_bin\n          package: '@playwright/mcp'\n          bin: missing-playwright-mcp-bin\n";
    fs::write(&manifest_path, manifest).expect("write missing-bin manifest");
    // manager owns exact package fingerprints and the isolated execution environment.
    // manager 持有精确插件包指纹与隔离执行环境。
    let manager = test_manager(&runtime_root, bundled_root);
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load initial catalog");
    // preview provides the complete declared permission set for explicit installation.
    // preview 提供显式安装所需的完整声明权限集合。
    let preview = manager
        .inspect_local_user_plugin(&source_package)
        .expect("inspect missing-bin package");
    manager
        .install_local_user_plugin(
            &source_package,
            &preview.content_fingerprint,
            preview.permissions,
            true,
        )
        .expect("install disabled missing-bin package");
    // disabled projects the installed package without starting its MCP process.
    // disabled 投影已安装插件包，但不启动其 MCP 进程。
    let disabled = BTreeSet::from([String::from("local-plugin")]);
    manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load disabled missing-bin package");
    assert!(
        !manager
            .plugin_environment_ready("local-plugin")
            .expect("inspect absent environment")
    );
    // error proves environment preparation and later enable use the same integrity gate.
    // error 证明环境准备与随后启用使用相同的完整性关卡。
    let error = manager
        .prepare_plugin_environment("local-plugin")
        .expect_err("reject nonexistent declared MCP package bin");
    assert!(
        error
            .to_string()
            .contains("prepared environment is incomplete")
    );
    assert!(
        !manager
            .plugin_environment_ready("local-plugin")
            .expect("inspect incomplete environment")
    );
}
