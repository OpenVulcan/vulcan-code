//! One retained original owner promotes or confirms its exact Prepared journal without reopening its request lease.
//! 同一保留原所有者晋级或确认精确 Prepared 日志，不重新打开其请求租约。

use super::*;
use VulcanPluginActivationTransactionPhase as Phase;

impl VulcanPluginGenerationStore {
    /// Promote or confirm original `preparation` using exactly its frozen `configuration`; return an actual configured owner.
    /// 使用精确固定 `configuration` 晋级或确认原 `preparation`；返回实际已配置所有者。
    /// Errors and unwinding retain the caller's original preparation; this never reacquires its exclusive request file.
    /// 错误及展开保留调用方原准备对象；此操作绝不重新取得其排他请求文件。
    /// Success briefly shares the same file with the returned transaction; retire preparation before candidate execution or mutation.
    /// 成功时短暂与返回事务共享同一文件；候选执行或变更前须退役准备对象。
    pub fn finish_activation_preparation_retained(
        &self,
        preparation: &mut VulcanPluginActivationPreparation,
        configuration: VulcanPluginConfigurationSelection,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationTransaction> {
        if preparation.store_root != self.root || preparation.record.phase != Phase::Preparing {
            return Err(Error::new(
                Code::Conflict,
                "Retained promotion does not belong to this original preparing store.",
            ));
        }
        // Frozen original identity is validated before accessing any namespace or physical target.
        // 访问任何命名空间或物理目标前，先校验固定原身份。
        let plugin_id = &preparation.record.preparation.generation.plugin_id;
        Self::validate_preparation_record(
            &preparation.record,
            plugin_id,
            preparation.record.request_id,
        )?;
        // Every authority read, resource pin and possible checkpoint write uses one original namespace fence.
        // 每次权威读取、资源固定及可能检查点写入均使用同一原命名空间屏障。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        if Arc::strong_count(&preparation.lease) != 1 {
            return Err(Error::new(
                Code::Busy,
                "Original credential preparation or returned transaction still owns unfinished work.",
            ));
        }
        if self.read_activation(plugin_id)? != preparation.record.predecessor {
            return Err(Error::new(
                Code::Conflict,
                "Retained promotion predecessor differs from its complete original selection.",
            ));
        }
        // Only the approved early intent and supplied frozen configuration define this one possible Prepared target.
        // 仅已批准早期意图及所提供固定配置定义此唯一可能 Prepared 目标。
        let intent = &preparation.record.preparation;
        // Complete configuration preserves explicit null versus absence according to the existing selection contract.
        // 完整配置按既有选择契约保留明确空值与缺失的区别。
        let expected = VulcanPluginActivationTransactionRecord {
            schema_version: preparation.record.schema_version,
            request_id: preparation.record.request_id,
            predecessor: preparation.record.predecessor.clone(),
            // The frozen predecessor alone derives the settings revision, so retries rebuild the same target.
            // 设置修订仅由固定前驱派生，因此重试会重建相同目标。
            target: VulcanPluginActivation::generation_selection(
                intent.generation.plugin_id.clone(),
                intent.activation_revision,
                intent.generation.clone(),
                intent.approved_permissions.clone(),
                VulcanPluginActivation::next_settings_revision(
                    preparation.record.predecessor.as_ref(),
                )?,
                configuration,
                intent.credential_revision.clone(),
            ),
            preparation: Some(intent.clone()),
            phase: Phase::Prepared,
        };
        Self::validate_transaction_record(&expected, plugin_id, expected.request_id)?;
        // Missing or foreign indices are errors; retained confirmation cannot repair or normalize original authority.
        // 缺失或外部索引属于错误；保留式确认不能修复或归一化原权威。
        let persisted = self.indexed_request_locked(plugin_id)?;
        // Only exact original Preparing can write; exact already-written Prepared is strictly a read-only confirmation.
        // 仅精确原 Preparing 可写入；精确已写 Prepared 严格属于只读确认。
        let needs_write = match persisted {
            Some(VulcanPluginActivationRequestRecord::Preparation(record))
                if record == preparation.record =>
            {
                true
            }
            Some(VulcanPluginActivationRequestRecord::Activation(record)) if record == expected => {
                false
            }
            _ => {
                return Err(Error::new(
                    Code::Conflict,
                    "Original promotion index, immutable intent or complete Prepared target changed.",
                ));
            }
        };
        // Real leases revalidate immutable bytes, exact permissions/configuration and the original staged credential UUID.
        // 实际租约复核不可变字节、精确权限／配置及原已暂存凭据 UUID。
        let (generation, credentials) = self.pin_transaction_target(&expected)?;
        if needs_write {
            self.write_transaction_locked(&expected)?;
            #[cfg(test)]
            observation::after_write(&self.root, expected.request_id)?;
            if self.indexed_transaction_locked(plugin_id)?.as_ref() != Some(&expected) {
                return Err(Error::new(
                    Code::Conflict,
                    "Successful original promotion lacks its exact indexed Prepared readback.",
                ));
            }
        }
        Ok(VulcanPluginActivationTransaction {
            store_root: self.root.clone(),
            record: expected,
            generation,
            credentials,
            _lease: Arc::clone(&preparation.lease),
        })
    }

    /// Observe request's real Prepared writes and optional one loss; return same-thread cleanup without production switches.
    /// 观察 request 真实 Prepared 写入及可选单次丢失；返回同线程清理，不提供生产开关。
    #[cfg(test)]
    pub(crate) fn observe_preparation_promotion_writes_for_test(
        &self,
        request: Uuid,
        loss: Option<bool>,
    ) -> impl Drop {
        observation::arm(&self.root, request, loss)
    }

    /// Read request's actual successful promotion writes in its current scope; no storage or ownership is changed.
    /// 读取当前作用域内 request 实际成功晋级写入次数；不改变存储或所有权。
    #[cfg(test)]
    pub(crate) fn preparation_promotion_write_count_for_test(&self, request: Uuid) -> usize {
        observation::writes(&self.root, request)
    }
}

/// Scoped post-write faults observe the real original atomic writer without replacing production storage behavior.
/// 作用域写入后故障观察真实原原子写入器，不替换生产存储行为。
#[cfg(test)]
mod observation {
    use super::*;

    /// Actual matching writer count and a single optional post-success loss for one original request.
    /// 单个原请求的实际匹配写入次数，以及单次可选成功后丢失。
    struct Fault {
        /// Exact original store and request exclude unrelated writers on this thread.
        /// 精确原存储及请求排除此线程无关写入者。
        key: (PathBuf, Uuid),
        /// None observes normally; false loses acknowledgement; true unwinds after successful persistence.
        /// 空值正常观察；false 丢失确认；true 在成功持久化后展开。
        loss: Option<bool>,
        /// Incremented only after the real matching Prepared atomic writer returned success.
        /// 仅在真实匹配 Prepared 原子写入器返回成功之后递增。
        writes: usize,
    }

    std::thread_local! {
        /// One same-thread original observation scope exists without any production fault switch.
        /// 单个同线程原观测作用域存在，不提供任何生产故障开关。
        static ARMED: std::cell::RefCell<Option<Fault>> = const { std::cell::RefCell::new(None) };
    }

    /// Same-thread cleanup retains the original observation scope across worker unwinding.
    /// 同线程清理跨工作展开保留原观测作用域。
    pub(super) struct Scope {
        /// Only this exact observation is cleared on scope retirement.
        /// 作用域退役时仅清除此精确观测。
        key: (PathBuf, Uuid),
        /// Cleanup must execute on the thread that owns its observation state.
        /// 清理必须在持有其观测状态的线程执行。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Scope {
        /// Clear this exact remaining fault during normal exit or unwinding; accepts no input and returns no value.
        /// 在正常退出或展开时清除此精确剩余故障；无输入且不返回值。
        fn drop(&mut self) {
            ARMED.with(|armed| {
                // Unrelated observations cannot be cleared by an older scope.
                // 较旧作用域不能清除无关观测。
                let mut armed = armed.borrow_mut();
                if armed.as_ref().is_some_and(|fault| fault.key == self.key) {
                    armed.take();
                }
            });
        }
    }

    /// Observe root/request's successful actual writes, optionally losing one acknowledgement; return scoped cleanup.
    /// 观察 root／request 成功实际写入，可选丢失一次确认；返回作用域清理。
    pub(super) fn arm(root: &Path, request: Uuid, loss: Option<bool>) -> Scope {
        assert!(!request.is_nil(), "original observation UUID cannot be nil");
        // Observation does not allocate a request, acquire a file or write any storage.
        // 观测不分配请求、不取得文件且不写入任何存储。
        let key = (root.to_path_buf(), request);
        ARMED.with(|armed| {
            // Explicit retirement prevents overlapping scoped faults from replacing one another.
            // 明确退役阻止重叠作用域故障相互替换。
            let mut armed = armed.borrow_mut();
            assert!(
                armed.is_none(),
                "original promotion observation scope overlaps"
            );
            *armed = Some(Fault {
                key: key.clone(),
                loss,
                writes: 0,
            });
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }

    /// Count root/request's real successful Prepared write and apply its one loss; return explicit observation status.
    /// 统计 root／request 真实成功 Prepared 写入并应用其一次丢失；返回明确观测状态。
    pub(super) fn after_write(root: &Path, request: Uuid) -> VulcanPluginStoreResult<()> {
        // The actual atomic writer already returned success before this test-only observer executes.
        // 此仅测试观察器执行前，实际原子写入器已经返回成功。
        let loss = ARMED.with(|armed| {
            // The mutable observation borrow ends before either an error or panic is raised.
            // 可变观测借用在返回错误或触发 panic 之前结束。
            let mut armed = armed.borrow_mut();
            if let Some(fault) = armed.as_mut()
                && fault.key.0 == root
                && fault.key.1 == request
            {
                fault.writes += 1;
                fault.loss.take()
            } else {
                None
            }
        });
        match loss {
            Some(true) => panic!("Controlled post-success original promotion worker panic."),
            Some(false) => Err(Error::new(
                Code::Storage,
                "Controlled loss of successful original promotion observation.",
            )),
            None => Ok(()),
        }
    }

    /// Read the matching scope's actual successful writer count; reject unrelated test identities.
    /// 读取匹配作用域的实际成功写入次数；拒绝无关测试身份。
    pub(super) fn writes(root: &Path, request: Uuid) -> usize {
        ARMED.with(|armed| {
            // Tests query only their retained original scope, never a process-wide substitute counter.
            // 测试仅查询其保留原作用域，绝不使用进程级替代计数器。
            let armed = armed.borrow();
            // An absent scope is a test error rather than a fabricated zero.
            // 缺失作用域属于测试错误，而非虚构零值。
            let fault = armed
                .as_ref()
                .expect("original promotion scope remains armed");
            assert_eq!(fault.key.0, root);
            assert_eq!(fault.key.1, request);
            fault.writes
        })
    }
}
