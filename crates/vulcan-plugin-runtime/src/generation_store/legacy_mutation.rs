//! Legacy mutable configuration and secret writers retain the actual activation namespace fence.
//! 旧格式可变配置及秘密写入者保留实际激活命名空间屏障。

use super::{
    VulcanPluginActivation, VulcanPluginGenerationStore, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result,
};
use std::fs::File;
use std::path::Path;

/// Exclusive original-selection authority retained across a legacy mutation's read, publication and rollback.
/// 旧格式变更的读取、发布及回滚期间保留的排他原选择权威。
/// This owner cannot be cloned, deserialized or reconstructed from a file, UUID or revision number.
/// 此所有者不能被克隆、反序列化，或从文件、UUID 或修订编号重建。
pub struct VulcanPluginLegacyMutationGuard {
    /// The original canonical store authority stays retained without reopening or reconstructing a backend root.
    /// 原规范存储权威保持保留，不重开或重建后端根目录。
    store: VulcanPluginGenerationStore,
    /// Exact validated plugin namespace owns both legacy configuration and OS secret accounts.
    /// 精确已校验插件命名空间同时持有旧格式配置及操作系统秘密账户。
    plugin_id: String,
    /// Complete originally selected activation, with explicit absence only for an unpublished namespace.
    /// 完整原已选激活，仅未发布命名空间使用显式缺失。
    original: Option<VulcanPluginActivation>,
    /// The real exclusive namespace file remains private and locked until this actual owner is dropped.
    /// 真实排他命名空间文件保持私有及加锁，直到此实际所有者释放。
    _namespace: File,
}

impl std::fmt::Debug for VulcanPluginLegacyMutationGuard {
    /// Format the non-secret plugin identity only; formatter receives no configuration, activation or native file details.
    /// 仅格式化无秘密插件身份；formatter 不接收配置、激活或原生文件细节。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginLegacyMutationGuard")
            .field("plugin_id", &self.plugin_id)
            .finish_non_exhaustive()
    }
}

impl VulcanPluginLegacyMutationGuard {
    /// Borrow the complete original selection as inert routing identity; no parameters or execution permission.
    /// 借用完整原选择作为未启动路由身份；无参数，也不授予执行权限。
    /// Returns exact selected metadata, preserving actual absence for an unpublished flat namespace.
    /// 返回精确已选元数据，为未发布平铺命名空间保留实际缺失。
    pub(crate) fn routing_original(&self) -> Option<&VulcanPluginActivation> {
        self.original.as_ref()
    }

    /// Compare this actual borrowed owner with a previously captured weak namespace allocation.
    /// 将此实际借用所有者与先前捕获的弱命名空间分配比较。
    /// `identity` observes the real Arc without upgrading it; returns physical identity only, never permission.
    /// `identity` 观测真实 Arc 而不升级；仅返回物理身份，绝不返回权限。
    pub(crate) fn matches_routing_namespace(&self, identity: &std::sync::Weak<Self>) -> bool {
        std::ptr::eq(self, identity.as_ptr())
    }

    /// Validate literal `user_root` and `plugin_id` against this original guard without constructing storage or acquiring another namespace File.
    /// 根据此原守卫校验字面 `user_root` 及 `plugin_id`，不构造存储或取得另一命名空间 File。
    /// Return success only for the existing canonical root, strict original layout and unchanged complete no-pending selection.
    /// 仅既有规范根、严格原布局及未变完整无待处理选择返回成功。
    pub(crate) fn validate_user_root(&self, user_root: &Path, plugin_id: &str) -> Result<()> {
        if !super::files::exists(user_root, true)? {
            return Err(Error::new(
                Code::Conflict,
                "Legacy owner requires its actual existing user plugin root.",
            ));
        }
        // Reject literal redirects before canonicalization, matching both original store entry contracts.
        // 规范化之前拒绝字面重定向，与两个原存储入口契约一致。
        let canonical_user_root = user_root
            .canonicalize()
            .map_err(|error| Error::io("Resolve user plugin root", error))?;
        if canonical_user_root.join(super::STORE_DIRECTORY) != self.store.root {
            return Err(Error::new(
                Code::Conflict,
                "Legacy owner belongs to another canonical user plugin root.",
            ));
        }
        // Derive forward from the sole layout constant; never infer a user root from a guessed parent or initialize missing children.
        // 从唯一布局常量正向派生；绝不从猜测父目录推断用户根，也不初始化缺失子项。
        self.store.validate_layout()?;
        self.validate_owner(&self.store, plugin_id)
    }

    /// Arm one exact guard's test-only replacement after successful validation; return same-thread scoped cleanup.
    /// 启用单个精确守卫验证成功后的仅测试替换；返回同线程作用域清理。
    /// `bytes` must come from a real valid selection writer in the isolated fixture; no production authority is constructed.
    /// `bytes` 必须来自隔离夹具真实有效选择写入器；不构造生产权威。
    #[cfg(test)]
    pub(crate) fn replace_catalog_selection_after_validation_for_test(
        &self,
        bytes: Vec<u8>,
    ) -> impl Drop {
        catalog_observation::arm(&self.store.root, &self.plugin_id, bytes)
    }

    /// Run the same exact-owner observer after a guarded reader's validation and before its actual second read.
    /// 在受保护读取器验证之后及实际第二次读取之前，运行同一精确所有者观察器。
    /// No parameters; returns the actual fixture write error without constructing another namespace owner.
    /// 无参数；返回实际夹具写入错误，不构造另一个命名空间所有者。
    #[cfg(test)]
    pub(in crate::generation_store) fn observe_catalog_selection_after_validation_for_test(
        &self,
    ) -> Result<()> {
        catalog_observation::after_validation(&self.store.root, &self.plugin_id)
    }

    /// Validate plugin_id against this guard's original retained store without asking a backend to construct storage authority.
    /// 根据此 guard 原保留存储校验 plugin_id，不要求后端构造存储权威。
    /// Return the same exact-owner validation result; no lock is opened and no secret value is accessed.
    /// 返回相同精确所有者校验结果；不打开锁且不访问秘密值。
    pub fn validate_plugin(&self, plugin_id: &str) -> Result<()> {
        self.validate_owner(&self.store, plugin_id)
    }

    /// Validate this retained original authority for store/plugin_id without reopening or replacing its namespace lock.
    /// 校验此保留原权威属于 store／plugin_id，不重开或替换其命名空间锁。
    /// Return success only for the unchanged complete selection without any unfinished activation or restoration request.
    /// 仅完整选择未变且没有未完成激活或恢复请求时返回成功。
    /// Call before each legacy backend mutation and its rollback; errors leave the same physical fence retained.
    /// 每次旧格式后端变更及其回滚前调用；错误保持相同物理屏障持续持有。
    pub fn validate_owner(
        &self,
        store: &VulcanPluginGenerationStore,
        plugin_id: &str,
    ) -> Result<()> {
        if self.store.root != store.root || self.plugin_id != plugin_id {
            return Err(Error::new(
                Code::Conflict,
                "Legacy mutation owner belongs to another store or plugin namespace.",
            ));
        }
        // The sole pending-request contract also includes the host's original restoration handoff fence.
        // 唯一待处理请求契约还包含宿主原恢复交接屏障。
        store.ensure_no_pending_activation_locked(plugin_id)?;
        if store.read_activation(plugin_id)? != self.original {
            return Err(Error::new(
                Code::Conflict,
                "Legacy mutation's complete original activation changed.",
            ));
        }
        Ok(())
    }
}

/// Same-thread original-bound observers reproduce real external replacement between the two catalog reads.
/// 同线程原请求绑定观察器复现两次目录读取之间的真实外部替换。
#[cfg(test)]
mod catalog_observation {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A single isolated original stores actual producer bytes without formatting or serializing secret-bearing state.
    /// 单个隔离原请求存储实际生产者字节，不格式化或序列化可能包含秘密的状态。
    struct Replacement {
        /// Exact canonical root and plugin limit the observer to its real retained namespace.
        /// 精确规范根及插件将观察器限制在其真实保留命名空间。
        key: (PathBuf, String),
        /// Actual valid bytes are consumed once, so retry cannot replay replacement.
        /// 实际有效字节仅消费一次，使重试不能重放替换。
        bytes: Option<Vec<u8>>,
    }

    std::thread_local! {
        /// Test-only state exists solely on the thread owning this original observer.
        /// 仅测试状态仅存在于持有此原观察器的线程。
        static ARMED: std::cell::RefCell<Option<Replacement>> = const { std::cell::RefCell::new(None) };
    }

    /// Thread-bound retirement removes only its matching observer, including during panic unwinding.
    /// 线程绑定退役仅移除匹配观察器，包括 panic 展开期间。
    pub(super) struct Scope {
        /// Exact scope identity prevents older observers from clearing unrelated tests.
        /// 精确作用域身份防止较旧观察器清除无关测试。
        key: (PathBuf, String),
        /// Cleanup may never move away from its original test thread.
        /// 清理绝不能离开原测试线程。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Scope {
        /// Retire this exact observer without touching any actual namespace File or production selection.
        /// 退役此精确观察器，不触及任何实际命名空间 File 或生产选择。
        fn drop(&mut self) {
            ARMED.with(|armed| {
                // The scope preserves every unrelated original observer even if an older guard retires late.
                // 即使较旧守卫延迟退役，作用域仍保留每个无关原观察器。
                let mut armed = armed.borrow_mut();
                if armed
                    .as_ref()
                    .is_some_and(|replacement| replacement.key == self.key)
                {
                    armed.take();
                }
            });
        }
    }

    /// Scope actual producer `bytes` to exact canonical `root` and `plugin_id`; return unwind-safe same-thread retirement.
    /// 将实际生产者 `bytes` 限定为精确规范 `root` 及 `plugin_id`；返回展开安全同线程退役。
    pub(super) fn arm(root: &Path, plugin_id: &str, bytes: Vec<u8>) -> Scope {
        // No file, namespace or production owner is opened by arming the observer.
        // 启用观察器不打开任何文件、命名空间或生产所有者。
        let key = (root.to_path_buf(), plugin_id.to_owned());
        ARMED.with(|armed| {
            // Explicit one-scope ownership rejects accidental overlapping fixture observers.
            // 明确单作用域所有权拒绝意外重叠夹具观察器。
            let mut armed = armed.borrow_mut();
            assert!(armed.is_none(), "catalog replacement scopes overlap");
            *armed = Some(Replacement {
                key: key.clone(),
                bytes: Some(bytes),
            });
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }

    /// Write the one matching original's captured valid selection only after its actual validation succeeded.
    /// 仅在实际验证成功之后，写入单个匹配原请求捕获的有效选择。
    /// Return explicit filesystem failure or success; production builds contain no observer or replacement state.
    /// 返回明确文件系统失败或成功；生产构建不包含观察器或替换状态。
    pub(super) fn after_validation(root: &Path, plugin_id: &str) -> Result<()> {
        // Consume only an exact root/plugin match before executing the isolated real filesystem replacement.
        // 执行隔离真实文件系统替换之前，仅消费精确根、插件匹配。
        let bytes = ARMED.with(|armed| {
            let mut armed = armed.borrow_mut();
            if let Some(replacement) = armed.as_mut()
                && replacement.key.0 == root
                && replacement.key.1 == plugin_id
            {
                replacement.bytes.take()
            } else {
                None
            }
        });
        if let Some(bytes) = bytes {
            std::fs::write(root.join("active").join(format!("{plugin_id}.json")), bytes).map_err(
                |error| Error::io("Controlled actual catalog selection replacement", error),
            )?;
        }
        Ok(())
    }
}

impl VulcanPluginGenerationStore {
    /// Acquire plugin_id's actual exclusive namespace against the complete expected activation and return its typed owner.
    /// 对照完整 expected 激活取得 plugin_id 实际排他命名空间，并返回其类型化所有者。
    /// Expected absence means no published activation, not a guessed revision or a fallback to a newer selection.
    /// 预期缺失表示没有已发布激活，不是猜测修订或回退至较新选择。
    /// Retain this guard through legacy read-before-write, readiness, full publication and any failure rollback.
    /// 在旧格式写前读取、就绪、完整发布及任何失败回滚期间保留此 guard。
    /// This fence never changes secret values, configuration documents, selection records or request journals.
    /// 此屏障绝不变更秘密值、配置文档、选择记录或请求日志。
    pub fn acquire_legacy_mutation_guard(
        &self,
        plugin_id: &str,
        expected: Option<&VulcanPluginActivation>,
    ) -> Result<VulcanPluginLegacyMutationGuard> {
        // Namespace construction validates the literal plugin identity and complete canonical store layout.
        // 命名空间构造校验字面插件身份及完整规范存储布局。
        let namespace = self.namespace_lock(plugin_id, false)?;
        // Reuse the sole authority for pending activation and original restoration requests under this same fence.
        // 在此相同屏障下复用待处理激活及原恢复请求的唯一权威。
        self.ensure_no_pending_activation_locked(plugin_id)?;
        // Read the exact full selected record while every cooperating namespace writer is excluded.
        // 排除每个协作命名空间写入者期间，读取精确完整已选记录。
        let original = self.read_activation(plugin_id)?;
        if original.as_ref() != expected {
            return Err(Error::new(
                Code::Conflict,
                "Legacy mutation requires its complete exact original activation.",
            ));
        }
        Ok(VulcanPluginLegacyMutationGuard {
            store: self.clone(),
            plugin_id: plugin_id.to_owned(),
            original,
            _namespace: namespace,
        })
    }
}

/// Real independent stores and runtime writers exercise the cross-process namespace boundary.
/// 真实独立存储及运行时写入器验证跨进程命名空间边界。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PinnedVulcanPluginGeneration, VulcanPluginConfigurationSelection,
        VulcanPluginCredentialBackend, VulcanPluginCredentialKey, VulcanPluginCredentialRevision,
        VulcanPluginCredentialValues, VulcanPluginRuntimeManager,
    };
    use serde_json::{Value, json};
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    mod user_root;

    /// Write plugin_id's valid remote configuration package beneath root and return its exact source path.
    /// 在 root 下写入 plugin_id 的有效远端配置插件包，并返回其精确来源路径。
    fn write_package(root: &Path, plugin_id: &str) -> PathBuf {
        // The package owns a real object-or-null schema and declares every required capability explicitly.
        // 插件包持有真实对象或空值模式，并明确声明每项所需能力。
        let source = root.join(plugin_id);
        fs::create_dir_all(&source).expect("create isolated legacy configuration package");
        fs::write(source.join("plugin.yaml"), format!("schema_version: 1\nid: {plugin_id}\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Legacy configuration fixture\n  capability: Original mutable configuration\npermissions: [config, network_https]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\nmcp:\n  servers:\n    - id: remote\n      name: Remote\n      transport:\n        type: http\n        url: {{source: literal, value: 'https://example.test/mcp'}}\n")).expect("write valid legacy manifest");
        fs::write(source.join("schema.json"), r#"{"type":["object","null"],"properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#).expect("write actual configuration schema");
        fs::write(source.join("defaults.json"), r#"{"label":"default"}"#)
            .expect("write original package defaults");
        source
    }

    /// Prepare source using its actual inspection digest and return the original physical package lease.
    /// 使用实际检查摘要准备 source，并返回原物理插件包租约。
    fn prepare(store: &VulcanPluginGenerationStore, source: &Path) -> PinnedVulcanPluginGeneration {
        // Production inspection provides the exact authorized digest, never a guessed fixture hash.
        // 生产检查提供精确授权摘要，绝不使用猜测夹具哈希。
        let inspected = crate::inspect_user_plugin_package(source).expect("inspect real package");
        store
            .prepare(source, &inspected.content_fingerprint)
            .expect("prepare actual immutable package")
    }

    /// Create root's real version-two legacy-config selection and return store, package, source and full activation.
    /// 创建 root 真实第二版旧配置选择，并返回存储、插件包、来源及完整激活。
    fn fixture(
        root: &Path,
    ) -> (
        VulcanPluginGenerationStore,
        PinnedVulcanPluginGeneration,
        PathBuf,
        VulcanPluginActivation,
    ) {
        // This root is host-created before the sole store is opened.
        // 打开唯一存储前，此根由宿主创建。
        let user = root.join("user");
        fs::create_dir_all(&user).expect("create host user plugin root");
        // Source bytes stay outside the managed user catalog to avoid a duplicate flat package.
        // 来源字节保持在受管用户目录之外，避免重复平铺插件包。
        let source = write_package(&root.join("source"), "legacy-plugin");
        // The store, generation and selection are produced by the same real production APIs.
        // 存储、代次及选择由相同真实生产接口产生。
        let store = VulcanPluginGenerationStore::open(&user).expect("open original store");
        // Retain actual package ownership through every independent observer's attempt.
        // 在每个独立观察者尝试期间保留实际插件包所有权。
        let package = prepare(&store, &source);
        // Version two has complete approval but deliberately retains the old mutable configuration contract.
        // 第二版具有完整批准，但特意保留旧可变配置契约。
        let original = store
            .activate_for_test(&package, 0, package.required_permissions.clone())
            .expect("publish exact legacy-config activation");
        (store, package, source, original)
    }

    /// Create and refresh root's actual manager without a Lua engine, preserving managed configuration semantics.
    /// 创建并刷新 root 不含 Lua 引擎的实际管理器，保留受管配置语义。
    fn manager(root: &Path) -> VulcanPluginRuntimeManager {
        // Runtime-free remote configuration requires no bundled execution dependency or actual network startup.
        // 无运行时远端配置不需要内置执行依赖或实际网络启动。
        let manager = VulcanPluginRuntimeManager::without_runtime(
            root.join("bundled"),
            root.join("user"),
            root.join("bridge"),
            root.join("distributions"),
            root.join("state"),
            root.join("runtime"),
        );
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("refresh actual legacy configuration catalog");
        manager
    }

    /// A real retained namespace excludes another store's update until the exact guard drops, while peers remain independent.
    /// 真实保留命名空间在精确 guard 释放前排除另一存储更新，同时同伴保持独立。
    #[test]
    fn legacy_mutation_guard_excludes_second_store_until_actual_drop() {
        // Both store objects observe the same canonical files without sharing a Rust mutex.
        // 两个存储对象观察相同规范文件，不共享 Rust 互斥锁。
        let root = tempfile::tempdir().expect("independent store lock fixture");
        // Every comparison uses the original complete selected activation.
        // 每个比较使用原完整已选激活。
        let (store, _package, source, original) = fixture(root.path());
        // This observer independently opens the real store and its underlying OS lock files.
        // 此观察者独立打开真实存储及其底层操作系统锁文件。
        let observer = VulcanPluginGenerationStore::open_existing(&root.path().join("user"))
            .expect("open independent observer")
            .expect("actual store exists");
        // Retain the actual exclusive namespace owner through the simulated publication and rollback window.
        // 在模拟发布及回滚窗口期间保留实际排他命名空间所有者。
        let guard = store
            .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
            .expect("acquire original typed namespace owner");
        guard
            .validate_plugin("legacy-plugin")
            .expect("same original authority");
        guard
            .validate_owner(&observer, "legacy-plugin")
            .expect("same canonical independent store authority");
        assert_eq!(
            observer
                .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
                .expect_err("independent namespace lock must be Busy")
                .code,
            Code::Busy
        );
        // A real update reservation is blocked before any package copying or credential allocation.
        // 真实更新预留在任何插件包复制或凭据分配前被阻挡。
        let request = Uuid::new_v4();
        // Actual inspection provides the frozen source digest for both attempts at this same request.
        // 实际检查为此相同请求的两次尝试提供固定来源摘要。
        let inspection =
            crate::inspect_user_plugin_package(&source).expect("inspect update source");
        assert_eq!(
            observer
                .begin_activation_preparation(
                    request,
                    &source,
                    &inspection.content_fingerprint,
                    Some(&original),
                    vec!["config".into(), "network_https".into()]
                )
                .expect_err("real original update must be Busy during legacy mutation")
                .code,
            Code::Busy
        );
        assert!(!store.root.join("transactions/legacy-plugin").exists());
        // An unrelated plugin can still prepare and publish under its own namespace.
        // 无关插件仍可在自身命名空间下准备及发布。
        let peer = prepare(
            &observer,
            &write_package(&root.path().join("peer"), "peer-plugin"),
        );
        observer
            .activate_for_test(&peer, 0, peer.required_permissions.clone())
            .expect("independent namespace remains available");
        guard
            .validate_plugin("legacy-plugin")
            .expect("peer publication cannot replace original authority");
        drop(guard);
        // Only dropping this actual namespace File permits the original request to begin.
        // 仅释放此实际命名空间 File 才允许原请求开始。
        let preparation = observer
            .begin_activation_preparation(
                request,
                &source,
                &inspection.content_fingerprint,
                Some(&original),
                vec!["config".into(), "network_https".into()],
            )
            .expect("original request enters after real guard release");
        assert_eq!(preparation.request_id(), request);
        assert_eq!(
            store
                .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
                .expect_err("durable pending request now fences legacy mutation")
                .code,
            Code::Busy
        );
    }

    /// Complete original selection equality rejects changed approvals, revisions and declared namespaces before granting ownership.
    /// 完整原选择相等在授予所有权之前拒绝已变批准、修订及声明命名空间。
    #[test]
    fn legacy_mutation_guard_rejects_changed_complete_expected_activation() {
        // A genuine selected activation supplies all immutable comparison fields.
        // 真实已选激活提供全部不可变比较字段。
        let root = tempfile::tempdir().expect("complete expected activation fixture");
        // Physical package pins remain valid so rejection is about exact selection authority.
        // 物理插件包固定句柄保持有效，使拒绝针对精确选择权威。
        let (store, _package, _, original) = fixture(root.path());
        for field in ["revision", "approved_permissions", "plugin_id"] {
            // Matching other fields cannot conceal one changed piece of the original full activation.
            // 其他字段匹配不能掩盖原完整激活的一项变更。
            let mut wrong = original.clone();
            match field {
                "revision" => wrong.revision += 1,
                "approved_permissions" => wrong.approved_permissions = vec!["config".into()],
                "plugin_id" => wrong.plugin_id = "other-plugin".into(),
                _ => unreachable!(),
            }
            assert_eq!(
                store
                    .acquire_legacy_mutation_guard("legacy-plugin", Some(&wrong))
                    .expect_err("complete original expected activation must match")
                    .code,
                Code::Conflict
            );
        }
        assert_eq!(
            store
                .acquire_legacy_mutation_guard("legacy-plugin", None)
                .expect_err("published selection cannot become expected absence")
                .code,
            Code::Conflict
        );
        // Rejected acquisitions release their temporary OS fence rather than preventing a correct caller.
        // 被拒绝取得释放其临时操作系统屏障，不阻止正确调用方。
        let guard = store
            .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
            .expect("correct original remains available");
        assert!(guard.validate_plugin("other-plugin").is_err());
        // A different canonical store cannot adopt this actual namespace guard.
        // 不同规范存储不能接管此实际命名空间 guard。
        let other = tempfile::tempdir().expect("foreign store root");
        // Opening the other actual store confers no ownership of the original file.
        // 打开另一实际存储不授予原文件所有权。
        let foreign = VulcanPluginGenerationStore::open(other.path()).expect("open foreign store");
        assert_eq!(
            guard
                .validate_owner(&foreign, "legacy-plugin")
                .expect_err("foreign namespace cannot reuse retained authority")
                .code,
            Code::Conflict
        );
    }

    /// Non-cooperating changed bytes invalidate the retained original guard without replacing it for rollback.
    /// 非协作变更字节使保留原 guard 无效，不为回滚替换它。
    #[test]
    fn legacy_mutation_guard_detects_changed_selection_while_retaining_original_fence() {
        // Explicit external corruption does not stand in for a cooperating writer, which the real lock excludes.
        // 明确外部损坏不代替协作写入者，真实锁已排除此类写入者。
        let root = tempfile::tempdir().expect("retained original validation fixture");
        // The selected record and physical package come from actual production operations.
        // 已选记录及物理插件包来自实际生产操作。
        let (store, _package, _, original) = fixture(root.path());
        // This guard must remain the sole rollback authority even after detecting bad disk bytes.
        // 即使检测到错误磁盘字节，此 guard 仍必须保持唯一回滚权威。
        let guard = store
            .acquire_legacy_mutation_guard("legacy-plugin", Some(&original))
            .expect("hold original mutation fence");
        // Preserve real original bytes for an explicit test-only repair, never an API fallback.
        // 保留真实原字节，用于明确仅测试修复，绝不是接口回退。
        let path = store.root.join("active/legacy-plugin.json");
        // Changed approval remains a well-formed activation at the same revision.
        // 已变批准保持为相同修订的格式合法激活。
        let bytes = fs::read(&path).expect("original actual selection bytes");
        // Preserve exact schema and identity while altering one complete authority field.
        // 保留精确模式及身份，同时改变一个完整权威字段。
        let mut changed = original.clone();
        changed.approved_permissions = vec!["config".into()];
        fs::write(
            &path,
            serde_json::to_vec(&changed).expect("changed selection bytes"),
        )
        .expect("write explicit external change");
        assert_eq!(
            guard
                .validate_plugin("legacy-plugin")
                .expect_err("same revision cannot conceal changed approval")
                .code,
            Code::Conflict
        );
        assert_eq!(
            store
                .acquire_legacy_mutation_guard("legacy-plugin", Some(&changed))
                .expect_err("rollback cannot acquire a substitute latest guard")
                .code,
            Code::Busy
        );
        fs::write(path, bytes).expect("restore explicit test original bytes");
        guard
            .validate_plugin("legacy-plugin")
            .expect("same original guard revalidates original bytes");
    }

    /// The first bundled configuration edit initializes an absent host user root and a bundled settings record under the real namespace fence.
    /// 首次内建配置编辑在真实命名空间屏障下初始化缺失的宿主用户根目录及内建设置记录。
    #[test]
    fn bundled_configuration_edit_creates_settings_record_in_absent_user_root() {
        // The host has bundled packages but has never installed a user plugin or created its root.
        // 宿主已有内建插件包，但从未安装用户插件或创建其根目录。
        let root = tempfile::tempdir().expect("absent bundled user root fixture");
        write_package(&root.path().join("bundled"), "legacy-plugin");
        assert!(!root.path().join("user").exists());
        // The ordinary no-engine manager constructor intentionally does not initialize filesystem paths.
        // 普通无引擎管理器构造器有意不初始化文件系统路径。
        let runtime = manager(root.path());
        assert!(!root.path().join("user").exists());
        runtime
            .update_plugin_config("legacy-plugin", Value::Null)
            .expect("explicit bundled mutation preserves established behavior");
        assert!(root.path().join("user").is_dir());
        assert_eq!(
            runtime
                .plugin_config("legacy-plugin")
                .expect("read actual bundled null document"),
            Value::Null
        );
        // The mutation created the actual canonical activation store and namespace lock, not a lock-free exception.
        // 此变更创建实际规范激活存储及命名空间锁，不是无锁例外。
        let store = VulcanPluginGenerationStore::open_existing(&root.path().join("user"))
            .expect("validate created real store")
            .expect("explicit writer initialized store");
        assert!(store.root.join("locks/legacy-plugin.lock").is_file());
        let record = store
            .activation("legacy-plugin")
            .expect("read bundled settings")
            .expect("first edit created the settings record");
        assert_eq!(
            record.package(),
            &crate::VulcanPluginPackageSelection::Bundled {}
        );
        assert!(record.credential_revision().is_none());
        assert!(!root.path().join("state/legacy-plugin/config.json").exists());
        // Any holder of the exclusive namespace fence blocks the settings writer.
        // 任何排他命名空间屏障的持有者都会阻止设置写入者。
        let guard = store
            .acquire_legacy_mutation_guard("legacy-plugin", Some(&record))
            .expect("hold the bundled namespace");
        assert!(
            runtime
                .update_plugin_config("legacy-plugin", json!({"label": "blocked-by-real-guard"}))
                .is_err()
        );
        drop(guard);
        // The refused writer left the original document in place.
        // 被拒写入者保留原文档。
        assert_eq!(
            runtime
                .plugin_config("legacy-plugin")
                .expect("failed contender preserves null"),
            Value::Null
        );
        runtime
            .update_plugin_config("legacy-plugin", json!({"label": "guard-released"}))
            .expect("guard drop permits real config writer");
    }

    /// An empty package secret inventory must never access an external backend during version-four staging.
    /// 第四版暂存期间，空插件包秘密清单绝不能访问外部后端。
    struct EmptyBackend;

    impl VulcanPluginCredentialBackend for EmptyBackend {
        /// Reject an unexpected read of plugin/revision/key; this fixture has no secret slots and returns no value.
        /// 拒绝意外读取 plugin／revision／key；此夹具没有秘密槽位且不返回值。
        fn read(
            &self,
            _: &str,
            _: &VulcanPluginCredentialRevision,
            _: &VulcanPluginCredentialKey,
        ) -> std::result::Result<Option<String>, String> {
            Err("Unexpected empty-inventory backend read.".into())
        }

        /// Reject an unexpected plugin/revision/key/value write; no value belongs to this fixture.
        /// 拒绝意外 plugin／revision／key／value 写入；此夹具不拥有任何值。
        fn write(
            &self,
            _: &str,
            _: &VulcanPluginCredentialRevision,
            _: &VulcanPluginCredentialKey,
            _: &str,
        ) -> std::result::Result<(), String> {
            Err("Unexpected empty-inventory backend write.".into())
        }

        /// Reject an unexpected plugin/revision/key deletion; the fixture owns no external entry.
        /// 拒绝意外 plugin／revision／key 删除；此夹具不拥有外部条目。
        fn delete(
            &self,
            _: &str,
            _: &VulcanPluginCredentialRevision,
            _: &VulcanPluginCredentialKey,
        ) -> std::result::Result<(), String> {
            Err("Unexpected empty-inventory backend deletion.".into())
        }
    }

    /// A selected managed Document(null) is explicit; edits replace it in the settings section without legacy state.
    /// 选定受管 Document(null) 是显式文档；编辑在设置段中替换它且不产生旧状态。
    #[test]
    fn managed_configuration_edits_use_settings_and_never_write_legacy_state() {
        // The package supports null, but version-four selected documents belong to the activation itself.
        // 插件包支持空值，但第四版已选文档属于激活本身。
        let root = tempfile::tempdir().expect("managed null configuration fixture");
        // The old version-two selection supplies the exact legitimate predecessor revision.
        // 旧第二版选择提供精确合法前驱修订。
        let (store, package, _, original) = fixture(root.path());
        // This staged credential identity contains no backend secret slots.
        // 此已暂存凭据身份不包含后端秘密槽位。
        let credentials = VulcanPluginCredentialRevision::allocate();
        store
            .begin_credential_staging(
                &package,
                credentials.clone(),
                &VulcanPluginCredentialValues::new(),
            )
            .expect("begin actual empty staged credentials")
            .write_and_finish(&EmptyBackend, &VulcanPluginCredentialValues::new())
            .expect("confirm exact empty revision");
        let selected = store
            .activate_configured(
                &package,
                original.revision,
                package.required_permissions.clone(),
                credentials,
                VulcanPluginConfigurationSelection::Document { value: Value::Null },
            )
            .expect("select complete managed null document");
        // Refresh the actual manager so rejection depends on its declared immutable selection, not stale metadata.
        // 刷新实际管理器，使拒绝取决于声明不可变选择，而非过期元数据。
        let runtime = manager(root.path());
        assert_eq!(
            runtime
                .plugin_config("legacy-plugin")
                .expect("managed null is explicit document"),
            Value::Null
        );
        runtime
            .update_plugin_config("legacy-plugin", json!({"label": "edited"}))
            .expect("managed edit commits to the settings section");
        assert_eq!(
            runtime
                .plugin_config("legacy-plugin")
                .expect("read edited settings"),
            json!({"label": "edited"})
        );
        // The edit kept the package selection and republished a descriptor mirroring the edited record.
        // 编辑保留插件包选择，并重新发布镜像已编辑记录的描述符。
        let edited = store
            .activation("legacy-plugin")
            .expect("read edited record")
            .expect("edited record");
        assert_eq!(edited.revision, selected.revision);
        assert_eq!(edited.package(), selected.package());
        assert_eq!(edited.settings().revision, selected.settings().revision + 1);
        assert!(
            runtime.snapshot().plugins["legacy-plugin"]
                .managed_package()
                .expect("managed descriptor")
                .matches_activation(&edited)
        );
        assert!(!root.path().join("state/legacy-plugin/config.json").exists());
    }
}
