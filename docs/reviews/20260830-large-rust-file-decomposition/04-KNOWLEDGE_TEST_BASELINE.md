# 04：知识库运行时原始测试基线

原始源码SHA-256：dc2111abb5b1017e1739c3e63cfa303501cc79b57509c746f60a363d12cd22ea。

279个核验节点包括3个方法内局部类型；13项测试、3个夹具、21个顶层常量。方法内常量/类型同时随完整函数体核对。

拆前命令：rtk proxy cargo test -p vulcan-workspace-store --lib knowledge_base::runtime:: -- --test-threads=1 --quiet。
结果：12通过、0失败、1原有忽略；真实控制器用例的ignore理由与原属性保留，本轮尚未手动执行。

| 原测试路径 | 原始行 | 属性 |
| --- | ---: | --- |
| `knowledge_base::runtime::tests::atomic_file_replacement_overwrites_existing_destination` | 9450 | 原test属性 |
| `knowledge_base::runtime::tests::bounded_package_entry_rejects_expansion_over_limit` | 9557 | 原test属性 |
| `knowledge_base::runtime::tests::cancellation_requires_exact_active_job_identity` | 9405 | 原test属性 |
| `knowledge_base::runtime::tests::interrupted_derived_index_statuses_become_actionable` | 9191 | 原test属性 |
| `knowledge_base::runtime::tests::library_id_requires_canonical_uuid` | 9331 | 原test属性 |
| `knowledge_base::runtime::tests::markdown_scan_honors_preexisting_cancellation` | 9591 | 原test属性 |
| `knowledge_base::runtime::tests::package_inspection_temp_name_requires_reserved_prefix_and_suffix` | 9340 | 原test属性 |
| `knowledge_base::runtime::tests::package_preparation_enforces_exact_entries_and_digest` | 9471 | 原test属性 |
| `knowledge_base::runtime::tests::portable_archive_name_is_safe_and_bounded` | 9567 | 原test属性 |
| `knowledge_base::runtime::tests::real_controller_markdown_bm25_end_to_end` | 9607 | tokio current_thread；原ignore需要bundled controller |
| `knowledge_base::runtime::tests::replacement_journal_rejects_inconsistent_identity_or_phase` | 9355 | 原test属性 |
| `knowledge_base::runtime::tests::slash_path_rejects_parent_components` | 9426 | 原test属性 |
| `knowledge_base::runtime::tests::warnings_are_bounded` | 9434 | 原test属性 |

| 原夹具 | 原始行 |
| --- | ---: |
| `write_package_fixture` | 9216 |
| `package_fixture_metadata` | 9256 |
| `write_identity_mismatch_package` | 9284 |
