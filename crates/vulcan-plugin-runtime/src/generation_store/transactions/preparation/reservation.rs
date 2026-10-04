//! Actual reservation ownership precedes the first original journal and retains its namespace through publication.
//! 实际预留所有权先于首份原日志，并在发布全程保留其命名空间。

use super::*;
use VulcanPluginActivationTransactionPhase as Phase;

/// Exclusive original resources reserved before any durable Preparing claim exists.
/// 声称存在任何持久 Preparing 之前预留的排他原资源。
/// This non-cloneable owner holds real files; its intended record grants neither execution nor a durable phase.
/// 此不可克隆所有者持有真实文件；其预定记录不授予执行权限，也不表示持久阶段。
pub struct VulcanPluginActivationPreparationReservation {
    /// Original canonical store binding excludes publication through a foreign store.
    /// 原规范存储绑定排除通过外部存储发布。
    store_root: PathBuf,
    /// Exact immutable intended body is compared against real observations, never projected as already persisted.
    /// 精确不可变预定正文与真实观测比较，绝不投影为已经持久保存。
    intended: VulcanPluginActivationPreparationRecord,
    /// Complete index observed before reservation, including genuine absence.
    /// 预留之前观察到的完整索引，包括真实缺失。
    index_baseline: Option<VulcanPluginActivationRequestRecord>,
    /// Same actual exclusive request file is shared only during confirmed owner transfer.
    /// 同一实际排他请求文件仅在已确认所有者移交期间共享。
    lease: Arc<File>,
    /// Actual exclusive namespace prevents another manager from publishing while the original journal is absent.
    /// 实际排他命名空间在原日志缺失时阻止另一管理器发布。
    _namespace: File,
}

impl std::fmt::Debug for VulcanPluginActivationPreparationReservation {
    /// Format immutable plugin/request identities through `formatter`; return formatting status without private documents.
    /// 通过 `formatter` 格式化不可变插件／请求身份；返回格式化状态，不包含私有文档。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginActivationPreparationReservation")
            .field("plugin_id", &self.intended.preparation.generation.plugin_id)
            .field("request_id", &self.intended.request_id)
            .finish_non_exhaustive()
    }
}

impl VulcanPluginActivationPreparationReservation {
    /// Borrow the originally reserved intent; takes no input and grants no claim that its journal already exists.
    /// 借用原预留意图；无输入且不声称其日志已经存在。
    pub fn intent(&self) -> &VulcanPluginActivationPreparationIntent {
        &self.intended.preparation
    }

    /// Return the original canonical request identity independently from publication confirmation.
    /// 返回独立于发布确认的原规范请求身份。
    pub fn request_id(&self) -> Uuid {
        self.intended.request_id
    }

    /// Borrow the complete original predecessor; none means the originally unmanaged selection.
    /// 借用完整原前驱；空值表示原非受管选择。
    pub fn predecessor(&self) -> Option<&VulcanPluginActivation> {
        self.intended.predecessor.as_ref()
    }
}

impl VulcanPluginGenerationStore {
    /// Reserve request/source/fingerprint/predecessor/permissions/policy before the first journal; return actual exclusive ownership.
    /// 在首份日志之前预留 request／source／fingerprint／predecessor／permissions／policy；返回实际排他所有权。
    /// Register this owner strongly before publication; this call writes neither a request journal nor package/credential contents.
    /// 发布之前须强注册此所有者；此调用不写请求日志或插件包／凭据内容。
    pub fn reserve_activation_update_preparation(
        &self,
        request_id: Uuid,
        source: &Path,
        expected_content_fingerprint: &str,
        expected_predecessor: Option<&VulcanPluginActivation>,
        permissions: Vec<String>,
        update_policy: VulcanPluginUpdatePolicy,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparationReservation> {
        self.reserve_preparation_with_policy(
            request_id,
            source,
            expected_content_fingerprint,
            expected_predecessor,
            permissions,
            Some(update_policy),
        )
    }

    /// Share exact reservation for the declared legacy or policy-bearing protocol; all arguments preserve existing begin validation.
    /// 为声明的旧协议或携带策略协议共用精确预留；全部参数保留既有开始校验。
    /// Return an intended owner with a real namespace and request file, without claiming durable Preparing.
    /// 返回具有真实命名空间及请求文件的预定所有者，不声称存在持久 Preparing。
    pub(super) fn reserve_preparation_with_policy(
        &self,
        request_id: Uuid,
        source: &Path,
        expected_content_fingerprint: &str,
        expected_predecessor: Option<&VulcanPluginActivation>,
        mut permissions: Vec<String>,
        update_policy: Option<VulcanPluginUpdatePolicy>,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparationReservation> {
        if request_id.is_nil() {
            return Err(Error::invalid("Activation request identity cannot be nil."));
        }
        // Original source inspection allocates identities only in memory before any managed contents exist.
        // 原来源检查在任何受管内容存在之前，仅在内存中分配身份。
        let generation = self.plan_generation(source, expected_content_fingerprint)?;
        // Validate exact approved permissions against the same inspected immutable source identity.
        // 按同一已检查不可变来源身份校验精确已批准权限。
        let inspected = crate::inspect_user_plugin_package(source).map_err(Error::invalid)?;
        if inspected.id != generation.plugin_id
            || inspected.version.as_deref() != Some(generation.version.as_str())
            || inspected.content_fingerprint != generation.content_fingerprint
        {
            return Err(Error::invalid(
                "Source changed before preparation intent was recorded.",
            ));
        }
        permissions.sort();
        if permissions
            != crate::runtime::plugin_required_permissions(&inspected)
                .map_err(|error| Error::invalid(error.to_string()))?
        {
            return Err(Error::invalid(
                "Preparation requires exact complete approved permissions.",
            ));
        }
        // Keep the actual namespace through service registration and first publication, including when no journal exists yet.
        // 在服务注册及首次发布全程保留实际命名空间，包括日志尚不存在时。
        let namespace = self.namespace_lock(&generation.plugin_id, false)?;
        self.ensure_no_pending_activation_locked(&generation.plugin_id)?;
        if self
            .read_request_locked(&generation.plugin_id, request_id)?
            .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Original activation request already exists.",
            ));
        }
        // Full selected authority and original index are captured under the same real exclusive fence.
        // 完整已选权威及原索引在同一真实排他屏障下捕获。
        let predecessor = self.read_activation(&generation.plugin_id)?;
        if predecessor.as_ref() != expected_predecessor {
            return Err(Error::new(
                Code::Conflict,
                "Preparation predecessor changed before intent publication.",
            ));
        }
        // A terminal baseline is retained in full; a different terminal request cannot be accepted on retry.
        // 终态基线被完整保留；重试不能接受不同终态请求。
        let index_baseline = self.indexed_request_locked(&generation.plugin_id)?;
        // The sole successor cannot wrap or be recalculated after uncertain publication.
        // 唯一后继不能溢出，也不能在不确定发布之后重新计算。
        let activation_revision = predecessor
            .as_ref()
            .map_or(0, |record| record.revision)
            .checked_add(1)
            .ok_or_else(|| Error::invalid("Plugin activation revision is exhausted."))?;
        // This body is merely intended until the actual first writer and exact readback confirm it.
        // 此正文在实际首次写入器及精确回读确认之前，仅属于预定内容。
        let intended = VulcanPluginActivationPreparationRecord {
            schema_version: if update_policy.is_some() {
                UPDATE_RECORD_VERSION
            } else {
                PREPARATION_RECORD_VERSION
            },
            request_id,
            predecessor,
            preparation: VulcanPluginActivationPreparationIntent {
                generation,
                credential_revision: VulcanPluginCredentialRevision::allocate(),
                approved_permissions: permissions,
                activation_revision,
                update_policy,
            },
            phase: Phase::Preparing,
        };
        Self::validate_preparation_record(
            &intended,
            &intended.preparation.generation.plugin_id,
            request_id,
        )?;
        self.transaction_directory(&intended.preparation.generation.plugin_id, true)?;
        // Actual original-request ownership exists before the first journal writer is ever invoked.
        // 实际原请求所有权在首次日志写入器被调用之前已经存在。
        let lease =
            self.transaction_lease(&intended.preparation.generation.plugin_id, request_id)?;
        Ok(VulcanPluginActivationPreparationReservation {
            store_root: self.root.clone(),
            intended,
            index_baseline,
            lease: Arc::new(lease),
            _namespace: namespace,
        })
    }

    /// Observe reservation's exact actual journal through its retained namespace; return genuine absence or the confirmed Preparing body.
    /// 通过 reservation 保留的命名空间观察其精确实际日志；返回真实缺失或已确认 Preparing 正文。
    /// This never claims the intended body is durable, reacquires a lock, repairs an index or writes storage.
    /// 此操作绝不声称预定正文已持久化、不重新取得锁、不修复索引且不写存储。
    pub fn activation_preparation_reservation_record(
        &self,
        reservation: &VulcanPluginActivationPreparationReservation,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationPreparationRecord>> {
        self.validate_preparation_reservation(reservation)?;
        #[cfg(test)]
        observation::before_readback(&self.root, reservation.intended.request_id)?;
        match self.read_request_locked(
            &reservation.intended.preparation.generation.plugin_id,
            reservation.intended.request_id,
        )? {
            None => Ok(None),
            Some(VulcanPluginActivationRequestRecord::Preparation(record))
                if record == reservation.intended =>
            {
                Ok(Some(record))
            }
            _ => Err(Error::new(
                Code::Conflict,
                "Reserved original journal differs from its exact intended Preparing record.",
            )),
        }
    }

    /// Publish or confirm reservation's exact first journal/index; return Preparing sharing the same actual request file.
    /// 发布或确认 reservation 精确首份日志／索引；返回共享同一实际请求文件的 Preparing。
    /// Errors retain the caller's actual namespace and original owner; successful service replacement must drop reservation before copying.
    /// 错误保留调用方实际命名空间及原所有者；服务成功替换后须在复制之前释放 reservation。
    pub fn publish_activation_preparation_retained(
        &self,
        reservation: &mut VulcanPluginActivationPreparationReservation,
    ) -> VulcanPluginStoreResult<VulcanPluginActivationPreparation> {
        self.validate_preparation_reservation(reservation)?;
        if Arc::strong_count(&reservation.lease) != 1 {
            return Err(Error::new(
                Code::Busy,
                "Returned preparation still shares the original reserved request owner.",
            ));
        }
        // Namespace ownership already belongs to this typed reservation; reacquisition would conflict with its own real file.
        // 命名空间所有权已属于此类型化预留；重新取得会与其自身真实文件冲突。
        let plugin_id = &reservation.intended.preparation.generation.plugin_id;
        // Inspection and publication accept the same three actual original-journal/index combinations.
        // 检查与发布接受相同三种实际原日志、索引组合。
        let (observed, indexed) = self.preparation_reservation_checkpoint(reservation)?;
        // Exact own index is recognized from the actual full body, never a copied UUID alone.
        // 精确自有索引从实际完整正文识别，绝不仅依据复制 UUID。
        let own_index = Some(VulcanPluginActivationRequestRecord::Preparation(
            reservation.intended.clone(),
        ));
        if observed.is_none() {
            self.write_preparation_locked(&reservation.intended)?;
            #[cfg(test)]
            observation::after_write(&self.root, reservation.intended.request_id, true)?;
            if self
                .activation_preparation_reservation_record(reservation)?
                .as_ref()
                != Some(&reservation.intended)
                || self.indexed_request_locked(plugin_id)? != reservation.index_baseline
            {
                return Err(Error::new(
                    Code::Conflict,
                    "First original journal lacks exact Preparing and original index readback.",
                ));
            }
        }
        if indexed != own_index {
            self.index_request_locked(plugin_id, reservation.intended.request_id)?;
            #[cfg(test)]
            observation::after_write(&self.root, reservation.intended.request_id, false)?;
        }
        if self
            .activation_preparation_reservation_record(reservation)?
            .as_ref()
            != Some(&reservation.intended)
            || self.indexed_request_locked(plugin_id)? != own_index
        {
            return Err(Error::new(
                Code::Conflict,
                "Original preparation publication lacks its exact journal and index readback.",
            ));
        }
        Ok(VulcanPluginActivationPreparation {
            store_root: self.root.clone(),
            record: reservation.intended.clone(),
            lease: Arc::clone(&reservation.lease),
        })
    }

    /// Validate reservation's exact store, intended original, selected predecessor and full index checkpoint.
    /// 校验预留精确存储、预定原请求、已选前驱及完整索引检查点。
    /// Return actual journal/index observations for absence plus baseline, Preparing plus baseline, or Preparing plus own index.
    /// 返回缺失配基线、Preparing 配基线、Preparing 配自有索引三种实际日志、索引观察。
    pub(super) fn preparation_reservation_checkpoint(
        &self,
        reservation: &VulcanPluginActivationPreparationReservation,
    ) -> VulcanPluginStoreResult<(
        Option<VulcanPluginActivationPreparationRecord>,
        Option<VulcanPluginActivationRequestRecord>,
    )> {
        self.validate_preparation_reservation(reservation)?;
        // The retained namespace permits actual reads without acquiring a second conflicting file.
        // 保留命名空间允许实际读取，无需取得第二个冲突文件。
        let plugin_id = &reservation.intended.preparation.generation.plugin_id;
        if self.read_activation(plugin_id)? != reservation.intended.predecessor {
            return Err(Error::new(
                Code::Conflict,
                "Reserved preparation complete predecessor changed.",
            ));
        }
        // Preserve the exact original readback and its existing fault-injection position before the indexed read.
        // 保留精确原回读及其既有故障注入位置，先于索引读取。
        let observed = self.activation_preparation_reservation_record(reservation)?;
        // Compare complete baseline bodies, rather than accepting a terminal phase or matching UUID alone.
        // 比较完整基线正文，而不是仅接受终态阶段或匹配 UUID。
        let indexed = self.indexed_request_locked(plugin_id)?;
        // Actual exact Preparing must exist before its own index can be acknowledged.
        // 确认自有索引之前必须存在实际精确 Preparing。
        let own_index = Some(VulcanPluginActivationRequestRecord::Preparation(
            reservation.intended.clone(),
        ));
        if indexed != reservation.index_baseline && !(observed.is_some() && indexed == own_index) {
            return Err(Error::new(
                Code::Conflict,
                "Reserved preparation index differs from its complete original baseline or exact published original.",
            ));
        }
        Ok((observed, indexed))
    }

    /// Validate reservation's original store and immutable intended body without reacquiring its privately retained files.
    /// 不重新取得私有保留文件，校验 reservation 原存储及不可变预定正文。
    /// Return unit only for this store's original Preparing intent; no durable claim is inferred here.
    /// 仅此存储原 Preparing 意图返回空值；此处不推导持久声明。
    fn validate_preparation_reservation(
        &self,
        reservation: &VulcanPluginActivationPreparationReservation,
    ) -> VulcanPluginStoreResult<()> {
        if reservation.store_root != self.root || reservation.intended.phase != Phase::Preparing {
            return Err(Error::new(
                Code::Conflict,
                "Original preparation reservation belongs to another store or intent.",
            ));
        }
        Self::validate_preparation_record(
            &reservation.intended,
            &reservation.intended.preparation.generation.plugin_id,
            reservation.intended.request_id,
        )
    }

    /// Observe request's real journal/index writers with optional post-success panic/error and first owned readback loss.
    /// 观察 request 真实日志／索引写入器，可选成功后 panic／错误及首次已持有所有者回读丢失。
    /// Return scoped same-thread cleanup; all controls exist only in tests.
    /// 返回作用域同线程清理；全部控制仅存在于测试。
    #[cfg(test)]
    pub(crate) fn observe_preparation_reservation_writes_for_test(
        &self,
        request: Uuid,
        after_journal: Option<bool>,
        after_index: Option<bool>,
        lose_readback: bool,
    ) -> impl Drop {
        observation::arm(
            &self.root,
            request,
            after_journal,
            after_index,
            lose_readback,
        )
    }

    /// Read request's actual successful journal/index counts in its retained scope; return no production ownership or phase.
    /// 读取保留作用域内 request 实际成功日志／索引次数；不返回生产所有权或阶段。
    #[cfg(test)]
    pub(crate) fn preparation_reservation_write_counts_for_test(
        &self,
        request: Uuid,
    ) -> (usize, usize) {
        observation::writes(&self.root, request)
    }
}

/// Same-thread faults observe only successful real original journal/index writes and actual owned readbacks.
/// 同线程故障仅观察成功真实原日志／索引写入及实际已持有所有者回读。
#[cfg(test)]
mod observation {
    use super::*;

    /// Exact original observation and its optional post-success failures.
    /// 精确原观测及其可选成功后失败。
    struct Fault {
        /// Root/request binding excludes unrelated physical writers.
        /// 根／请求绑定排除无关物理写入者。
        key: (PathBuf, Uuid),
        /// False returns an error; true panics after the real journal writer succeeds.
        /// false 返回错误；true 在真实日志写入器成功之后 panic。
        after_journal: Option<bool>,
        /// False returns an error; true panics after the real index writer succeeds.
        /// false 返回错误；true 在真实索引写入器成功之后 panic。
        after_index: Option<bool>,
        /// Lose only the first owned readback after an actual successful write.
        /// 仅丢失实际成功写入之后的首次已持有所有者回读。
        lose_readback: bool,
        /// Successful actual journal/index writer counts never substitute for durable production state.
        /// 成功实际日志／索引写入次数绝不替代持久生产状态。
        writes: (usize, usize),
    }

    std::thread_local! {
        /// One original test observation exists on this thread without a production fault switch.
        /// 此线程存在单个原测试观测，不提供生产故障开关。
        static ARMED: std::cell::RefCell<Option<Fault>> = const { std::cell::RefCell::new(None) };
    }

    /// Non-Send cleanup retains and retires one original observation scope during normal exit or unwinding.
    /// 不可跨线程清理在正常退出或展开期间保留并退役单个原观测作用域。
    pub(super) struct Scope {
        /// Only this exact original observation is cleared.
        /// 仅清除此精确原观测。
        key: (PathBuf, Uuid),
        /// Cleanup belongs to the original armed thread.
        /// 清理属于原启用线程。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }

    impl Drop for Scope {
        /// Clear this exact observation; accepts no input and returns no value.
        /// 清除此精确观测；无输入且不返回值。
        fn drop(&mut self) {
            ARMED.with(|armed| {
                // Never clear another scoped original's observation.
                // 绝不清除另一作用域原请求的观测。
                let mut armed = armed.borrow_mut();
                if armed.as_ref().is_some_and(|fault| fault.key == self.key) {
                    armed.take();
                }
            });
        }
    }

    /// Arm exact root/request's optional journal/index loss and owned readback loss; return same-thread cleanup.
    /// 启用精确 root／request 可选日志／索引丢失及已持有所有者回读丢失；返回同线程清理。
    pub(super) fn arm(
        root: &Path,
        request: Uuid,
        after_journal: Option<bool>,
        after_index: Option<bool>,
        lose_readback: bool,
    ) -> Scope {
        // Observation allocates no storage or request ownership.
        // 观测不分配存储或请求所有权。
        let key = (root.to_path_buf(), request);
        ARMED.with(|armed| {
            // Tests must retire the prior scope before arming another original.
            // 测试必须在启用另一原请求之前退役先前作用域。
            let mut armed = armed.borrow_mut();
            assert!(
                armed.is_none(),
                "original reservation observation scopes overlap"
            );
            *armed = Some(Fault {
                key: key.clone(),
                after_journal,
                after_index,
                lose_readback,
                writes: (0, 0),
            });
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }

    /// Observe a real successful root/request writer; journal selects its checkpoint and errors preserve all actual files.
    /// 观察真实成功 root／request 写入器；journal 选择其检查点，错误保留全部实际文件。
    pub(super) fn after_write(
        root: &Path,
        request: Uuid,
        journal: bool,
    ) -> VulcanPluginStoreResult<()> {
        // Both callers invoke this hook only after their unchanged real writer returns success.
        // 两个调用方均仅在其未变真实写入器返回成功之后调用此钩子。
        let loss = ARMED.with(|armed| {
            // Release the scoped mutable observation before an error or panic is raised.
            // 在返回错误或触发 panic 之前释放作用域可变观测。
            let mut armed = armed.borrow_mut();
            if let Some(fault) = armed.as_mut()
                && fault.key.0 == root
                && fault.key.1 == request
            {
                if journal {
                    fault.writes.0 += 1;
                    fault.after_journal.take()
                } else {
                    fault.writes.1 += 1;
                    fault.after_index.take()
                }
            } else {
                None
            }
        });
        match loss {
            Some(true) => panic!("Controlled post-success original reservation worker panic."),
            Some(false) => Err(Error::new(
                Code::Storage,
                "Controlled loss of successful original reservation write observation.",
            )),
            None => Ok(()),
        }
    }

    /// Lose only root/request's first actual owned observation after a successful write; canonical bytes remain unchanged.
    /// 仅丢失 root／request 成功写入之后的首次实际已持有所有者观测；规范字节保持不变。
    pub(super) fn before_readback(root: &Path, request: Uuid) -> VulcanPluginStoreResult<()> {
        // No failure can activate before this same original's real journal or index writer executed.
        // 此相同原请求的真实日志或索引写入器执行之前，不会启用任何失败。
        let lost = ARMED.with(|armed| {
            // An observation loss is consumed once while the original real owner stays retained.
            // 原真实所有者继续被保留时，观测丢失仅消费一次。
            let mut armed = armed.borrow_mut();
            if let Some(fault) = armed.as_mut()
                && fault.key.0 == root
                && fault.key.1 == request
                && fault.lose_readback
                && fault.writes != (0, 0)
                && fault.after_journal.is_none()
                && fault.after_index.is_none()
            {
                fault.lose_readback = false;
                true
            } else {
                false
            }
        });
        if lost {
            Err(Error::new(
                Code::Storage,
                "Controlled loss of original reserved-owner readback.",
            ))
        } else {
            Ok(())
        }
    }

    /// Read actual successful root/request journal/index counts; reject an absent or unrelated scope.
    /// 读取实际成功 root／request 日志／索引次数；拒绝缺失或无关作用域。
    pub(super) fn writes(root: &Path, request: Uuid) -> (usize, usize) {
        ARMED.with(|armed| {
            // Tests never substitute a fabricated zero for a missing original observation.
            // 测试绝不以虚构零值替代缺失原观测。
            let armed = armed.borrow();
            // Counts belong only to the exact retained scope.
            // 次数仅属于精确保留作用域。
            let fault = armed
                .as_ref()
                .expect("original reservation scope remains armed");
            assert_eq!(fault.key.0, root);
            assert_eq!(fault.key.1, request);
            fault.writes
        })
    }
}
