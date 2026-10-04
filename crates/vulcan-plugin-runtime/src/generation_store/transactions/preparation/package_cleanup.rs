//! Original Aborted preparations observe or collect only their exact package while retaining actual owners through readback.
//! 原 Aborted 准备对象仅观察或回收自身精确插件包，并跨回读保留实际所有者。

use super::*;
use VulcanPluginActivationTransactionPhase as Phase;

/// Actual package cleanup observations bound to the borrowed original; none grants future deletion authority.
/// 绑定借用原请求的实际插件包清理观察；均不授予未来删除权。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginPreparationPackageCleanupObservation {
    /// The exact generation directory is absent under this namespace observation; no lock file or package is created.
    /// 此命名空间观察内精确代际目录缺失；不创建锁文件或插件包。
    DirectoryAbsent,
    /// The actual directory exists but both marker and exact package child are absent; unknown siblings remain intact.
    /// 实际目录存在，但标记及精确插件包子目录均缺失；未知同级条目保持完整。
    UnmarkedPackageAbsent,
    /// Strict exact metadata states Collected and actual package bytes are absent after the sole collection primitive.
    /// 唯一回收原语后，严格精确元数据声明 Collected，且实际插件包字节缺失。
    Collected,
}

impl VulcanPluginGenerationStore {
    /// Observe or collect only `preparation`'s package after its actual local Aborted confirmation.
    /// 在 `preparation` 实际本地 Aborted 确认后，仅观察或回收其插件包。
    /// Return a typed actual observation with the original File retained; no implicit abort, credential cleanup or callback occurs.
    /// 保留原 File 并返回类型化实际观察；不隐式中止、不清理凭据且不执行回调。
    pub fn collect_activation_preparation_package_retained(
        &self,
        preparation: &VulcanPluginActivationPreparation,
    ) -> VulcanPluginStoreResult<VulcanPluginPreparationPackageCleanupObservation> {
        self.check_retained_preparation_binding(preparation)?;
        if preparation.record.phase != Phase::Aborted {
            return Err(Error::new(
                Code::Conflict,
                "Retained package cleanup requires its already confirmed local Aborted original.",
            ));
        }
        // One real exclusive namespace orders the full original, admission checks and any generation guard acquisition.
        // 同一真实排他命名空间排序完整原请求、准入检查及任何代际守卫取得。
        let identity = &preparation.record.preparation.generation;
        // The shared full-original validator accepts only exact local Aborted, including all intent/schema fields and actual index.
        // 共享完整原校验器仅接受精确本地 Aborted，包括全部意图、模式字段及实际索引。
        let _namespace = self.namespace_lock(&identity.plugin_id, false)?;
        self.retained_preparation_original_locked(preparation)?;
        self.check_generation_collection_admission_locked(identity)?;
        // Strict component observations reject redirects and foreign entry types before dereferencing descendants.
        // 严格组件观察在解引用后代之前拒绝重定向及外来条目类型。
        let plugin_root = self.root.join("packages").join(&identity.plugin_id);
        if !files::exists(&plugin_root, true)? {
            return Ok(VulcanPluginPreparationPackageCleanupObservation::DirectoryAbsent);
        }
        // No nonexistent bytes require a manufactured generation lease or collection marker.
        // 不存在字节不需要伪造代际租约或回收标记。
        let generation_root = plugin_root.join(&identity.generation_id);
        if !files::exists(&generation_root, true)? {
            return Ok(VulcanPluginPreparationPackageCleanupObservation::DirectoryAbsent);
        }
        // Existing generation bytes are observed and deleted only while their actual exclusive File remains held.
        // 既有代际字节仅在其实际排他 File 保持持有时观察及删除。
        let _generation = self.lock_generation_for_collection_locked(identity)?;
        // Marker absence is never converted into permission to delete an unproven package child.
        // 标记缺失绝不转换为删除未经证明插件包子目录的许可。
        let marker_path = generation_root.join("generation.json");
        // The exact original identity determines the only removable package child.
        // 精确原身份确定唯一可删除插件包子目录。
        let package_root = generation_root.join(&identity.plugin_id);
        if !files::exists(&marker_path, false)? {
            if files::exists(&package_root, true)? {
                return Err(Error::invalid(
                    "Unmarked original generation still contains unproven package bytes.",
                ));
            }
            return Ok(VulcanPluginPreparationPackageCleanupObservation::UnmarkedPackageAbsent);
        }
        // The test-only original context scopes post-success observers without changing the sole production collection algorithm.
        // 仅测试原上下文限定成功后观察器，不改变唯一生产回收算法。
        #[cfg(test)]
        let _observation = observation::bind(&self.root, identity, preparation.record.request_id);
        self.collect_generation_locked(identity)?;
        // Final strict readback occurs before either the namespace or exclusive generation File retires.
        // 最终严格回读发生在命名空间或排他代际 File 任一退役之前。
        let (_, record) = self.read_generation(identity)?;
        if record.state != GenerationState::Collected || files::exists(&package_root, true)? {
            return Err(Error::invalid(
                "Original package cleanup lacks exact Collected metadata and actual byte absence.",
            ));
        }
        Ok(VulcanPluginPreparationPackageCleanupObservation::Collected)
    }

    /// Observe exact root/generation/request collection checkpoints, optionally losing acknowledgement after a real success.
    /// 观察精确根、代际、请求回收检查点，可选在真实成功后丢失确认。
    /// Return same-thread cleanup; Collecting selects real remove success and Collected selects the actual final marker writer.
    /// 返回同线程清理；Collecting 选择真实删除成功，Collected 选择实际最终标记写入器。
    #[cfg(test)]
    pub(crate) fn observe_preparation_package_collection_for_test(
        &self,
        preparation: &VulcanPluginActivationPreparation,
        checkpoint: GenerationState,
        panic: Option<bool>,
    ) -> impl Drop {
        observation::arm(
            &self.root,
            preparation.intent().generation(),
            preparation.request_id(),
            checkpoint,
            panic,
        )
    }

    /// Record only the active original's actual physical removal or final marker success; return its one acknowledgement outcome.
    /// 仅记录活动原请求实际物理删除或最终标记成功；返回其单次确认结果。
    #[cfg(test)]
    pub(in crate::generation_store) fn observe_package_collection_checkpoint_for_test(
        &self,
        identity: &VulcanPluginGenerationIdentity,
        checkpoint: GenerationState,
    ) -> VulcanPluginStoreResult<()> {
        observation::checkpoint(&self.root, identity, checkpoint)
    }

    /// Return this original's actual successful removal/final-marker counts without allocating or releasing any production owner.
    /// 返回此原请求实际成功删除、最终标记次数，不分配或释放任何生产所有者。
    #[cfg(test)]
    pub(crate) fn preparation_package_collection_counts_for_test(
        &self,
        preparation: &VulcanPluginActivationPreparation,
    ) -> (usize, usize) {
        observation::counts(
            &self.root,
            preparation.intent().generation(),
            preparation.request_id(),
        )
    }
}

/// Test-only scoped observers count true collection successes and preserve original binding across unwinding.
/// 仅测试作用域观察器统计真实回收成功，并跨展开保留原绑定。
#[cfg(test)]
mod observation {
    use super::*;

    /// Full root/identity/request binding prevents an unrelated ordinary collect call from consuming a fault.
    /// 完整根、身份、请求绑定阻止无关普通 collect 调用消费故障。
    #[derive(Clone, PartialEq, Eq)]
    struct Key {
        /// Actual store root.
        /// 实际存储根。
        root: PathBuf,
        /// Complete original generation identity.
        /// 完整原代际身份。
        identity: VulcanPluginGenerationIdentity,
        /// Actual retained original request.
        /// 实际保留原请求。
        request: Uuid,
    }

    /// One exact original's scoped fault and real successful writer counts.
    /// 单个精确原请求作用域故障及真实成功写入次数。
    struct Fault {
        /// Original immutable binding.
        /// 原不可变绑定。
        key: Key,
        /// Physical removal or final marker checkpoint.
        /// 物理删除或最终标记检查点。
        checkpoint: GenerationState,
        /// One optional post-success Err or panic.
        /// 单次可选成功后 Err 或 panic。
        panic: Option<bool>,
        /// Actual remove/final-marker success counts.
        /// 实际删除、最终标记成功次数。
        counts: (usize, usize),
    }
    std::thread_local! {
        /// Scoped fault exists only on its originating test thread.
        /// 作用域故障仅存在于其原测试线程。
        static ARMED: std::cell::RefCell<Option<Fault>> = const { std::cell::RefCell::new(None) };
        /// Only an actual retained entry can bind the original request around the common collection primitive.
        /// 仅实际保留入口可在共同回收原语周围绑定原请求。
        static ACTIVE: std::cell::RefCell<Option<Key>> = const { std::cell::RefCell::new(None) };
    }
    /// Thread-bound guard clears either its original fault or bounded active context on exit and unwinding.
    /// 线程绑定守卫在退出及展开时清除原故障或有界活动上下文。
    pub(super) struct Scope {
        /// Exact scope binding.
        /// 精确作用域绑定。
        key: Key,
        /// Active writer context and fault scope have distinct cleanup ownership.
        /// 活动写入上下文及故障作用域具有不同清理归属。
        active: bool,
        /// Never migrate cleanup to another thread.
        /// 绝不将清理迁移至另一线程。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }
    impl Drop for Scope {
        /// Retire only this exact thread-local scope; no production file is touched.
        /// 仅退役此精确线程局部作用域；不触及任何生产文件。
        fn drop(&mut self) {
            if self.active {
                ACTIVE.with(|active| {
                    let mut active = active.borrow_mut();
                    if active.as_ref() == Some(&self.key) {
                        active.take();
                    }
                });
            } else {
                ARMED.with(|armed| {
                    let mut armed = armed.borrow_mut();
                    if armed.as_ref().is_some_and(|fault| fault.key == self.key) {
                        armed.take();
                    }
                });
            }
        }
    }
    /// Build an exact test key without opening files or granting authority.
    /// 不打开文件或授予权威，构建精确测试键。
    fn key(root: &Path, identity: &VulcanPluginGenerationIdentity, request: Uuid) -> Key {
        Key {
            root: root.to_path_buf(),
            identity: identity.clone(),
            request,
        }
    }
    /// Arm one exact original's checkpoint loss; return cleanup preserving real writer counts.
    /// 启用单个精确原请求检查点丢失；返回保留真实写入计数的清理。
    pub(super) fn arm(
        root: &Path,
        identity: &VulcanPluginGenerationIdentity,
        request: Uuid,
        checkpoint: GenerationState,
        panic: Option<bool>,
    ) -> Scope {
        assert!(matches!(
            checkpoint,
            GenerationState::Collecting | GenerationState::Collected
        ));
        // A fault binds complete source-defined identity and actual original UUID.
        // 故障绑定完整源码定义身份及实际原 UUID。
        let key = key(root, identity, request);
        ARMED.with(|armed| {
            let mut armed = armed.borrow_mut();
            assert!(armed.is_none(), "collection scopes overlap");
            *armed = Some(Fault {
                key: key.clone(),
                checkpoint,
                panic,
                counts: (0, 0),
            });
        });
        Scope {
            key,
            active: false,
            _thread: std::marker::PhantomData,
        }
    }
    /// Bind the actual validated original around the shared primitive; return unwind-safe retirement.
    /// 在共享原语周围绑定实际已校验原请求；返回展开安全退役。
    pub(super) fn bind(
        root: &Path,
        identity: &VulcanPluginGenerationIdentity,
        request: Uuid,
    ) -> Scope {
        // This context exists only after production full-original and real generation-owner checks pass.
        // 此上下文仅在生产完整原请求及真实代际所有者检查通过后存在。
        let key = key(root, identity, request);
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            assert!(active.is_none(), "collection contexts overlap");
            *active = Some(key.clone());
        });
        Scope {
            key,
            active: true,
            _thread: std::marker::PhantomData,
        }
    }
    /// Count an actual success only when both the fault and active original fully match; return its consumed loss.
    /// 仅在故障及活动原请求完整匹配时统计实际成功；返回其已消费丢失。
    pub(super) fn checkpoint(
        root: &Path,
        identity: &VulcanPluginGenerationIdentity,
        checkpoint: GenerationState,
    ) -> VulcanPluginStoreResult<()> {
        // Copy the active original before borrowing the independent fault slot.
        // 借用独立故障槽之前复制活动原请求。
        let active = ACTIVE.with(|active| active.borrow().clone());
        // An ordinary collect call lacks an actual retained request context and cannot consume these faults.
        // 普通 collect 调用缺少实际保留请求上下文，不能消费这些故障。
        let loss = ARMED.with(|armed| {
            let mut armed = armed.borrow_mut();
            if let Some(fault) = armed.as_mut()
                && active.as_ref() == Some(&fault.key)
                && fault.key.root == root
                && fault.key.identity == *identity
            {
                match checkpoint {
                    GenerationState::Collecting => fault.counts.0 += 1,
                    GenerationState::Collected => fault.counts.1 += 1,
                    _ => unreachable!("declared physical collection checkpoints"),
                }
                if checkpoint == fault.checkpoint {
                    fault.panic.take()
                } else {
                    None
                }
            } else {
                None
            }
        });
        match loss {
            Some(true) => panic!("Controlled post-success original package collection panic."),
            Some(false) => Err(Error::new(
                Code::Storage,
                "Controlled loss of original package collection acknowledgement.",
            )),
            None => Ok(()),
        }
    }
    /// Read this scoped original's true removal and marker counts; unrelated keys are test errors.
    /// 读取此作用域原请求真实删除及标记次数；无关键属于测试错误。
    pub(super) fn counts(
        root: &Path,
        identity: &VulcanPluginGenerationIdentity,
        request: Uuid,
    ) -> (usize, usize) {
        ARMED.with(|armed| {
            let armed = armed.borrow();
            let fault = armed.as_ref().expect("collection scope exists");
            assert!(
                fault.key == key(root, identity, request),
                "actual original collection scope matches"
            );
            fault.counts
        })
    }
}
