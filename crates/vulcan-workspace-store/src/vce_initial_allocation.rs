//! Durable first-allocation ownership, independent from published project bindings.
//! 独立于已发布项目绑定的首次分配持久归属。

use super::*;

/// Exact receipt retained before either initial vector table can be created.
/// 创建任一首次向量表之前保留的精确回执。
#[derive(Deserialize)]
pub(super) struct InitialAllocation {
    /// Stable project that owns this private database.
    /// 持有此私有数据库的稳定项目。
    project_id: String,
    /// Unique attempt identity; a later retry cannot reuse an earlier caller's authority.
    /// 唯一尝试身份；后续重试不能复用先前调用方的权限。
    allocation_id: String,
    /// Exact serialized target, including root, space and configuration revision.
    /// 精确序列化目标，包含根、空间及配置修订。
    target_json: String,
    /// Immutable namespace expected at final binding publication.
    /// 最终绑定发布时要求的不可变命名空间。
    namespace_id: String,
    /// Exact content table, validated against the serialized target before cleanup.
    /// 精确内容表，清理前按序列化目标校验。
    content_table: String,
    /// Exact path table, validated against the serialized target before cleanup.
    /// 精确路径表，清理前按序列化目标校验。
    path_table: String,
    /// Allocating, cleanup, or published; an interrupted allocating owner is never adopted.
    /// 分配中、待清理或已发布；绝不接管被中断的分配中所有者。
    state: String,
    /// One only after successful content creation was acknowledged durably.
    /// 仅成功创建内容表并持久确认后为一。
    content_created: i64,
    /// One only after successful path creation was acknowledged durably.
    /// 仅成功创建路径表并持久确认后为一。
    path_created: i64,
}

impl InitialAllocation {
    /// Validate stored ownership before using any persisted physical name.
    /// 使用任何持久物理名称之前校验存储归属。
    /// `project_id` is the database owner; returns the exact decoded target or an explicit error.
    /// `project_id` 为数据库所有者；返回精确解码目标或明确错误。
    fn validate(&self, project_id: &str) -> Result<VceProjectIndexConfig, String> {
        // The serialized target is the sole source of immutable physical identity.
        // 序列化目标是不可变物理身份的唯一来源。
        let config: VceProjectIndexConfig = serde_json::from_str(&self.target_json)
            .map_err(|error| format!("Decode VCE initial allocation target: {error}"))?;
        config.validate()?;
        // Parse the persisted attempt rather than treating malformed identity as another valid owner.
        // 解析持久尝试身份，不能把损坏身份视为另一有效所有者。
        let allocation_id = uuid::Uuid::parse_str(&self.allocation_id)
            .map_err(|error| format!("Decode VCE initial allocation identity: {error}"))?;
        if self.project_id != project_id
            || config.project_id != project_id
            || self.namespace_id != config.binding.namespace_id
            || allocation_id.is_nil()
            || !matches!(self.state.as_str(), "allocating" | "cleanup" | "published")
            || !matches!(self.content_created, 0 | 1)
            || !matches!(self.path_created, 0 | 1)
        {
            return Err(String::from("VCE initial allocation ownership is invalid"));
        }
        // Both names must be derived from the same exact model and namespace contract.
        // 两个名称必须由同一精确模型及命名空间契约派生。
        for (channel, stored) in [("content", &self.content_table), ("path", &self.path_table)] {
            if *stored
                != vector_table_name(
                    channel,
                    &config.vector_space_id,
                    &config.physical_fingerprint,
                    &config.preprocessing_version,
                    &config.chunker_version,
                    config.dimension,
                )
            {
                return Err(String::from(
                    "VCE initial allocation table identity differs from its target",
                ));
            }
        }
        Ok(config)
    }

    /// Guard publication and mark this receipt published in the binding's existing SQL transaction.
    /// 在绑定的既有 SQL 事务中保护发布，并将此回执标记为已发布。
    /// No parameters; returns statements to run after binding insertion and before transaction commit.
    /// 无参数；返回在绑定插入之后、事务提交之前执行的语句。
    pub(super) fn publication_sql(&self) -> String {
        // Every interpolated value is escaped; the guard's foreign key now sees the transaction's binding.
        // 每个插值均已转义；此时守卫外键可见事务中的绑定。
        let namespace = sqlite_string_literal(&self.namespace_id);
        // Attempt and target must both match; table presence alone never proves ownership.
        // 尝试与目标必须同时匹配；仅表存在绝不证明归属。
        let allocation = sqlite_string_literal(&self.allocation_id);
        // Target includes configuration revision, preventing stale publication into a changed project contract.
        // 目标包含配置修订，防止过期发布进入已变化的项目契约。
        let target = sqlite_string_literal(&self.target_json);
        format!(
            "INSERT OR ROLLBACK INTO vce_commit_guards(namespace_id, valid) VALUES ('{namespace}', CASE WHEN EXISTS(SELECT 1 FROM vce_initial_allocations WHERE namespace_id = '{namespace}' AND allocation_id = '{allocation}' AND target_json = '{target}' AND state = 'allocating' AND content_created = 1 AND path_created = 1) THEN 1 ELSE 0 END);\nUPDATE vce_initial_allocations SET state = 'published', last_error = NULL WHERE allocation_id = '{allocation}';\nDELETE FROM vce_commit_guards WHERE namespace_id = '{namespace}';"
        )
    }
}

impl VceDataPlaneHandle {
    /// Read the one unfinished allocation receipt without allocating or adopting physical tables.
    /// 读取唯一未完成分配回执，不分配或接管物理表。
    /// `project_id` is the exact database owner; returns an optional validated receipt.
    /// `project_id` 为精确数据库所有者；返回可选的已校验回执。
    async fn initial_allocation(
        &self,
        project_id: &str,
    ) -> Result<Option<InitialAllocation>, String> {
        self.validate_project_owner(project_id)?;
        // The partial unique index permits published namespaces but only one unfinished project owner.
        // 部分唯一索引允许保留已发布命名空间，但每个项目只能有一个未完成所有者。
        let mut rows: Vec<InitialAllocation> = self.query_rows(
            "SELECT project_id, allocation_id, target_json, namespace_id, content_table, path_table, state, content_created, path_created FROM vce_initial_allocations WHERE project_id = ? AND state != 'published';",
            vec![sqlite_text(project_id)],
        ).await?;
        if rows.len() > 1 {
            return Err(String::from(
                "VCE initial allocation has multiple project owners",
            ));
        }
        if let Some(receipt) = rows.first() {
            receipt.validate(project_id)?;
        }
        Ok(rows.pop())
    }

    /// Reserve an exact attempt, recover acknowledged cleanup, then create the two non-overwriting tables.
    /// 预留精确尝试，恢复已确认清理，再创建两张不覆盖的表。
    /// `config` and table names are validated host facts; the caller retains the project writer lease.
    /// `config` 及表名为已校验宿主事实；调用方保留项目写入租约。
    /// Returns durable ownership for atomic binding publication, or the primary and cleanup failures.
    /// 返回用于原子绑定发布的持久归属，失败时返回主错误及清理错误。
    pub(super) fn allocate_initial_tables<'a>(
        &'a self,
        config: &'a VceProjectIndexConfig,
        content_table: &'a str,
        path_table: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<InitialAllocation, String>> + Send + 'a>,
    > {
        // Keep the controller allocation/cleanup state machine off every enclosing host future's stack.
        // 将控制器分配／清理状态机移出各层宿主任务的栈，避免沿初始化调用链放大。
        Box::pin(async move {
            self.validate_project_owner(&config.project_id)?;
            config.validate()?;
            // Only an explicitly failed attempt permits cleanup/retry; crash or lost acknowledgement is not adopted.
            // 只有明确失败的尝试允许清理／重试；不接管崩溃或丢失回执的尝试。
            if let Some(prior) = self.initial_allocation(&config.project_id).await? {
                if prior.state != "cleanup" {
                    return Err(String::from(
                        "VCE initial allocation is already owned; unresolved ownership requires explicit recovery",
                    ));
                }
                self.cleanup_initial_allocation(&prior).await?;
                self.execute(
                "DELETE FROM vce_initial_allocations WHERE project_id = ? AND allocation_id = ? AND state = 'cleanup' AND content_created = 0 AND path_created = 0;",
                vec![sqlite_text(&prior.project_id), sqlite_text(&prior.allocation_id)],
                "Retire acknowledged VCE initial allocation cleanup",
            ).await?;
            }
            // A new retry receives a new token; the complete immutable target remains serialized in the receipt.
            // 新重试取得新令牌；完整不可变目标保留在序列化回执中。
            let allocation = InitialAllocation {
                project_id: config.project_id.clone(),
                allocation_id: uuid::Uuid::new_v4().to_string(),
                target_json: serde_json::to_string(config)
                    .map_err(|error| format!("Encode VCE initial allocation: {error}"))?,
                namespace_id: config.binding.namespace_id.clone(),
                content_table: content_table.to_owned(),
                path_table: path_table.to_owned(),
                state: String::from("allocating"),
                content_created: 0,
                path_created: 0,
            };
            allocation.validate(&config.project_id)?;
            self.execute(
            "INSERT INTO vce_initial_allocations(project_id, allocation_id, target_json, namespace_id, content_table, path_table, state, content_created, path_created) VALUES (?, ?, ?, ?, ?, ?, 'allocating', 0, 0);",
            vec![sqlite_text(&allocation.project_id), sqlite_text(&allocation.allocation_id), sqlite_text(&allocation.target_json), sqlite_text(&allocation.namespace_id), sqlite_text(content_table), sqlite_text(path_table)],
            "Reserve VCE initial allocation ownership",
        ).await?;
            // A zero flag never authorizes deleting a collision or an operation whose success was not acknowledged.
            // 零标志绝不授权删除冲突表或成功尚未确认的操作目标。
            for (column, table) in [
                ("content_created", content_table),
                ("path_created", path_table),
            ] {
                if let Err(primary) = self.ensure_vector_table(table, config.dimension).await {
                    return Err(self.fail_initial_allocation(&allocation, primary).await);
                }
                // Fixed column names come solely from the two typed channels above, not user input.
                // 固定列名仅来自上方两种明确通道，不来自用户输入。
                if let Err(primary) = self.execute(
                &format!("UPDATE vce_initial_allocations SET {column} = 1 WHERE allocation_id = ? AND state = 'allocating';"),
                vec![sqlite_text(&allocation.allocation_id)],
                "Acknowledge VCE initial table creation",
            ).await {
                return Err(self.fail_initial_allocation(&allocation, primary).await);
            }
            }
            Ok(allocation)
        })
    }

    /// Mark a failed owned attempt for cleanup and preserve all cleanup diagnostics.
    /// 将失败的自有尝试标记为待清理，并保留全部清理诊断。
    /// `allocation` is the exact token and `primary` the observed failure; returns a non-success diagnostic.
    /// `allocation` 为精确令牌，`primary` 为已观察失败；返回非成功诊断。
    pub(super) async fn fail_initial_allocation(
        &self,
        allocation: &InitialAllocation,
        primary: String,
    ) -> String {
        // Persist failure before deletion so cleanup failures remain visible to the next explicit retry.
        // 删除前持久化失败，使清理失败对下一次明确重试保持可见。
        let marked = self.execute(
            "UPDATE vce_initial_allocations SET state = 'cleanup', last_error = ? WHERE allocation_id = ? AND state = 'allocating';",
            vec![sqlite_text(&primary), sqlite_text(&allocation.allocation_id)],
            "Mark failed VCE initial allocation",
        ).await;
        if let Err(error) = marked {
            return format!("{primary}; initial allocation failure recording failed: {error}");
        }
        match self.cleanup_initial_allocation(allocation).await {
            Ok(()) => primary,
            Err(error) => format!("{primary}; acknowledged initial table cleanup failed: {error}"),
        }
    }

    /// Delete only acknowledged tables of an unpublished, explicitly failed initial attempt.
    /// 仅删除未发布且明确失败的首次尝试已确认创建的表。
    /// `allocation` is exact prior ownership; returns after durable cleanup acknowledgements or an explicit error.
    /// `allocation` 为精确先前归属；持久清理确认后返回，失败时明确报错。
    async fn cleanup_initial_allocation(
        &self,
        allocation: &InitialAllocation,
    ) -> Result<(), String> {
        // Re-read durable flags; the caller's snapshot predates physical table creation.
        // 重读持久标志；调用方快照早于物理表创建。
        let current = self
            .initial_allocation(&allocation.project_id)
            .await?
            .ok_or_else(|| String::from("VCE initial allocation cleanup lost its receipt"))?;
        if current.allocation_id != allocation.allocation_id
            || current.target_json != allocation.target_json
            || current.state != "cleanup"
        {
            return Err(String::from(
                "VCE initial allocation cleanup requires the exact failed owner",
            ));
        }
        // Protect every namespace/table reference, including inactive bindings and lost publication replies.
        // 保护每个命名空间／表引用，包括非活动绑定及丢失的发布回复。
        let bindings: Vec<CountRow> = self
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_project_bindings WHERE namespace_id = ? OR content_table IN (?, ?) OR path_table IN (?, ?);",
                vec![sqlite_text(&current.namespace_id), sqlite_text(&current.content_table), sqlite_text(&current.path_table), sqlite_text(&current.content_table), sqlite_text(&current.path_table)],
            )
            .await?;
        if !matches!(bindings.as_slice(), [row] if row.count == 0) {
            return Err(String::from(
                "VCE initial cleanup cannot delete tables referenced by a project binding",
            ));
        }
        // Only positive durable acknowledgements authorize physical deletion; foreign collisions stay untouched.
        // 只有明确持久确认授权物理删除；外部碰撞保持不变。
        for (column, created, table) in [
            (
                "content_created",
                current.content_created,
                &current.content_table,
            ),
            ("path_created", current.path_created, &current.path_table),
        ] {
            if created == 0 {
                continue;
            }
            self.client
                .drop_lancedb_table(VCE_SPACE_ID, &self.lancedb_binding_id, table)
                .await
                .map_err(|error| format!("Drop owned initial VCE table `{table}`: {error}"))?;
            self.execute(
                &format!("UPDATE vce_initial_allocations SET {column} = 0 WHERE allocation_id = ? AND state = 'cleanup';"),
                vec![sqlite_text(&allocation.allocation_id)],
                "Acknowledge VCE initial table cleanup",
            ).await?;
        }
        Ok(())
    }
}

/// Define first-allocation receipts independently of project bindings that do not yet exist.
/// 独立于尚不存在的项目绑定定义首次分配回执。
/// No parameters; returns the complete current-format schema, without migration or external database changes.
/// 无参数；返回完整当前格式结构，不包含迁移或外部数据库变更。
pub(super) fn initial_allocation_schema_script() -> &'static str {
    r#"
CREATE TABLE IF NOT EXISTS vce_initial_allocations (
    project_id TEXT NOT NULL CHECK(length(trim(project_id)) > 0),
    allocation_id TEXT NOT NULL UNIQUE CHECK(length(allocation_id) = 36),
    target_json TEXT NOT NULL,
    namespace_id TEXT PRIMARY KEY CHECK(length(trim(namespace_id)) > 0),
    content_table TEXT NOT NULL CHECK(length(trim(content_table)) > 0),
    path_table TEXT NOT NULL CHECK(length(trim(path_table)) > 0),
    state TEXT NOT NULL CHECK(state IN ('allocating', 'cleanup', 'published')),
    content_created INTEGER NOT NULL CHECK(content_created IN (0, 1)),
    path_created INTEGER NOT NULL CHECK(path_created IN (0, 1)),
    last_error TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_vce_initial_unfinished_project
ON vce_initial_allocations(project_id) WHERE state != 'published';
"#
}

/// Verify exact rollback, durable cleanup retry and published-table protection with a private controller.
/// 使用私有控制器验证精确回滚、持久清理重试与已发布表保护。
/// `controller` owns temporary storage, `config` is the isolated target, and `interrupt_cleanup` disables only its vector binding.
/// `controller` 持有临时存储，`config` 为隔离目标，`interrupt_cleanup` 仅禁用其向量绑定。
/// Returns after assertions and closes all fixture bindings; this does not exercise unknown-success recovery.
/// 断言后返回并关闭全部夹具绑定；不验证未知成功恢复。
#[cfg(test)]
pub(super) fn verify_initial_allocation_failures(
    controller: &ControllerRuntimeHandle,
    config: VceProjectIndexConfig,
    interrupt_cleanup: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
    // Erase the large fixture future before it enters the existing integration test's stack frame.
    // 在大型夹具任务进入既有集成测试栈帧前擦除其具体类型。
    Box::pin(async move {
        // Catalog paths are actual host allocations, not guessed storage locations.
        // 目录路径来自实际宿主分配，不猜测存储位置。
        let catalog = crate::vce_storage_catalog::VceStorageCatalog::bootstrap(controller)
            .await
            .expect("open initial failure catalog");
        // A distinct stable project keeps all injected faults inside this fixture.
        // 独立稳定项目使所有注入故障局限于此夹具。
        let paths = catalog
            .allocate_project(&config.project_id)
            .await
            .expect("allocate initial failure paths");
        // Only this project's schema and private controller bindings are prepared.
        // 仅准备此项目的结构及私有控制器绑定。
        let store = prepare_vce_storage_binding(
            controller.client.clone(),
            paths.root,
            config.project_id.clone(),
            true,
        )
        .expect("prepare initial failure store");
        store
            .activate_bindings(true)
            .await
            .expect("activate initial failure store");
        catalog
            .close()
            .await
            .expect("close initial failure catalog");
        // Exact physical names use the production identity function for both channels.
        // 两个通道的精确物理名称使用生产身份函数。
        let tables = ["content", "path"].map(|channel| {
            vector_table_name(
                channel,
                &config.vector_space_id,
                &config.physical_fingerprint,
                &config.preprocessing_version,
                &config.chunker_version,
                config.dimension,
            )
        });
        if interrupt_cleanup {
            // Reserve and acknowledge both creations before injecting a known cleanup failure.
            // 在注入已知清理失败之前预留并确认两次创建。
            let allocation = store
                .allocate_initial_tables(&config, &tables[0], &tables[1])
                .await
                .expect("allocate tables before cleanup interruption");
            store
                .client
                .disable_lancedb(VCE_SPACE_ID, &store.lancedb_binding_id)
                .await
                .expect("disable only fixture vector binding");
            // The real deletion request fails while SQLite retains the exact positive ownership flags.
            // 真实删除请求失败时，SQLite 保留精确的正向归属标志。
            let failure = store
                .fail_initial_allocation(
                    &allocation,
                    String::from("fixture: known allocation failure"),
                )
                .await;
            assert!(failure.contains("fixture: known allocation failure"));
            assert!(failure.contains("acknowledged initial table cleanup failed"));
        } else {
            store.execute(
            "CREATE TRIGGER fixture_reject_initial_binding BEFORE INSERT ON vce_project_bindings BEGIN SELECT RAISE(ROLLBACK, 'fixture: reject initial binding'); END;",
            Vec::new(), "Install fixture-only registration failure",
        ).await.expect("install initial registration failure");
            // Public factory owns the real writer lease and executes both physical creates and SQL publication.
            // 公开工厂持有真实写入租约，并执行两次物理创建及 SQL 发布。
            let factory = bootstrap_vce_data_plane(controller)
                .await
                .expect("open registration failure factory");
            // The injected transaction rollback must be returned, never translated into initialization success.
            // 注入的事务回滚必须返回，绝不能转成初始化成功。
            let failure = factory
                .create_project_index(config.clone())
                .await
                .expect_err("registration rollback must fail");
            assert!(
                failure.contains("fixture: reject initial binding"),
                "{failure}"
            );
            factory
                .shutdown()
                .await
                .expect("close registration failure factory");
            store
                .execute(
                    "DROP TRIGGER fixture_reject_initial_binding;",
                    Vec::new(),
                    "Remove fixture-only registration failure",
                )
                .await
                .expect("remove initial registration failure");
            store
                .close_bindings()
                .await
                .expect("close failed registration inspection binding");
        }
        // Reopening the actual database proves recovery reads durable state rather than a caller snapshot.
        // 重开实际数据库证明恢复读取持久状态，而不是调用方快照。
        store
            .activate_bindings(false)
            .await
            .expect("reopen persisted initial failure");
        // One unfinished receipt must retain the original target and acknowledged cleanup phase.
        // 唯一未完成回执必须保留原始目标及已确认清理阶段。
        let failed = store
            .initial_allocation(&config.project_id)
            .await
            .expect("read durable initial failure")
            .expect("retain initial failure ownership");
        assert_eq!(failed.state, "cleanup");
        assert_eq!(
            failed.target_json,
            serde_json::to_string(&config).expect("serialize exact target")
        );
        assert_eq!(failed.content_created, i64::from(interrupt_cleanup));
        assert_eq!(failed.path_created, i64::from(interrupt_cleanup));
        assert!(
            store
                .load_active_project_config(&config.project_id)
                .await
                .expect("read unpublished initial failure")
                .is_none()
        );
        // Release the inspection owner before the public factory reopens the same physical SQLite file.
        // 公开工厂重开同一物理 SQLite 文件之前释放检查所有者。
        store
            .close_bindings()
            .await
            .expect("release initial recovery inspection owner");
        // Retrying through the public factory must clean its own tables, assign a new attempt, and publish atomically.
        // 通过公开工厂重试必须清理自有表、分配新尝试，并原子发布。
        let retry_factory = bootstrap_vce_data_plane(controller)
            .await
            .expect("open initial recovery factory");
        // A first physical binding is not a completed content index or an engine checkpoint.
        // 首次物理绑定不等于已完成的内容索引或引擎检查点。
        let index = retry_factory
            .create_project_index(config.clone())
            .await
            .expect("retry durable initial allocation");
        // Subsequent assertions use the factory's exact reopened binding, never a competing inspection pool.
        // 后续断言使用工厂精确重开的绑定，绝不使用竞争的检查连接池。
        let recovered_store = &index.data_plane;
        assert!(
            index
                .load_state_snapshot()
                .await
                .expect("inspect uninitialized snapshot")
                .is_none()
        );
        assert!(
            recovered_store
                .initial_allocation(&config.project_id)
                .await
                .expect("read completed ownership")
                .is_none()
        );
        // Query the published receipt directly; unfinished-owner lookup must exclude it.
        // 直接查询已发布回执；未完成所有者查询必须排除它。
        let mut published: Vec<InitialAllocation> = recovered_store.query_rows(
        "SELECT project_id, allocation_id, target_json, namespace_id, content_table, path_table, state, content_created, path_created FROM vce_initial_allocations WHERE namespace_id = ?;",
        vec![sqlite_text(&config.binding.namespace_id)],
    ).await.expect("read published initial receipt");
        assert_eq!(published.len(), 1);
        // Exact publication includes both durable table acknowledgements and a fresh retry identity.
        // 精确发布包含两个持久表确认及新的重试身份。
        let published = published.pop().expect("one published receipt");
        assert_eq!(published.state, "published");
        assert_eq!((published.content_created, published.path_created), (1, 1));
        assert_ne!(published.allocation_id, failed.allocation_id);
        assert!(
            recovered_store
                .fail_initial_allocation(&published, String::from("fixture: late failure"))
                .await
                .contains("acknowledged initial table cleanup failed")
        );
        for table in &tables {
            assert!(
                recovered_store
                    .ensure_vector_table(table, config.dimension)
                    .await
                    .is_err(),
                "published table must survive a late failure notification"
            );
        }
        assert_eq!(
            recovered_store
                .load_active_project_config(&config.project_id)
                .await
                .expect("read protected published binding"),
            Some(config)
        );
        drop(index);
        retry_factory
            .shutdown()
            .await
            .expect("close initial recovery factory");
    })
}
