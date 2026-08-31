# 04：知识库运行时测试与夹具迁移映射

## 第10阶段保全

13项原测试仍全部可发现：11项随职责迁移，2项原模块路径保留于runtime/tests.rs；3个归档夹具唯一共享。全部测试体、正反断言、属性与原ignore理由严格等价，没有删除、复制、改弱或跳过新的测试。

普通定向回归12通过、1原有忽略。原真实控制器E2E已保留ignore声明并使用--ignored --exact单独执行通过：第二阶段前10.93秒、第二阶段后11.18秒、第三阶段后10.74秒、第四阶段后11.47秒、第五阶段后10.84秒、第六阶段后10.93秒、第七阶段后11.10秒、第八阶段后11.03秒、第九阶段后10.87秒、最终测试归集后10.91秒。使用独立临时库与随机本地端口，只在子进程清空可选性能语料环境变量；这是原用例的额外验证，不是新增测试或生产语料性能基准。

## 全量测试路径

| 原路径 | 当前路径 | 当前产物行 |
| --- | --- | --- |
| `knowledge_base::runtime::tests::atomic_file_replacement_overwrites_existing_destination` | `knowledge_base::runtime::storage::tests::atomic_file_replacement_overwrites_existing_destination` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage/tests.rs:19` |
| `knowledge_base::runtime::tests::bounded_package_entry_rejects_expansion_over_limit` | `knowledge_base::runtime::package_archive::tests::bounded_package_entry_rejects_expansion_over_limit` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive/tests.rs:211` |
| `knowledge_base::runtime::tests::cancellation_requires_exact_active_job_identity` | `knowledge_base::runtime::operations::tests::cancellation_requires_exact_active_job_identity` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations/tests.rs:9` |
| `knowledge_base::runtime::tests::interrupted_derived_index_statuses_become_actionable` | `knowledge_base::runtime::tests::interrupted_derived_index_statuses_become_actionable` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/tests.rs:31` |
| `knowledge_base::runtime::tests::library_id_requires_canonical_uuid` | `knowledge_base::runtime::storage::tests::library_id_requires_canonical_uuid` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage/tests.rs:10` |
| `knowledge_base::runtime::tests::markdown_scan_honors_preexisting_cancellation` | `knowledge_base::runtime::import::tests::markdown_scan_honors_preexisting_cancellation` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import/tests.rs:34` |
| `knowledge_base::runtime::tests::package_inspection_temp_name_requires_reserved_prefix_and_suffix` | `knowledge_base::runtime::package_lifecycle::tests::package_inspection_temp_name_requires_reserved_prefix_and_suffix` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle/tests.rs:10` |
| `knowledge_base::runtime::tests::package_preparation_enforces_exact_entries_and_digest` | `knowledge_base::runtime::package_archive::tests::package_preparation_enforces_exact_entries_and_digest` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive/tests.rs:125` |
| `knowledge_base::runtime::tests::portable_archive_name_is_safe_and_bounded` | `knowledge_base::runtime::package_archive::tests::portable_archive_name_is_safe_and_bounded` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive/tests.rs:221` |
| `knowledge_base::runtime::tests::real_controller_markdown_bm25_end_to_end` | `knowledge_base::runtime::tests::real_controller_markdown_bm25_end_to_end` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/tests.rs:58` |
| `knowledge_base::runtime::tests::replacement_journal_rejects_inconsistent_identity_or_phase` | `knowledge_base::runtime::package_lifecycle::tests::replacement_journal_rejects_inconsistent_identity_or_phase` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle/tests.rs:25` |
| `knowledge_base::runtime::tests::slash_path_rejects_parent_components` | `knowledge_base::runtime::import::tests::slash_path_rejects_parent_components` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import/tests.rs:10` |
| `knowledge_base::runtime::tests::warnings_are_bounded` | `knowledge_base::runtime::import::tests::warnings_are_bounded` | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import/tests.rs:18` |

## 共享夹具

| 原夹具 | 原始行 | 当前产物行 | 核查 |
| --- | ---: | --- | --- |
| `write_package_fixture` | 9216 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive/tests.rs:10` | 唯一且严格等价 |
| `package_fixture_metadata` | 9256 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive/tests.rs:50` | 唯一且严格等价 |
| `write_identity_mismatch_package` | 9284 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive/tests.rs:78` | 唯一且严格等价 |

## 匹配与边界

- package_archive三项回归保持精确元数据/摘要与非法条目拒绝、JSON膨胀上限、安全导出名；三夹具同属归档格式，身份伪造夹具仍由原跨域E2E显式引用。
- package_lifecycle两项回归保持保留解压前缀/后缀识别与日志身份/备份名/阶段拒绝。真实控制器E2E同时验证导出前FTS完整性、SQLite内身份、覆盖后项目绑定、prepared回滚和committed清理，继续保留为跨域整体用例，不错误归属为仅包测试。
- operations一项回归保持过期job不能取消新任务的精确原子信号断言；实际控制器用例复验普通导入进度和分离重建持久轮询，不冒充外部模型显示/向量调用验收。
- storage两项回归保留规范UUID拒绝与既有目标原子替换断言；import三项回归保留取消扫描、相对路径与告警限制；联合显示/向量恢复及E2E两项在runtime/tests.rs保留原模块路径，没有拆散联合断言。
- 显示/向量生产方法没有专属原始单元用例；通过全量词法/作用域保全、包级与消费者编译及现有服务回归核验迁移。原控制器E2E没有运行在线整理、Embedding或实际LanceDB代次生成，不冒充这些链路验收。
- 第六/七阶段没有专属原始单元测试迁移；跨域控制器E2E继续覆盖创建、项目授权保全、原始浏览、BM25/substring、索引拒绝及包恢复。测试发现仍13项，不把新模块数换算成测试覆盖。
- 最终原13测试全部位于6个测试文件，根无内联测试；900行E2E原为914行，差异仅缩进和rustfmt折行，完整token/断言等价，原ignore理由不变。该长用例是同一控制器/隔离数据生命周期下连续依赖的整体验证，不机械拆碎。
- 更正“排队外部派生快照”为实际running快照、共享控制器包装并非新缓存等说明，不改测试行为。控制器恢复用例是模拟崩溃产物并调用恢复，不是实际强杀进程测试。
