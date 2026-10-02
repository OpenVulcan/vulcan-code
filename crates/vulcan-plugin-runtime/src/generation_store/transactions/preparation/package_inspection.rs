//! Original package preparation is confirmed through actual markers, verified bytes and a real retained generation File.
//! 原插件包准备通过实际标记、已验证字节及真实保留代次 File 确认。

use super::*;
use VulcanPluginActivationTransactionPhase as Phase;

impl VulcanPluginGenerationStore {
    /// Inspect this actual original preparation's completed package; return its real immutable package pin.
    /// 检查此实际原准备已完成插件包；返回真实不可变插件包固定句柄。
    /// The complete original journal/index/predecessor and approved permissions must match under one shared namespace fence.
    /// 完整原日志／索引／前驱及已批准权限必须在单个共享命名空间屏障下匹配。
    /// No source is accepted and no preparation, copy, marker write or incomplete-directory cleanup is performed.
    /// 不接受来源，不执行准备、复制、标记写入或不完整目录清理。
    pub fn inspect_activation_preparation_package(
        &self,
        preparation: &VulcanPluginActivationPreparation,
    ) -> VulcanPluginStoreResult<PinnedVulcanPluginGeneration> {
        if preparation.store_root != self.root || preparation.record.phase != Phase::Preparing {
            return Err(Error::new(
                Code::Conflict,
                "Package inspection requires this store's original Preparing owner.",
            ));
        }
        // Physical namespace ownership spans the complete original validation and actual generation pin acquisition.
        // 物理命名空间所有权覆盖完整原校验及实际代次固定取得。
        let _namespace =
            self.namespace_lock(&preparation.record.preparation.generation.plugin_id, true)?;
        self.check_preparation_owner(preparation)?;
        // The existing sole pin primitive requires exact Prepared identity metadata and re-verifies actual immutable bytes.
        // 既有唯一固定原语要求精确 Prepared 身份元数据，并重新验证实际不可变字节。
        let generation = self.pin_locked(&preparation.record.preparation.generation)?;
        if generation.required_permissions != preparation.record.preparation.approved_permissions {
            return Err(Error::new(
                Code::Conflict,
                "Prepared package permissions differ from its exact original approval.",
            ));
        }
        Ok(generation)
    }
}

#[cfg(test)]
impl VulcanPluginGenerationStore {
    /// Scope identity's actual allocation or successful Prepared writer observation and optional panic/error loss.
    /// 限定 identity 实际分配或成功 Prepared 写入观察及可选 panic／错误丢失作用域。
    /// Returns same-thread cleanup; never allocates or writes a generation itself.
    /// 返回同线程清理；自身绝不分配或写入代次。
    pub(crate) fn observe_package_preparation_for_test(
        &self,
        identity: &VulcanPluginGenerationIdentity,
        checkpoint: crate::generation_store::VulcanPluginGenerationPhase,
        panic: Option<bool>,
    ) -> impl Drop {
        observation::arm(&self.root, identity, checkpoint, panic)
    }

    /// Record identity's actual checkpoint after real I/O; return only the scoped test acknowledgement.
    /// 真实 I/O 后记录 identity 实际检查点；仅返回作用域测试确认。
    pub(in crate::generation_store) fn observe_package_preparation_checkpoint_for_test(
        &self,
        identity: &VulcanPluginGenerationIdentity,
        checkpoint: crate::generation_store::VulcanPluginGenerationPhase,
    ) -> VulcanPluginStoreResult<()> {
        observation::checkpoint(&self.root, identity, checkpoint)
    }

    /// Return identity's actual allocation and successful Prepared marker writer counts within its current test scope.
    /// 返回 identity 当前测试作用域实际分配及成功 Prepared 标记写入次数。
    pub(crate) fn package_preparation_checkpoint_counts_for_test(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> (usize, usize) {
        observation::counts(&self.root, identity)
    }
}

/// Test-only physical allocation and real successful Prepared writer observations never replace production I/O.
/// 仅测试物理分配及真实成功 Prepared 写入观测，绝不替换生产 I/O。
#[cfg(test)]
mod observation {
    use super::*;
    use crate::generation_store::VulcanPluginGenerationPhase;

    /// One original generation's physical checkpoint acknowledgement loss and actual checkpoint counts.
    /// 单个原代次物理检查点确认丢失及实际检查点次数。
    struct Fault {
        /// Original fixed store and complete package identity prevent unrelated observer interference.
        /// 原固定存储及完整插件包身份防止无关观察器干扰。
        key: (PathBuf, VulcanPluginGenerationIdentity),
        /// Preparing denotes actual mkdir before the first marker; Prepared denotes the actual successful final writer.
        /// Preparing 表示首标记前实际 mkdir；Prepared 表示实际成功最终写入器。
        checkpoint: VulcanPluginGenerationPhase,
        /// One pending acknowledgement loss returns Storage or panics; consumed only by its actual checkpoint.
        /// 单次待定确认丢失返回 Storage 或 panic；仅由实际检查点消费。
        panic: Option<bool>,
        /// Count actual original directory allocations and successful Prepared marker writes independently.
        /// 分别统计实际原目录分配及成功 Prepared 标记写入。
        counts: (usize, usize),
    }

    std::thread_local! {
        /// Scoped test controls are confined to one exact generation on the current thread.
        /// 作用域测试控制限定于当前线程单个精确代次。
        static ARMED: std::cell::RefCell<Option<Fault>> = const { std::cell::RefCell::new(None) };
    }

    /// Same-thread cleanup owns only this original observer scope and never a production resource.
    /// 同线程清理仅拥有此原观察器作用域，不拥有生产资源。
    pub(super) struct Scope {
        /// Clear only the originally armed fixed store and complete generation.
        /// 仅清除原启用固定存储及完整代次。
        key: (PathBuf, VulcanPluginGenerationIdentity),
        /// A fault observer cannot migrate away from its original test thread.
        /// 故障观察器不能迁离原测试线程。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Scope {
        /// Retire this exact original observer on scope exit; accepts no arguments and returns no value.
        /// 作用域退出时退役此精确原观察器；无参数且不返回值。
        fn drop(&mut self) {
            ARMED.with(|armed| {
                // Mutable scope storage is borrowed only for this bounded cleanup.
                // 可变作用域存储仅为此有界清理借用。
                let mut armed = armed.borrow_mut();
                if armed.as_ref().is_some_and(|fault| fault.key == self.key) {
                    armed.take();
                }
            });
        }
    }

    /// Observe root/identity's declared real checkpoint and optional acknowledgement loss; return same-thread cleanup.
    /// 观察 root／identity 声明真实检查点及可选确认丢失；返回同线程清理。
    pub(super) fn arm(
        root: &Path,
        identity: &VulcanPluginGenerationIdentity,
        checkpoint: VulcanPluginGenerationPhase,
        panic: Option<bool>,
    ) -> Scope {
        assert!(matches!(
            checkpoint,
            VulcanPluginGenerationPhase::Preparing | VulcanPluginGenerationPhase::Prepared
        ));
        // Full original identity excludes every other generation sharing this plugin namespace.
        // 完整原身份排除共用此插件命名空间的其他每个代次。
        let key = (root.to_path_buf(), identity.clone());
        ARMED.with(|armed| {
            // A test must retire its actual observer before arming a different original generation.
            // 测试必须退役实际观察器后，才能启用不同原代次。
            let mut armed = armed.borrow_mut();
            assert!(armed.is_none(), "package checkpoint scopes overlap");
            *armed = Some(Fault {
                key: key.clone(),
                checkpoint,
                panic,
                counts: (0, 0),
            });
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }

    /// Observe root/identity only after actual mkdir or final writer success; return acknowledgement or the one scoped loss.
    /// 仅在实际 mkdir 或最终写入成功后观察 root／identity；返回确认或单次作用域丢失。
    pub(super) fn checkpoint(
        root: &Path,
        identity: &VulcanPluginGenerationIdentity,
        checkpoint: VulcanPluginGenerationPhase,
    ) -> VulcanPluginStoreResult<()> {
        // Consume a matching fault before panicking so the caller can inspect the exact original without replay.
        // 在 panic 前消费匹配故障，使调用方无需重放即可检查精确原代次。
        let loss = ARMED.with(|armed| {
            // This bounded mutable observer borrow is released before any unwind.
            // 此有界可变观察器借用在任何展开之前释放。
            let mut armed = armed.borrow_mut();
            if let Some(fault) = armed.as_mut()
                && fault.key.0 == root
                && fault.key.1 == *identity
            {
                match checkpoint {
                    VulcanPluginGenerationPhase::Preparing => fault.counts.0 += 1,
                    VulcanPluginGenerationPhase::Prepared => fault.counts.1 += 1,
                    _ => unreachable!("only physical preparation checkpoints are instrumented"),
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
            Some(true) => panic!(
                "Controlled loss of original package preparation checkpoint acknowledgement."
            ),
            Some(false) => Err(Error::new(
                Code::Storage,
                "Controlled loss of original physical package checkpoint acknowledgement.",
            )),
            None => Ok(()),
        }
    }

    /// Read root/identity's scoped actual allocation/final writer counts; return counts without granting physical authority.
    /// 读取 root／identity 作用域实际分配／最终写入次数；返回次数，不授予物理权威。
    pub(super) fn counts(root: &Path, identity: &VulcanPluginGenerationIdentity) -> (usize, usize) {
        ARMED.with(|armed| {
            // The scope must still exist when its actual checkpoint evidence is asserted.
            // 断言实际检查点证据时，作用域必须仍存在。
            let armed = armed.borrow();
            // The one exact observer is identified by full original store and package identity.
            // 单个精确观察器由完整原存储及插件包身份定位。
            let fault = armed
                .as_ref()
                .expect("original package checkpoint scope remains armed");
            assert_eq!(fault.key.0, root);
            assert_eq!(&fault.key.1, identity);
            fault.counts
        })
    }
}
