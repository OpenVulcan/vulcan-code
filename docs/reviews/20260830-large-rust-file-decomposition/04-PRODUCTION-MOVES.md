# 04：知识库运行时生产节点迁移映射

## 第10阶段检查点

原10521行，原始SHA-256为dc2111abb5b1017e1739c3e63cfa303501cc79b57509c746f60a363d12cd22ea。当前根295行，19个Rust产物合计12992行；独立模块接线、明确导入和双语说明计入物理行数。

279核验节点严格等价：263生产区域节点、13测试、3夹具；34顶层类型及3个方法内局部类型完整。局部类型也随所属完整方法体核验，不作为独立可迁移业务。累计251生产节点迁移，11项测试归位，21顶层常量完整；本检查点的逐文件结论见04-FINAL-REVIEW.md；全局审核独立计数。

## 全量生产映射

| 原节点 | 原始行 | 当前产物 | 核查 | 原声明→当前声明 |
| --- | ---: | --- | --- | --- |
| `struct:KnowledgeBaseRuntime` | 113 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:65` | 严格等价 | `pub`→`pub` |
| `struct:KnowledgeBaseRuntimeSharedState` | 140 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:92` | 严格等价 | `pub(crate)`→`pub(crate)` |
| `struct:ActiveKnowledgeOperation` | 164 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:31` | 严格等价 | `private`→`pub(super)` |
| `struct:PreparedKnowledgeOperation` | 176 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:43` | 严格等价 | `private`→`pub(super)` |
| `signal_matching_operation_cancellation` | 196 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:114` | 严格等价 | `private`→`private` |
| `struct:KnowledgeManifest` | 213 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:20` | 严格等价 | `private`→`pub(super)` |
| `struct:PreparedKnowledgePackage` | 227 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:46` | 严格等价 | `private`→`pub(super)` |
| `struct:KnowledgeReplacementJournal` | 249 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:47` | 严格等价 | `private`→`pub(super)` |
| `struct:KnowledgeReplacementJournalGuard` | 269 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:67` | 严格等价 | `private`→`private` |
| `KnowledgeReplacementJournalGuard::new` | 281 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:79` | 严格等价 | `private`→`private` |
| `KnowledgeReplacementJournalGuard::retain` | 290 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:88` | 严格等价 | `private`→`private` |
| `KnowledgeReplacementJournalGuard::remove_now` | 296 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:94` | 严格等价 | `private`→`private` |
| `Drop for KnowledgeReplacementJournalGuard::drop` | 310 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:108` | 严格等价 | `private`→`private` |
| `PreparedKnowledgePackage::database_path` | 320 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:67` | 严格等价 | `private`→`pub(super)` |
| `PreparedKnowledgePackage::display_database_path` | 326 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:73` | 严格等价 | `private`→`pub(super)` |
| `PreparedKnowledgePackage::vector_root` | 332 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:79` | 严格等价 | `private`→`pub(super)` |
| `struct:LibraryRow` | 340 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:20` | 严格等价 | `private`→`pub(super)` |
| `struct:ProjectBindingRow` | 379 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/authorization.rs:12` | 严格等价 | `private`→`private` |
| `struct:SourceRow` | 394 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:48` | 严格等价 | `private`→`private` |
| `struct:SourceSummaryRow` | 403 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:29` | 严格等价 | `private`→`private` |
| `struct:ExistingDocumentRow` | 430 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:57` | 严格等价 | `private`→`private` |
| `struct:ExistingChunkRow` | 445 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:72` | 严格等价 | `private`→`private` |
| `struct:JoinedChunkRow` | 457 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:56` | 严格等价 | `private`→`pub(super)` |
| `struct:FtsRankRow` | 493 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/retrieval.rs:19` | 严格等价 | `private`→`private` |
| `struct:DocumentSummaryRow` | 505 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:92` | 严格等价 | `private`→`private` |
| `struct:DocumentFolderRow` | 538 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:125` | 严格等价 | `private`→`private` |
| `struct:DocumentContentRow` | 547 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:134` | 严格等价 | `private`→`private` |
| `struct:IndexGateRow` | 583 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:28` | 严格等价 | `private`→`private` |
| `struct:ImportJobRow` | 607 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:64` | 严格等价 | `private`→`private` |
| `struct:IndexTablePresenceRow` | 658 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:52` | 严格等价 | `private`→`private` |
| `struct:IndexIntegrityCountsRow` | 667 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:61` | 严格等价 | `private`→`private` |
| `struct:CountRow` | 685 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:13` | 严格等价 | `private`→`pub(super)` |
| `struct:DisplayCountRow` | 697 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:22` | 严格等价 | `private`→`private` |
| `struct:DisplaySourceHashRow` | 706 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:31` | 严格等价 | `private`→`private` |
| `struct:DisplayMetadataRow` | 718 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:43` | 严格等价 | `private`→`private` |
| `struct:DisplayDocumentRow` | 730 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:55` | 严格等价 | `private`→`private` |
| `DisplayDocumentRow::into_display_document` | 775 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:103` | 严格等价 | `private`→`private` |
| `struct:DisplaySearchRow` | 801 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:129` | 严格等价 | `private`→`private` |
| `struct:DisplayChunkRow` | 834 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:162` | 严格等价 | `private`→`private` |
| `DisplayChunkRow::into_display_chunk` | 867 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:198` | 严格等价 | `private`→`private` |
| `struct:ChunkCountRow` | 885 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:79` | 严格等价 | `private`→`private` |
| `struct:ScannedMarkdownFile` | 894 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:84` | 严格等价 | `private`→`private` |
| `struct:ScanProgressSnapshot` | 920 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:108` | 严格等价 | `private`→`private` |
| `enum:ImportIndexSyncOutcome` | 932 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:88` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::new` | 944 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:120` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::recover_interrupted_jobs` | 961 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:141` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::recover_interrupted_derived_index_states` | 983 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:165` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::recover_interrupted_replacements` | 1029 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:128` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::cleanup_orphan_package_directories` | 1168 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:277` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::cancel_operation` | 1212 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:173` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::cancel_import` | 1229 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:190` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::latest_import_job` | 1235 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:206` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::start_external_operation` | 1285 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:256` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::update_external_operation_progress` | 1357 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:328` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::update_operation_observability` | 1384 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:355` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::operation_is_active` | 1413 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:384` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::operation_is_active_for_library` | 1419 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:400` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::operation_is_cancelled` | 1431 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:412` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::finish_external_operation` | 1446 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:427` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::fail_external_operation` | 1463 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:444` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::finish_cancelled_external_operation` | 1475 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:456` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::create_library` | 1483 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:73` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::create_library_with_settings` | 1548 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:154` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::set_library_settings` | 1632 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:250` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::rename_library` | 1784 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:416` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::export_package` | 1840 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:327` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::inspect_package` | 1957 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:454` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::import_package_as_new` | 2002 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:509` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::replace_library_from_package` | 2064 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:581` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::list_libraries` | 2308 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:482` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::get_library` | 2337 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:521` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::set_project_binding` | 2355 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/authorization.rs:43` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::list_project_bindings` | 2404 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/authorization.rs:102` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::list_bindings_for_project` | 2427 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/authorization.rs:135` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::authorized_library_ids` | 2453 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/authorization.rs:173` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::list_custom_words` | 2473 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:110` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::upsert_custom_word` | 2500 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:151` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::remove_custom_word` | 2521 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:184` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::import_markdown_directory` | 2537 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:132` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::import_markdown_directory_for_source` | 2554 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:157` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::start_import_markdown_directory_for_source` | 2585 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:196` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::execute_prepared_import` | 2632 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:251` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::list_sources` | 2689 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:180` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::list_documents` | 2716 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:223` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::list_document_folders` | 2756 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:273` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::read_document_summary` | 2779 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:308` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::read_document` | 2808 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:349` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::read_chunk` | 2850 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:403` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::ensure_display_storage` | 2865 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:226` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::prepare_display_generation` | 2883 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:256` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::resume_display_generation` | 2927 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:312` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::begin_display_generation` | 2951 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:350` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::list_display_generation_source_hashes` | 2994 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:407` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::discard_display_generation_source` | 3026 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:455` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::write_display_document` | 3074 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:519` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::activate_display_generation` | 3177 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:638` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::mark_display_generation_ready` | 3243 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:722` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::set_display_index_status` | 3300 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:793` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::search_display` | 3333 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:840` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::search_display_hybrid_with_vector` | 3430 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:953` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::read_display_document` | 3542 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:1077` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::read_display_document_content` | 3587 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:1134` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::list_display_chunks` | 3648 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:1209` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::count_display_chunks` | 3688 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:1259` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::ensure_vector_storage` | 3722 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:30` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::prepare_vector_generation` | 3730 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:52` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::write_vector_rows` | 3809 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:149` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::search_vector_rows` | 3861 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:219` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::mark_vector_generation_ready` | 3896 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:270` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::set_vector_index_status` | 3957 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:345` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::search` | 3991 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/retrieval.rs:45` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::search_with_mode` | 4002 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/retrieval.rs:72` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::rebuild_index` | 4075 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:210` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::start_rebuild_index` | 4091 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:232` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::repair_index` | 4105 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:252` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::start_repair_index` | 4121 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:274` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::start_index_operation` | 4135 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:296` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::execute_prepared_index_operation` | 4174 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:341` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::delete_library` | 4291 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:549` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::import_markdown_directory_inner` | 4362 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:329` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::initialize_library_storage` | 4657 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:47` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::resolve_source_id` | 4690 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:640` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::persist_source` | 4755 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:722` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::clear_staging` | 4794 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:773` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::stage_import_rows` | 4810 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:807` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::commit_staging` | 4859 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:868` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::sync_changed_fts` | 4918 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:479` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::rebuild_index_for_import` | 4977 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:556` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::replay_staged_fts_for_import` | 5034 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:631` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::replay_authoritative_fts_for_import` | 5067 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:682` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::replay_staged_fts_batch` | 5100 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:731` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::replay_authoritative_fts_batch` | 5134 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:781` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::repair_index_inner` | 5167 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:832` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::replay_missing_fts_batch` | 5276 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:957` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::rebuild_index_inner` | 5314 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1013` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::search_one_library` | 5387 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/retrieval.rs:161` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::load_substring_chunks` | 5536 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:434` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::inspect_index_integrity` | 5574 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1096` | 严格等价 | `pub`→`pub` |
| `KnowledgeBaseRuntime::inspect_index_integrity_for_binding` | 5588 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1124` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::load_index_gate` | 5698 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1246` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::verify_index_gate` | 5727 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1287` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::verify_index_integrity` | 5763 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1327` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::verify_index_rows_exact` | 5770 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1350` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::load_index_integrity_counts` | 5799 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1391` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::mark_index_ready` | 5850 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1454` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::mark_dictionary_dirty` | 5870 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1488` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::load_joined_chunk` | 5891 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:482` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::load_joined_chunks_by_ids` | 5916 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:521` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::authoritative_counts` | 5948 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:565` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::staged_chunk_count` | 5972 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1521` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::project_registry_authoritative_counts` | 6002 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:732` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::project_registry_counts` | 6030 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:776` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::set_library_index_state` | 6059 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1563` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::set_registry_index_status` | 6082 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1598` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::mark_derived_indexes_stale` | 6098 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:629` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::find_library_by_normalized_name` | 6159 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:835` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::install_prepared_package_database` | 6186 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:872` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::retarget_staged_package_database` | 6272 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:968` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::cleanup_failed_package_install` | 6340 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1046` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::insert_imported_library_registry` | 6348 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1064` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::update_replaced_library_registry` | 6391 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1117` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::rollback_replaced_library` | 6434 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1170` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::restore_library_registry` | 6464 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1200` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::require_active_library` | 6513 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:700` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::ensure_library_binding` | 6529 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:88` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::ensure_display_binding` | 6538 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:110` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::activate_library_binding` | 6547 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:134` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::release_library_binding` | 6569 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:166` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::release_display_binding` | 6588 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:195` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::release_vector_binding` | 6610 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:227` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::activate_vector_binding` | 6632 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:261` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::enable_vector_binding` | 6659 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:300` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::activate_display_binding` | 6724 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:377` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::enable_display_binding` | 6751 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:416` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::validate_display_bound_metadata` | 6803 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:482` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::enable_library_binding` | 6843 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:534` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::validate_bound_library_metadata` | 6883 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:588` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::verify_sqlite_quick_check` | 6926 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:643` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::validate_library_artifacts` | 6967 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:700` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::library_binding_identity` | 7033 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:776` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::display_binding_identity` | 7046 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:802` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::vector_binding_identity` | 7059 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:825` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::knowledge_root` | 7072 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:851` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::library_root` | 7078 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:867` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::remove_host_owned_child_directory_if_exists` | 7104 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:893` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::replacement_operation_artifacts` | 7167 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1261` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::write_replacement_journal_atomic` | 7198 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1292` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::write_manifest_atomic` | 7250 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:954` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::library_mutation_guard` | 7277 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:215` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::query_app_rows` | 7294 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:993` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::query_library_rows` | 7315 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:1030` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::execute_library_batch` | 7333 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:1066` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::prepare_operation` | 7357 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:470` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::insert_operation_job` | 7408 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:521` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::release_active_operation` | 7445 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:558` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::active_operation_for_job` | 7459 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:582` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::release_operation_by_job` | 7467 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:600` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::scan_markdown_files_with_progress` | 7480 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:940` | 严格等价 | `private`→`private` |
| `KnowledgeBaseRuntime::update_import_scan_progress` | 7530 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:623` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::prepare_import_processing` | 7551 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:654` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::update_operation_phase` | 7571 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:684` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::update_import_progress` | 7598 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:721` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::update_import_index_progress` | 7629 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:762` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::finish_import_job` | 7656 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:799` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::finish_index_operation_job` | 7699 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:842` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::finish_failed_operation_job` | 7724 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:877` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::finish_cancelled_operation_job` | 7741 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:908` | 严格等价 | `private`→`pub(super)` |
| `queued_operation_summary` | 7762 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:134` | 严格等价 | `private`→`pub(super)` |
| `persist_tempfile_atomically` | 7796 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:1097` | 严格等价 | `private`→`pub(super)` |
| `scan_markdown_files` | 7808 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:1005` | 严格等价 | `private`→`private` |
| `publish_scan_progress` | 7914 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:1125` | 严格等价 | `private`→`private` |
| `import_progress_path` | 7929 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:1152` | 严格等价 | `private`→`private` |
| `require_operation_not_cancelled` | 7940 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:158` | 严格等价 | `private`→`pub(super)` |
| `metadata_is_link_or_reparse` | 7950 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:1119` | 严格等价 | `private`→`pub(super)` |
| `canonical_import_root` | 7968 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:1173` | 严格等价 | `private`→`private` |
| `slash_path` | 7979 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:1194` | 严格等价 | `private`→`private` |
| `validate_library_name` | 7994 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:816` | 严格等价 | `private`→`pub(super)` |
| `validate_custom_word` | 8007 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1625` | 严格等价 | `private`→`private` |
| `validate_library_id` | 8020 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:1147` | 严格等价 | `private`→`pub(super)` |
| `validate_generation_id` | 8031 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:237` | 严格等价 | `private`→`private` |
| `display_fts_index_name` | 8042 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:1304` | 严格等价 | `private`→`private` |
| `validate_display_fts_index` | 8053 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:1325` | 严格等价 | `private`→`private` |
| `vector_table_name` | 8066 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:390` | 严格等价 | `private`→`pub(super)` |
| `validate_vector_table_name` | 8073 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:407` | 严格等价 | `private`→`private` |
| `is_package_inspection_temp_directory_name` | 8096 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1355` | 严格等价 | `private`→`private` |
| `validate_replacement_journal` | 8113 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1372` | 严格等价 | `private`→`private` |
| `library_name_key` | 8148 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:839` | 严格等价 | `private`→`pub(super)` |
| `checked_package_count` | 8154 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:1407` | 严格等价 | `private`→`private` |
| `canonical_existing_directory` | 8160 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:96` | 严格等价 | `private`→`pub(super)` |
| `canonical_knowledge_package_path` | 8172 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:118` | 严格等价 | `private`→`pub(super)` |
| `prepare_knowledge_package` | 8197 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:153` | 严格等价 | `private`→`pub(super)` |
| `extract_bounded_zip_file` | 8352 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:318` | 严格等价 | `private`→`private` |
| `safe_vector_archive_relative_path` | 8380 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:356` | 严格等价 | `private`→`private` |
| `read_bounded_zip_entry` | 8400 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:386` | 严格等价 | `private`→`private` |
| `validate_knowledge_package_metadata` | 8418 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:414` | 严格等价 | `private`→`private` |
| `export_knowledge_package_files` | 8557 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:563` | 严格等价 | `private`→`pub(super)` |
| `collect_export_artifact_files` | 8737 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:753` | 严格等价 | `private`→`private` |
| `copy_prepared_derived_artifacts` | 8799 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:825` | 严格等价 | `private`→`pub(super)` |
| `copy_ordinary_directory_recursively` | 8821 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:857` | 严格等价 | `private`→`private` |
| `collect_ordinary_files_recursively` | 8865 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:911` | 严格等价 | `private`→`private` |
| `relative_to_zip_path` | 8891 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:947` | 严格等价 | `private`→`private` |
| `sha256_file_hex` | 8920 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:986` | 严格等价 | `private`→`private` |
| `portable_knowledge_archive_name` | 8953 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:1019` | 严格等价 | `private`→`private` |
| `path_key` | 8974 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:1219` | 严格等价 | `private`→`private` |
| `decode_rows` | 8988 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:1168` | 严格等价 | `private`→`private` |
| `library_summary_from_row` | 8994 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs:855` | 严格等价 | `private`→`pub(super)` |
| `document_summary_from_row` | 9020 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:596` | 严格等价 | `private`→`private` |
| `source_summary_from_row` | 9040 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:622` | 严格等价 | `private`→`private` |
| `chunk_from_joined_row` | 9054 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:646` | 严格等价 | `private`→`private` |
| `nonnegative_u64` | 9069 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:252` | 严格等价 | `private`→`private` |
| `parse_revision` | 9079 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:1650` | 严格等价 | `private`→`private` |
| `text` | 9087 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:260` | 严格等价 | `private`→`private` |
| `int` | 9093 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:268` | 严格等价 | `private`→`private` |
| `bool_value` | 9099 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/authorization.rs:204` | 严格等价 | `private`→`private` |
| `unix_epoch_millis` | 9105 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:276` | 严格等价 | `private`→`private` |
| `system_time_millis` | 9115 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:1243` | 严格等价 | `private`→`private` |
| `push_warning` | 9124 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:1264` | 严格等价 | `private`→`private` |
| `truncate_chars` | 9132 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:290` | 严格等价 | `private`→`private` |
| `recovered_display_index_status` | 9142 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs:1350` | 严格等价 | `private`→`pub(super)` |
| `recovered_vector_index_status` | 9162 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs:432` | 严格等价 | `private`→`pub(super)` |
| `KnowledgeBaseRuntime::validate_display_bound_metadata::struct:DisplayBoundMetadataRow` | 6810 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:489` | 严格等价；随所属方法保留 | `private`→`private` |
| `KnowledgeBaseRuntime::validate_bound_library_metadata::struct:BoundMetadataRow` | 6890 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:595` | 严格等价；随所属方法保留 | `private`→`private` |
| `KnowledgeBaseRuntime::verify_sqlite_quick_check::struct:QuickCheckRow` | 6934 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs:651` | 严格等价；随所属方法保留 | `private`→`private` |

## 精确可见性与接口

- 76项节点声明、25字段及4共享常量范围单独逐token核验；其他可见性、表达式、字面量和测试属性保持。
- 71个原可见类型/方法声明不变，其中67个固有方法迁入子模块，但仍属于原KnowledgeBaseRuntime，消费者调用路径不变。两个原运行状态类型仍在根，所有包装共享同一ControllerRuntimeHandle持有的锁/缓存/活动操作。
- PreparedKnowledgePackage及路径方法、归档入口只向runtime开放；package/source_manifest两字段供安装读取，其他资源字段私有。身份不匹配夹具仅在cfg(test)下以pub(in super::super)向原E2E共享，不扩大生产接口。
- 日志类型与五字段及精确产物方法为原E2E共享；守卫/Drop与其他事务助手私有。包公开方法保留原pub，两个启动恢复方法仅pub(super)。日志三常量供原E2E通过测试专用导入读取，仍限runtime。
- ActiveKnowledgeOperation和PreparedKnowledgeOperation类型供根共享状态/执行器引用，只有Prepared五字段开放到runtime；Active字段及ImportJobRow字段保持操作模块私有。仅实际跨领域进度/终态/预留方法与排队/取消纯助手开放到父域。
- KnowledgeManifest及三个字段只供原库重命名、归档和包事务访问；初始化/绑定/目录/查询入口按真实调用开放到runtime，底层解码、私有enable和派生释放助手保持私有。共享状态与变更锁没有复制。
- 显示六个行类型/字段和两个转换保持私有；两个代次恢复函数供根联合恢复使用，vector_table_name供显示混合检索使用，其余专属助手私有。22个显示/向量公开方法只迁移实现位置。
- LibraryRow及库名/转换助手为包事务复用，仅向runtime开放且行字段仍私有；授权行与布尔转换完全私有。注册配置归并、删除撤权与项目默认筛选没有改写。
- raw_corpus的JoinedChunkRow及检索所用8字段、bulk/substring查询和显示所用无正文摘要仅向runtime开放，其他字段/类型/助手私有。检索路由仍按当前库策略选择原始或显示内容，不承担服务层在线Embedding。
- FTS维护行/计数类型及内部回放均私有，只有导入同步/重建、检索门禁、包完整性及取消结果向runtime开放。CountRow及权威计数查询归raw_corpus，两个字段供真实读取/投影消费；区分成功导入与不推进导入时间的注册投影归library。
- import的5类型、11方法与8助手完整内聚，除3个原公开导入方法外均保持私有；扫描/告警测试随职责迁移。根最终只保留两个共享类型、四编排/锁方法、六共享助手与五共享常量，不制造微型通用文件。
- 原models别名仍指向唯一knowledge_base::models，不引入候选模块或协议。没有放宽通用token比较器；取消错误常量的自动折行按完整声明读取后严格比较，值未改变。

| 类型 | 明确开放到runtime的原字段 |
| --- | --- |
| `struct:PreparedKnowledgePackage` | `package`、`source_manifest` |
| `struct:KnowledgeReplacementJournal` | `version`、`target_library_id`、`backup_directory_name`、`phase`、`previous` |
| `struct:PreparedKnowledgeOperation` | `job_id`、`library_id`、`operation_kind`、`cancellation`、`created_at_ms` |
| `struct:KnowledgeManifest` | `library_id`、`name`、`schema_version` |
| `struct:JoinedChunkRow` | `id`、`document_id`、`heading_path`、`start_line`、`end_line`、`content`、`title`、`relative_path` |
| `struct:CountRow` | `document_count`、`chunk_count` |

## 常量完整性

| 常量 | 原始行 | 当前产物 | 声明与值 |
| --- | ---: | --- | --- |
| `MAX_IMPORT_FILES` | 47 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:31` | 唯一且值相同；声明不变 |
| `MAX_MARKDOWN_FILE_BYTES` | 50 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:35` | 唯一且值相同；声明不变 |
| `MAX_IMPORT_BYTES` | 53 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:39` | 唯一且值相同；声明不变 |
| `FTS_MUTATION_BATCH_SIZE` | 56 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:19` | 唯一且值相同；声明不变 |
| `FTS_DELETE_PARAMETER_BATCH_SIZE` | 59 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs:23` | 唯一且值相同；声明不变 |
| `MAX_IMPORT_WARNINGS` | 62 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs:43` | 唯一且值相同；声明不变 |
| `MAX_WARNING_CHARS` | 65 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:47` | 唯一且值相同；声明不变 |
| `MAX_DOCUMENT_PAGE_SIZE` | 68 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs:24` | 唯一且值相同；声明不变 |
| `MAX_SEARCH_LIMIT` | 71 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:50` | 唯一且值相同；声明不变 |
| `RRF_K` | 74 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:53` | 唯一且值相同；声明不变 |
| `KNOWLEDGE_OPERATION_CANCELLED_ERROR` | 77 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs:25` | 唯一且值相同；private→pub(super)，仍限runtime |
| `KNOWLEDGE_PACKAGE_METADATA_ENTRY` | 80 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:26` | 唯一且值相同；声明不变 |
| `KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY` | 83 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:30` | 唯一且值相同；声明不变 |
| `KNOWLEDGE_PACKAGE_DATABASE_ENTRY` | 86 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:34` | 唯一且值相同；声明不变 |
| `KNOWLEDGE_PACKAGE_DISPLAY_DATABASE_ENTRY` | 89 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:38` | 唯一且值相同；声明不变 |
| `KNOWLEDGE_PACKAGE_VECTOR_PREFIX` | 92 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs:42` | 唯一且值相同；声明不变 |
| `KNOWLEDGE_PACKAGE_INSPECTION_TEMP_PREFIX` | 95 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:56` | 唯一且值相同；声明不变 |
| `KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION` | 99 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:33` | 唯一且值相同；private→pub(super)，仍限runtime |
| `KNOWLEDGE_REPLACEMENT_PHASE_PREPARED` | 102 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:37` | 唯一且值相同；private→pub(super)，仍限runtime |
| `KNOWLEDGE_REPLACEMENT_PHASE_COMMITTED` | 105 | `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs:41` | 唯一且值相同；private→pub(super)，仍限runtime |
| `MAX_KNOWLEDGE_PACKAGE_JSON_BYTES` | 108 | `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs:60` | 唯一且值相同；声明不变 |

## 当前产物摘要

| Rust产物 | 行数 | SHA-256 |
| --- | ---: | --- |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime.rs` | 295 | `3ce4985875a6111e846898deceabb9010fa5170a871eb51a3cde08b286b5a8a2` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/vector.rs` | 444 | `f3c07d621312bf17fe162d4a313e930c412c87430d456ae677ac3a7d3112a150` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/tests.rs` | 957 | `b3305bbe76add3e050387c7dde81377dc5907dfe36efd4d6a6e2c0408834c2e7` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage.rs` | 1173 | `0ce909409a66c08d8cb864e2044235fb2d05ceacc94b5c0ec907bb31795d9946` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/storage/tests.rs` | 35 | `7ed9b7d322c2124b3c9f1798216c24adf793b817757eacf9793d80e47dd67ef1` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/retrieval.rs` | 307 | `11d43ef4cd966c4c387e16af82b1dee811d9a9d6fafe8efdd782b65c82cdd905` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/raw_corpus.rs` | 657 | `3902bc8d2ddf4166713bd8cd2ecb68797bf6fb7a2e31790bd70388420a152367` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle.rs` | 1412 | `99043b976b3a1c510be5bba798b54bdb3fe097b0bf1aca1e1d04f34a240beab9` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_lifecycle/tests.rs` | 70 | `aa6bfdb14996f7ed882fae72f92b9e06a7c1c32dd4ac28950d844c87dc65251c` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive.rs` | 1039 | `78ea482588962b8ca86ef87cc353fa1ffb4bca8342dbf3e879fbdea3f1e001c7` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/package_archive/tests.rs` | 240 | `459480b7d6c6d3bbc3e67f860e3c904e35ebf37fece8495e4bf9700accc3113f` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations.rs` | 924 | `36b23458e5c473d5e49353783c29cc832b23bf6f9f5c89bf4b3f7745dfff3c21` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/operations/tests.rs` | 25 | `2f4c55644b49d4387c51bc0c8991ff6ca53def465395ee50b99108bd49de9760` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/library.rs` | 877 | `9626a3f28adbc37a9c0d08fc700a76e17862d987ce0ad7a60d85a87f0295ad23` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/import.rs` | 1271 | `b98910fa79470f239cd0abc9d61b85e83b5c42842a11f4dbcbf5e2d9faed762c` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/display.rs` | 1362 | `aa66efc07a3bfb93140c5b14d3ff0db1f653a42387bd2e6b55979586d5432ea8` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/authorization.rs` | 206 | `fc0164a657ae87f20cab5e0856a18d44793d0114fdda6d9ffbc8830982cc6658` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/fts.rs` | 1654 | `d7efacbe3284db6e07e7670e8189cbc5c64353904a137128c9347717fdc90231` |
| `crates/vulcan-workspace-store/src/knowledge_base/runtime/import/tests.rs` | 44 | `b155f3c57dc43f3323dfcf8646fec8c3a47b812e0cd4d2d89d1e24f89472f1f4` |
