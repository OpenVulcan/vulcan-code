//! Shared real published startup inputs derive from the existing native candidate fixture without retaining its request.
//! 共享真实已发布启动输入从既有原生候选夹具派生，不保留其请求。

use super::*;
use crate::mcp::{PreparedPublishedPluginMcpConfigs, published_global_mcp_server_configs};
use vulcan_plugin_runtime::VulcanPluginRegisteredNativeMcpHostAuthority;

/// One actual original publication with sealed startup inputs and no candidate request owner.
/// 单个实际原发布，具有密封启动输入且没有候选请求所有者。
pub(in crate::mcp) struct PublishedFixture {
    /// Same real manager issues the source and its native host registration.
    /// 同一真实管理器签发来源及其原生宿主注册。
    pub(in crate::mcp) manager: VulcanPluginRuntimeManager,
    /// Definitions are inseparable from their actual original package and environment ownership.
    /// 定义与其实际原插件包及环境所有权不可分离。
    pub(in crate::mcp) configs: PreparedPublishedPluginMcpConfigs,
    /// The first original registered host is cloned for subsequent same-issuer validation.
    /// 首个原已注册宿主被克隆，以供后续同签发者校验。
    pub(in crate::mcp) authority: VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Actual package and credential namespaces supply independent durable ownership.
    /// 实际插件包及凭据命名空间提供独立持久所有权。
    pub(in crate::mcp) store: VulcanPluginGenerationStore,
    /// Only the actual Node child writes complete startup evidence to this path.
    /// 仅实际 Node 子进程向此路径写入完整启动证据。
    pub(in crate::mcp) marker: PathBuf,
    /// Exact environment root is returned by production, not derived from guessed layout components.
    /// 精确环境根由生产返回，不从猜测布局组件派生。
    pub(in crate::mcp) environment: PathBuf,
    /// Actual paths outlive every original resource and observation in a test.
    /// 实际路径跨测试每个原资源及观察存续。
    pub(in crate::mcp) _temp: tempfile::TempDir,
}

/// Prepare original native definitions for mode after explicitly releasing and aborting the unused candidate.
/// 显式释放及中止未使用候选之后，为 mode 准备原原生定义。
/// Return the real published source with exactly one original issuer and no fabricated native receipt.
/// 返回真实已发布来源，具有恰好一个原签发者且没有伪造原生回执。
pub(in crate::mcp) fn published_fixture(mode: &str) -> PublishedFixture {
    published_fixture_with_scope(mode, VulcanPluginMcpScopeManifest::Global)
}

/// Prepare actual sealed published inputs from a manifest whose explicit scope was selected before immutable copying.
/// 从不可变复制前已选定显式作用域的清单，准备实际密封已发布输入。
/// mode supplies protocol behavior and scope supplies declaration ownership; returns the original source with no request owner.
/// mode 提供协议行为，scope 提供声明所有权；返回没有请求所有者的原来源。
pub(in crate::mcp) fn published_fixture_with_scope(
    mode: &str,
    scope: VulcanPluginMcpScopeManifest,
) -> PublishedFixture {
    // Reuse the only actual binary packaging and configured activation producer used by candidate tests.
    // 复用候选测试使用的唯一实际二进制打包及已配置激活生产者。
    let Fixture {
        prepared,
        store,
        transaction,
        manager,
        marker,
        _temp,
    } = fixture_with_scope(mode, scope);
    // Save the actual original request identity before releasing its foreground and prepared owners.
    // 释放前台及已准备所有者之前，保存实际原请求身份。
    let request_id = transaction.record().request_id();
    drop(prepared);
    drop(transaction);
    store
        .abort_activation_request("bound-plugin", request_id)
        .expect("close unused original candidate");
    // The full selected revision comes from the actual publication rather than its candidate successor.
    // 完整选定修订来自实际发布，而非其候选后继。
    let selected = store
        .activation("bound-plugin")
        .expect("actual original selection")
        .expect("published source");
    // This backend reads only the genuinely staged original credential namespace.
    // 此后端仅读取真实暂存的原凭据命名空间。
    let secrets = RevisionSecrets {
        labels: BTreeMap::from([(
            selected
                .credential_revision()
                .expect("managed credential revision")
                .clone(),
            "first",
        )]),
        reads: Mutex::new(Vec::new()),
    };
    // Production returns the exact environment root for later real deletion-exclusion checks.
    // 生产返回精确环境根，以供后续真实删除互斥检查。
    let environment = manager
        .prepare_plugin_environment("bound-plugin")
        .expect("actual original environment");
    // The manager registers one actual host incarnation for this source.
    // 管理器为此来源注册单个实际宿主实例。
    let authority = manager.register_native_mcp_host_adapter();
    // The original immutable current snapshot supplies all complete source validation.
    // 原不可变当前快照提供全部完整来源校验。
    let snapshot = manager.snapshot();
    // Actual published resource preparation precedes sealed native materialization.
    // 实际已发布资源准备先于密封原生物化。
    let owner = manager
        .prepare_published_mcp_resources(&snapshot, "bound-plugin", &authority)
        .expect("actual published source");
    // The sealed pair preserves the actual original route and selected public child inputs.
    // 密封组合保留实际原路由及选定公开子进程输入。
    let configs = published_global_mcp_server_configs(owner, &authority, &secrets)
        .expect("actual published definitions");
    PublishedFixture {
        manager,
        configs,
        authority,
        store,
        marker,
        environment,
        _temp,
    }
}
