//! Retained early abandonment confirms the exact original without reopening its request file or deleting resources.
//! 保留所有者的早期放弃确认精确原请求，不重新打开请求文件或删除资源。

use super::*;
use VulcanPluginActivationTransactionPhase as Phase;

impl VulcanPluginGenerationStore {
    /// Validate the actual original store, full local early body and sole request owner before acquiring its namespace.
    /// 在取得命名空间之前校验实际原存储、完整本地早期正文及唯一请求所有者。
    /// Return unit for an unshared Preparing or Aborted original; never reopen its File.
    /// 对未共享 Preparing 或 Aborted 原请求返回空值；绝不重新打开其 File。
    pub(super) fn check_retained_preparation_binding(
        &self,
        preparation: &VulcanPluginActivationPreparation,
    ) -> VulcanPluginStoreResult<()> {
        if preparation.store_root != self.root
            || !matches!(preparation.record.phase, Phase::Preparing | Phase::Aborted)
        {
            return Err(Error::new(
                Code::Conflict,
                "Retained abandonment does not belong to this original early store.",
            ));
        }
        // Validate the complete local original before allowing its phase-only terminal successor.
        // 在允许仅改变阶段的终态后继之前，校验完整本地原请求。
        let plugin_id = &preparation.record.preparation.generation.plugin_id;
        Self::validate_preparation_record(
            &preparation.record,
            plugin_id,
            preparation.record.request_id,
        )?;
        if Arc::strong_count(&preparation.lease) != 1 {
            return Err(Error::new(
                Code::Busy,
                "An original preparation worker still shares its actual request file.",
            ));
        }
        Ok(())
    }

    /// Inspect the retained complete original under its already-held namespace using the unique sidecar and index validators.
    /// 在已持有命名空间内按唯一附属及索引校验器检查保留完整原请求。
    /// Return only exact local body or Preparing to exact Aborted successor; missing/foreign authority and reverse phases are errors.
    /// 仅返回精确本地正文或 Preparing 到精确 Aborted 后继；缺失、外来权威及逆向阶段属于错误。
    pub(super) fn retained_preparation_original_locked(
        &self,
        preparation: &VulcanPluginActivationPreparation,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparationRecord> {
        // Every path derives from the exact original generation namespace.
        // 每个路径均派生自精确原代际命名空间。
        let plugin_id = &preparation.record.preparation.generation.plugin_id;
        if self.read_activation(plugin_id)? != preparation.record.predecessor {
            return Err(Error::new(
                Code::Conflict,
                "Retained abandonment complete predecessor changed.",
            ));
        }
        // Journal aliases and sidecar aliases use the same sole canonical inventories as sealed inspection.
        // 日志及附属别名使用与密封检查相同的唯一规范清单。
        let request_ids = self.transaction_ids_locked(plugin_id)?;
        // No early original can legitimately have any of the four actual execution sidecars.
        // 任何早期原请求均不能合法持有四类实际执行附属文件。
        let sidecars = self.enumerate_preparation_sidecars_locked(plugin_id, &request_ids)?;
        if sidecars.contains_key(&preparation.record.request_id) {
            return Err(Error::invalid(
                "Early preparation cannot own request sidecars.",
            ));
        }
        // Actual missing or configured journals cannot be projected back into an early original.
        // 实际缺失或已配置日志不能投影回早期原请求。
        let Some(VulcanPluginActivationRequestRecord::Preparation(current)) =
            self.read_request_locked(plugin_id, preparation.record.request_id)?
        else {
            return Err(Error::new(
                Code::Conflict,
                "Retained abandonment original early journal is missing or already configured.",
            ));
        };
        // Permit only Preparing -> exact Aborted, never a reverse phase or changed immutable intent/schema.
        // 仅允许 Preparing 到精确 Aborted，绝不允许逆向阶段或改变不可变意图、模式。
        let mut abandoned = preparation.record.clone();
        abandoned.phase = Phase::Aborted;
        if current != preparation.record
            && !(preparation.record.phase == Phase::Preparing && current == abandoned)
        {
            return Err(Error::new(
                Code::Conflict,
                "Retained abandonment differs from its complete original or sole Aborted successor.",
            ));
        }
        // The complete current index must still name this actual full original; absence is never repaired here.
        // 完整当前索引仍须指向此实际完整原请求；此处绝不修复缺失。
        let current_request = VulcanPluginActivationRequestRecord::Preparation(current.clone());
        if self.indexed_request_locked(plugin_id)? != Some(current_request) {
            return Err(Error::new(
                Code::Conflict,
                "Retained abandonment lacks its exact complete original index.",
            ));
        }
        Ok(current)
    }

    /// Abandon `preparation` through its same actual owner after all sharing workers retire.
    /// 在全部共享工作者退役后，通过同一实际所有者放弃 `preparation`。
    /// Return the exact durable Aborted original; errors and unwinding retain the caller's last confirmed body and File.
    /// 返回精确持久 Aborted 原请求；错误及展开保留调用方最后确认正文及 File。
    /// This selects abandonment only: no package/credential deletion, callback execution or predecessor handoff is authorized.
    /// 此操作仅选择放弃：不授权插件包、凭据删除、回调执行或前驱交接。
    pub fn abort_activation_preparation_retained(
        &self,
        preparation: &mut VulcanPluginActivationPreparation,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationRequestRecord> {
        self.check_retained_preparation_binding(preparation)?;
        // Plugin identity remains fixed by the actual original owner.
        // 插件身份仍由实际原所有者固定。
        let plugin_id = &preparation.record.preparation.generation.plugin_id;
        // The original request File is borrowed throughout; only the short namespace fence is newly acquired.
        // 全程借用原请求 File；仅新取得短命名空间屏障。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        let current = self.retained_preparation_original_locked(preparation)?;
        // Only the phase changes for this exact retained original.
        // 此精确保留原请求仅改变阶段。
        let mut abandoned = preparation.record.clone();
        abandoned.phase = Phase::Aborted;
        if current.phase == Phase::Preparing {
            self.write_preparation_locked(&abandoned)?;
            #[cfg(test)]
            observation::after_write(&self.root, preparation.record.request_id)?;
            // A true writer result does not update local authority until journal/index/selection readback is exact.
            // 真实写入结果在日志、索引、选择回读精确之前不更新本地权威。
            let expected = VulcanPluginActivationRequestRecord::Preparation(abandoned.clone());
            if self.read_request_locked(plugin_id, preparation.record.request_id)?
                != Some(expected.clone())
                || self.indexed_request_locked(plugin_id)? != Some(expected)
                || self.read_activation(plugin_id)? != preparation.record.predecessor
            {
                return Err(Error::new(
                    Code::Conflict,
                    "Retained abandonment lacks exact Aborted original readback.",
                ));
            }
        }
        // Unknown successful writes are confirmed read-only with the same original lease on retry.
        // 未知成功写入在重试时使用同一原租约进行只读确认。
        preparation.record = abandoned.clone();
        Ok(VulcanPluginActivationRequestRecord::Preparation(abandoned))
    }

    /// Observe one original's actual Aborted writes, optionally losing a post-success acknowledgement; return same-thread cleanup.
    /// 观察单个原请求实际 Aborted 写入，可选丢失成功后确认；返回同线程清理。
    #[cfg(test)]
    pub(crate) fn observe_preparation_abort_writes_for_test(
        &self,
        request: Uuid,
        loss: Option<bool>,
    ) -> impl Drop {
        observation::arm(&self.root, request, loss)
    }

    /// Read only the scoped original's real successful Aborted writer count; no production state is synthesized.
    /// 仅读取作用域原请求真实成功 Aborted 写入次数；不合成生产状态。
    #[cfg(test)]
    pub(crate) fn preparation_abort_write_count_for_test(&self, request: Uuid) -> usize {
        observation::writes(&self.root, request)
    }
}

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
        /// Incremented only after the real matching Aborted atomic writer returned success.
        /// 仅在真实匹配 Aborted 原子写入器返回成功之后递增。
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
                "original abandonment observation scope overlaps"
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

    /// Count root/request's real successful Aborted write and apply its one loss; return explicit observation status.
    /// 统计 root／request 真实成功 Aborted 写入并应用其一次丢失；返回明确观测状态。
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
            Some(true) => panic!("Controlled post-success original abandonment worker panic."),
            Some(false) => Err(Error::new(
                Code::Storage,
                "Controlled loss of successful original abandonment observation.",
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
                .expect("original abandonment scope remains armed");
            assert_eq!(fault.key.0, root);
            assert_eq!(fault.key.1, request);
            fault.writes
        })
    }
}
