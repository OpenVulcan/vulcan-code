//! Real in-process VCE physical-compaction performance and equivalence fixture.
//! 真实进程内 VCE 物理整理性能与等价性夹具。

use super::{
    VceProjectIndex, VceProjectIndexConfig, bootstrap_vce_data_plane, lance_string_literal,
    lance_string_set, physical_compaction_options, vector_row_id,
};
use crate::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};
use uuid::Uuid;
use vulcan_context_engine::{
    EmbeddingSpace, VceIndexBinding, VcePhysicalCompactionState, VcePhysicalTableRole,
    VcePhysicalTableStatistics, blob_name,
};
use vulcan_lancedb_engine::types::{
    LanceDbInputFormat, LanceDbOutputFormat, LanceDbSearchInput, LanceDbUpsertInput,
};

/// Vector dimension from the diagnosed production-equivalent workload.
/// 来自已诊断生产等价工作负载的向量维度。
const PERFORMANCE_DIMENSION: usize = 1_024;

/// Current members retained by the scale fixture.
/// 规模夹具保留的当前成员数量。
const PERFORMANCE_MEMBER_COUNT: usize = 10_000;

/// Current members used by the original long-filter regression profile.
/// 原始长过滤回归配置使用的当前成员数量。
const REGRESSION_MEMBER_COUNT: usize = 3_139;

/// Visible members owned by the diagnosed second generation.
/// 诊断基线中由第二代持有的可见成员数量。
const SECOND_GENERATION_MEMBER_COUNT: usize = 27;

/// Replaced members whose first-generation rows remain physically present but invisible.
/// 第一代旧行仍物理保留但不可见的已替换成员数量。
const HIDDEN_HISTORICAL_ROW_COUNT: usize = 21;

/// Rows written per transport request, producing realistic bounded physical fragments.
/// 每次传输请求写入的行数，用于产生真实有界物理碎片。
const WRITE_BATCH_ROWS: usize = 250;

/// Result count used by every equivalent nearest-neighbour query.
/// 每次等价近邻查询使用的结果数量。
const QUERY_LIMIT: u32 = 64;

/// Warm single-query repetitions required by the acceptance plan.
/// 验收方案要求的预热单路查询重复次数。
const QUERY_REPETITIONS: usize = 20;

/// Three-way concurrent groups required by the original regression profile.
/// 原始回归配置要求的三路并发组数。
const CONCURRENT_QUERY_GROUPS: usize = 20;

/// One measured query profile with cold, warmed, and concurrent observations.
/// 一组包含冷启动、预热及并发观察的查询测量结果。
struct QueryMeasurements {
    /// First uncached request duration.
    /// 首次未预热请求耗时。
    cold: Duration,
    /// Warm sequential request durations.
    /// 预热后的顺序请求耗时。
    single: Vec<Duration>,
    /// Individual request durations collected from concurrent groups.
    /// 从并发组收集的单个请求耗时。
    concurrent_calls: Vec<Duration>,
    /// Wall-clock durations of complete three-way groups.
    /// 完整三路并发组的墙钟耗时。
    concurrent_groups: Vec<Duration>,
    /// Ordered stable row identities used for before/after equivalence.
    /// 用于整理前后等价比较的有序稳定行身份。
    signature: Vec<String>,
}

/// Build one 1024-dimensional project configuration under a canonical temporary root.
/// 在规范临时根目录下构建一个 1024 维项目配置。
///
/// `project_id` and `root` own the isolated namespace. Returns a writable production-shaped
/// binding with one exact tagged physical fingerprint.
/// `project_id` 与 `root` 持有隔离命名空间。返回带精确类型化物理指纹的生产形态可写绑定。
fn performance_project_config(project_id: &str, root: &Path) -> VceProjectIndexConfig {
    let canonical = std::fs::canonicalize(root).expect("canonicalize performance project root");
    let root_path = canonical.display().to_string();
    let root_hash = hex::encode(Sha256::digest(root_path.as_bytes()));
    let checkpoint_chain_id = format!("project:{project_id}:root:{root_hash}");
    let namespace_id = format!("vce:{checkpoint_chain_id}");
    let vector_space_id = format!("code-semantic:{project_id}:{root_hash}");
    let physical_fingerprint = format!(
        "sha256:{}",
        hex::encode(Sha256::digest(b"vce-native-compaction-performance"))
    );
    let preprocessing_version = String::from("vce-preprocess-v1");
    let chunker_version = String::from("vce-chunker-v1");
    let embedding_space = EmbeddingSpace {
        vector_space_id: vector_space_id.clone(),
        physical_fingerprint: Some(physical_fingerprint.clone()),
        dimension: PERFORMANCE_DIMENSION,
        preprocessing_version: preprocessing_version.clone(),
        chunker_version: chunker_version.clone(),
    };
    VceProjectIndexConfig {
        project_id: project_id.to_owned(),
        root_path,
        root_hash,
        binding: VceIndexBinding::new(namespace_id, checkpoint_chain_id, 1, embedding_space)
            .expect("build performance index binding"),
        vector_space_id,
        physical_fingerprint,
        preprocessing_version,
        chunker_version,
        dimension: PERFORMANCE_DIMENSION as u32,
        config_revision: 1,
    }
}

/// Build one deterministic 1024-dimensional vector with a unique distance from the query.
/// 构建一个与查询距离唯一的确定性 1024 维向量。
///
/// `ordinal` identifies the logical member. Returns finite values without provider execution.
/// `ordinal` 标识逻辑成员。返回不调用供应商即可得到的有限值。
fn performance_vector(ordinal: usize) -> Vec<f32> {
    let mut vector = vec![0.0; PERFORMANCE_DIMENSION];
    vector[0] = 1.0;
    vector[1] = ordinal as f32 / PERFORMANCE_MEMBER_COUNT as f32;
    vector
}

/// Build one VCE-shaped physical vector row for a table role and generation.
/// 为一个表角色与代际构建一条 VCE 形态的物理向量行。
///
/// `index` supplies immutable storage identity; `role`, `ordinal`, `blob`, `refresh_id`, and
/// `generation` select the exact historical occurrence. Returns one complete JSON row.
/// `index` 提供不可变存储身份；`role`、`ordinal`、`blob`、`refresh_id` 与 `generation`
/// 选择精确历史出现位置。返回一条完整 JSON 行。
fn performance_row(
    index: &VceProjectIndex,
    role: VcePhysicalTableRole,
    ordinal: usize,
    blob: &str,
    refresh_id: &str,
    generation: u64,
) -> serde_json::Value {
    let input_id = hex::encode(Sha256::digest(
        format!("{role:?}:input:{ordinal}").as_bytes(),
    ));
    let path = format!("src/performance_{ordinal}.rs");
    serde_json::json!({
        "row_id": vector_row_id(
            &index.binding.namespace_id,
            &index.physical_fingerprint,
            refresh_id,
            generation,
            &input_id,
        ),
        "id": input_id,
        "blob_name": blob,
        "namespace_id": index.binding.namespace_id,
        "refresh_id": refresh_id,
        "generation": generation,
        "vector_space_id": index.vector_space_id,
        "physical_fingerprint": index.physical_fingerprint,
        "path": path,
        "start_line": 1_u64,
        "end_line": 1_u64,
        "content_hash": hex::encode(Sha256::digest(format!("content:{ordinal}").as_bytes())),
        "written_at_unix_ms": 1_i64,
        "vector": performance_vector(ordinal),
    })
}

/// Write one bounded row batch to both physical VCE vector tables.
/// 将一个有界行批次写入两张物理 VCE 向量表。
///
/// `index`, `blobs`, and `ordinals` select current or historical records; `refresh_id` and
/// `generation` freeze their attempt identity. Returns after both real engine writes finish.
/// `index`、`blobs` 与 `ordinals` 选择当前或历史记录；`refresh_id` 与 `generation`
/// 冻结其尝试身份。两次真实引擎写入均完成后返回。
async fn write_performance_rows(
    index: &VceProjectIndex,
    blobs: &[String],
    ordinals: std::ops::Range<usize>,
    refresh_id: &str,
    generation: u64,
) {
    let content_rows = ordinals
        .clone()
        .map(|ordinal| {
            performance_row(
                index,
                VcePhysicalTableRole::Content,
                ordinal,
                &blobs[ordinal],
                refresh_id,
                generation,
            )
        })
        .collect::<Vec<_>>();
    let path_rows = ordinals
        .map(|ordinal| {
            performance_row(
                index,
                VcePhysicalTableRole::Path,
                ordinal,
                &blobs[ordinal],
                refresh_id,
                generation,
            )
        })
        .collect::<Vec<_>>();
    let content_data = serde_json::to_vec(&content_rows).expect("encode content performance rows");
    let path_data = serde_json::to_vec(&path_rows).expect("encode path performance rows");
    let engine = index
        .data_plane
        .vector_engine()
        .await
        .expect("open performance vector engine");
    engine
        .vector_upsert(LanceDbUpsertInput {
            table_name: index.content_table.clone(),
            input_format: LanceDbInputFormat::JsonRows,
            data: content_data,
            key_columns: vec![String::from("row_id")],
        })
        .await
        .expect("write content performance fragment");
    engine
        .vector_upsert(LanceDbUpsertInput {
            table_name: index.path_table.clone(),
            input_format: LanceDbInputFormat::JsonRows,
            data: path_data,
            key_columns: vec![String::from("row_id")],
        })
        .await
        .expect("write path performance fragment");
}

/// Populate one exact current-member scale plus hidden historical rows in bounded fragments.
/// 以有界碎片填充一个精确当前成员规模，并额外保留隐藏历史行。
///
/// `index` owns the physical tables and `member_count` selects the exact visible checkpoint scale.
/// Returns the canonical Blob identities in deterministic ordinal order.
/// `index` 持有物理表，`member_count` 选择精确可见检查点规模。返回按序号确定排序的规范 Blob 身份。
async fn populate_performance_tables(index: &VceProjectIndex, member_count: usize) -> Vec<String> {
    assert!(member_count >= SECOND_GENERATION_MEMBER_COUNT);
    let blobs = (0..member_count)
        .map(|ordinal| {
            let path = format!("src/performance_{ordinal}.rs");
            let content = format!("pub fn performance_{ordinal}() -> usize {{ {ordinal} }}\n");
            blob_name(&path, &content).expect("build canonical performance Blob")
        })
        .collect::<Vec<_>>();
    for start in (0..member_count).step_by(WRITE_BATCH_ROWS) {
        let end = (start + WRITE_BATCH_ROWS).min(member_count);
        // Updated members are written in generation two; all remaining members stay in generation one.
        // 已更新成员写入第二代；其余成员保留在第一代。
        if start < SECOND_GENERATION_MEMBER_COUNT {
            let updated_end = end.min(SECOND_GENERATION_MEMBER_COUNT);
            write_performance_rows(index, &blobs, start..updated_end, "perf-new", 2).await;
            if updated_end < end {
                write_performance_rows(index, &blobs, updated_end..end, "perf-base", 1).await;
            }
        } else {
            write_performance_rows(index, &blobs, start..end, "perf-base", 1).await;
        }
    }
    // Retain the replaced first-generation rows as physically present but checkpoint-invisible history.
    // 保留被替换的第一代行，使其物理存在但对检查点不可见。
    write_performance_rows(
        index,
        &blobs,
        0..HIDDEN_HISTORICAL_ROW_COUNT,
        "perf-base",
        1,
    )
    .await;
    blobs
}

/// Build the exact two-generation VCE filter and its allowed-member sets.
/// 构建精确的双代际 VCE 过滤条件及其允许成员集合。
///
/// `index`, `blobs`, and `member_count` select one pinned logical view. Returns the complete
/// physical predicate, allowed Blobs, and second-generation Blobs used to validate every result.
/// `index`、`blobs` 与 `member_count` 选择一个固定逻辑视图。返回完整物理谓词、允许 Blob
/// 及用于校验每条结果的第二代 Blob。
fn performance_filter(
    index: &VceProjectIndex,
    blobs: &[String],
    member_count: usize,
) -> (String, BTreeSet<String>, BTreeSet<String>) {
    let allowed = blobs[..member_count]
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let updated = blobs[..SECOND_GENERATION_MEMBER_COUNT]
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let base = allowed
        .difference(&updated)
        .cloned()
        .collect::<BTreeSet<_>>();
    let scope = format!(
        "(refresh_id = 'perf-base' AND generation = 1 AND blob_name IN ({})) OR (refresh_id = 'perf-new' AND generation = 2 AND blob_name IN ({}))",
        lance_string_set(&base),
        lance_string_set(&updated),
    );
    let filter = format!(
        "namespace_id = '{}' AND vector_space_id = '{}' AND physical_fingerprint = '{}' AND ({scope})",
        lance_string_literal(&index.binding.namespace_id),
        lance_string_literal(&index.vector_space_id),
        lance_string_literal(&index.physical_fingerprint),
    );
    (filter, allowed, updated)
}

/// Execute and validate one bounded real-engine vector query.
/// 执行并校验一次有界真实引擎向量查询。
///
/// `index` and `role` select the table, `filter` freezes the physical scope, and `allowed` plus
/// `updated` define the exact logical view. Returns elapsed time and the ordered result signature.
/// `index` 与 `role` 选择表，`filter` 冻结物理范围，`allowed` 与 `updated` 定义精确逻辑视图。
/// 返回耗时及保留数据库结果顺序的签名。
async fn measured_search(
    index: &VceProjectIndex,
    role: VcePhysicalTableRole,
    filter: &str,
    allowed: &BTreeSet<String>,
    updated: &BTreeSet<String>,
) -> (Duration, Vec<String>) {
    let mut query = vec![0.0; PERFORMANCE_DIMENSION];
    query[0] = 1.0;
    let table_name = match role {
        VcePhysicalTableRole::Content => index.content_table.as_str(),
        VcePhysicalTableRole::Path => index.path_table.as_str(),
    };
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        index
            .data_plane
            .vector_engine()
            .await
            .expect("open performance vector engine")
            .vector_search(LanceDbSearchInput {
                table_name: table_name.to_string(),
                vector: query,
                limit: QUERY_LIMIT,
                filter: filter.to_string(),
                vector_column: String::from("vector"),
                output_format: LanceDbOutputFormat::JsonRows,
            }),
    )
    .await
    .expect("performance query must finish within the original timeout")
    .expect("real in-process engine performance query succeeds");
    let elapsed = started.elapsed();
    let rows = serde_json::from_slice::<Vec<serde_json::Value>>(&result.data)
        .expect("decode performance query rows");
    assert_eq!(rows.len(), QUERY_LIMIT as usize);
    let mut seen_blobs = BTreeSet::new();
    let mut signature = Vec::with_capacity(rows.len());
    for row in rows {
        let blob = row
            .get("blob_name")
            .and_then(serde_json::Value::as_str)
            .expect("query row has Blob identity");
        let refresh_id = row
            .get("refresh_id")
            .and_then(serde_json::Value::as_str)
            .expect("query row has refresh identity");
        let generation = row
            .get("generation")
            .and_then(serde_json::Value::as_u64)
            .expect("query row has generation");
        assert!(
            allowed.contains(blob),
            "query escaped its pinned member set"
        );
        assert!(
            seen_blobs.insert(blob.to_owned()),
            "query duplicated one Blob"
        );
        if updated.contains(blob) {
            assert_eq!((refresh_id, generation), ("perf-new", 2));
        } else {
            assert_eq!((refresh_id, generation), ("perf-base", 1));
        }
        signature.push(
            row.get("row_id")
                .and_then(serde_json::Value::as_str)
                .expect("query row has stable row identity")
                .to_owned(),
        );
    }
    (elapsed, signature)
}

/// Measure one cold query, warm sequential repetitions, and optional three-way groups.
/// 测量一次冷查询、预热后的顺序重复以及可选三路并发组。
///
/// `index`, `role`, and scope arguments select the exact query. `concurrent_groups` controls only
/// the additional concurrency workload. Returns all observations and one equivalence signature.
/// `index`、`role` 与范围参数选择精确查询。`concurrent_groups` 仅控制额外并发负载。
/// 返回全部观察值及一个等价性签名。
async fn measure_query_profile(
    index: &VceProjectIndex,
    role: VcePhysicalTableRole,
    filter: &str,
    allowed: &BTreeSet<String>,
    updated: &BTreeSet<String>,
    concurrent_groups: usize,
) -> QueryMeasurements {
    let (cold, signature) = measured_search(index, role, filter, allowed, updated).await;
    let (_, warmed_signature) = measured_search(index, role, filter, allowed, updated).await;
    assert_eq!(warmed_signature, signature);
    let mut single = Vec::with_capacity(QUERY_REPETITIONS);
    for _ in 0..QUERY_REPETITIONS {
        let (elapsed, current) = measured_search(index, role, filter, allowed, updated).await;
        assert_eq!(current, signature);
        single.push(elapsed);
    }
    let mut concurrent_calls = Vec::with_capacity(concurrent_groups * 3);
    let mut concurrent_group_durations = Vec::with_capacity(concurrent_groups);
    for _ in 0..concurrent_groups {
        let started = Instant::now();
        let (first, second, third) = tokio::join!(
            measured_search(index, role, filter, allowed, updated),
            measured_search(index, role, filter, allowed, updated),
            measured_search(index, role, filter, allowed, updated),
        );
        concurrent_group_durations.push(started.elapsed());
        for (elapsed, current) in [first, second, third] {
            assert_eq!(current, signature);
            concurrent_calls.push(elapsed);
        }
    }
    QueryMeasurements {
        cold,
        single,
        concurrent_calls,
        concurrent_groups: concurrent_group_durations,
        signature,
    }
}

/// Return deterministic median, ninety-fifth percentile, and maximum microseconds.
/// 返回确定性的中位数、百分之九十五分位及最大微秒数。
///
/// `samples` must be non-empty. Returns integer microseconds in percentile order.
/// `samples` 必须非空。按分位顺序返回整数微秒数。
fn duration_summary(samples: &[Duration]) -> (u128, u128, u128) {
    assert!(!samples.is_empty());
    let mut values = samples.iter().map(Duration::as_micros).collect::<Vec<_>>();
    values.sort_unstable();
    let percentile = |percent: usize| {
        let rank = (values.len() * percent).div_ceil(100).max(1);
        values[rank - 1]
    };
    (percentile(50), percentile(95), values[values.len() - 1])
}

/// Print one stable machine-readable performance profile and return its single-query p95.
/// 打印一条稳定且机器可读的性能配置，并返回其单路查询百分之九十五分位。
///
/// `phase`, `role`, `member_count`, and `measurements` identify one before/after sample group.
/// Returns the warmed sequential p95 duration.
/// `phase`、`role`、`member_count` 与 `measurements` 标识一组整理前后样本。返回预热顺序请求 p95。
fn report_query_profile(
    phase: &str,
    role: VcePhysicalTableRole,
    member_count: usize,
    filter_bytes: usize,
    measurements: &QueryMeasurements,
) -> Duration {
    let (single_p50, single_p95, single_max) = duration_summary(&measurements.single);
    let concurrent = if measurements.concurrent_calls.is_empty() {
        String::from("concurrent_calls=0")
    } else {
        let (call_p50, call_p95, call_max) = duration_summary(&measurements.concurrent_calls);
        let (group_p50, group_p95, group_max) = duration_summary(&measurements.concurrent_groups);
        format!(
            "concurrent_calls={} concurrent_call_p50_us={call_p50} concurrent_call_p95_us={call_p95} concurrent_call_max_us={call_max} concurrent_group_p50_us={group_p50} concurrent_group_p95_us={group_p95} concurrent_group_max_us={group_max}",
            measurements.concurrent_calls.len(),
        )
    };
    println!(
        "VCE_NATIVE_COMPACTION_PERF phase={phase} role={role:?} members={member_count} filter_bytes={filter_bytes} cold_us={} single_samples={} single_p50_us={single_p50} single_p95_us={single_p95} single_max_us={single_max} {concurrent}",
        measurements.cold.as_micros(),
        measurements.single.len(),
    );
    Duration::from_micros(u64::try_from(single_p95).expect("p95 duration fits u64"))
}

/// Compact one physical table through the production VCE in-process adapter.
/// 通过生产 VCE 进程内适配器整理一张物理表。
///
/// `index` and `role` select the exact authorized table. Returns authoritative before/after
/// statistics and elapsed wall time after the engine operation succeeds.
/// `index` 与 `role` 选择精确授权表。引擎操作成功后返回权威整理前后统计与墙钟耗时。
async fn compact_performance_table(
    index: &VceProjectIndex,
    role: VcePhysicalTableRole,
) -> (
    VcePhysicalTableStatistics,
    VcePhysicalTableStatistics,
    Duration,
) {
    let before = index
        .physical_table_statistics(role)
        .await
        .expect("read pre-compaction statistics");
    let operation_id = format!("performance-compact-{role:?}-{}", Uuid::new_v4());
    let started = Instant::now();
    let mut operation = index
        .submit_physical_compaction(
            role,
            &operation_id,
            before.version,
            &physical_compaction_options(),
        )
        .await
        .expect("submit performance compaction");
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            match operation.state {
                VcePhysicalCompactionState::Succeeded => break,
                VcePhysicalCompactionState::Failed => {
                    panic!("performance compaction failed: {:?}", operation.error)
                }
                VcePhysicalCompactionState::Cancelled => {
                    panic!("performance compaction was cancelled")
                }
                VcePhysicalCompactionState::Queued | VcePhysicalCompactionState::Running => {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    operation = index
                        .physical_compaction_operation(role, &operation_id)
                        .await
                        .expect("query performance compaction operation");
                }
                VcePhysicalCompactionState::Unspecified => {
                    panic!("performance compaction returned an unspecified state")
                }
            }
        }
    })
    .await
    .expect("performance compaction must finish before deadline");
    let elapsed = started.elapsed();
    let after = index
        .physical_table_statistics(role)
        .await
        .expect("read post-compaction statistics");
    println!(
        "VCE_NATIVE_COMPACTION_LAYOUT role={role:?} elapsed_ms={} rows_before={} rows_after={} fragments_before={} fragments_after={} bytes_before={} bytes_after={} version_before={} version_after={}",
        elapsed.as_millis(),
        before.row_count,
        after.row_count,
        before.fragment_count,
        after.fragment_count,
        before.total_bytes,
        after.total_bytes,
        before.version,
        after.version,
    );
    (before, after, elapsed)
}

/// Benchmark both VCE vector tables before and after native physical compaction.
/// 对原生物理整理前后的两张 VCE 向量表进行基准测试。
///
/// Takes no parameters. Uses one temporary database fixture and performs no provider
/// calls or writes to user storage.
/// 无参数。仅使用一个临时数据库夹具，不调用供应商也不写入用户存储。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "runs the full performance matrix; not a per-commit gate"]
async fn in_process_native_compaction_meets_vce_performance_targets() {
    let temporary = tempfile::tempdir().expect("create VCE performance root");
    let regression_project_root = temporary.path().join("project-regression");
    let scale_project_root = temporary.path().join("project-scale");
    let data_root = temporary.path().join("data");
    std::fs::create_dir_all(&regression_project_root)
        .expect("create regression performance project root");
    std::fs::create_dir_all(&scale_project_root).expect("create scale performance project root");
    std::fs::create_dir_all(&data_root).expect("create performance data root");
    let host_runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: data_root.clone(),
        sqlite_path: data_root.join("unused.sqlite3"),
        sqlite_binding_id: Uuid::new_v4().to_string(),
        system_statistics_sqlite_path: data_root.join("unused-statistics.sqlite3"),
        system_statistics_sqlite_binding_id: Uuid::new_v4().to_string(),
    });
    let factory = bootstrap_vce_data_plane(&host_runtime)
        .await
        .expect("bootstrap performance VCE data plane");
    let regression_index = factory
        .create_project_index(performance_project_config(
            "performance-regression-project",
            &regression_project_root,
        ))
        .await
        .expect("create regression performance project index");
    let scale_index = factory
        .create_project_index(performance_project_config(
            "performance-scale-project",
            &scale_project_root,
        ))
        .await
        .expect("create scale performance project index");
    let regression_blobs =
        populate_performance_tables(&regression_index, REGRESSION_MEMBER_COUNT).await;
    let scale_blobs = populate_performance_tables(&scale_index, PERFORMANCE_MEMBER_COUNT).await;
    let (regression_filter, regression_allowed, updated) = performance_filter(
        &regression_index,
        &regression_blobs,
        REGRESSION_MEMBER_COUNT,
    );
    let (scale_filter, scale_allowed, scale_updated) =
        performance_filter(&scale_index, &scale_blobs, PERFORMANCE_MEMBER_COUNT);
    assert_eq!(updated.len(), SECOND_GENERATION_MEMBER_COUNT);
    assert_eq!(scale_updated.len(), SECOND_GENERATION_MEMBER_COUNT);
    println!(
        "VCE_NATIVE_COMPACTION_FIXTURE dimension={} current_members={} second_generation_members={} historical_rows={} regression_members={} regression_filter_bytes={} scale_filter_bytes={}",
        PERFORMANCE_DIMENSION,
        PERFORMANCE_MEMBER_COUNT,
        SECOND_GENERATION_MEMBER_COUNT,
        HIDDEN_HISTORICAL_ROW_COUNT,
        REGRESSION_MEMBER_COUNT,
        regression_filter.len(),
        scale_filter.len(),
    );

    for role in [VcePhysicalTableRole::Content, VcePhysicalTableRole::Path] {
        let before_regression = measure_query_profile(
            &regression_index,
            role,
            &regression_filter,
            &regression_allowed,
            &updated,
            CONCURRENT_QUERY_GROUPS,
        )
        .await;
        report_query_profile(
            "before",
            role,
            REGRESSION_MEMBER_COUNT,
            regression_filter.len(),
            &before_regression,
        );

        let (regression_before_layout, regression_after_layout, _) =
            compact_performance_table(&regression_index, role).await;
        assert_eq!(
            regression_before_layout.row_count,
            (REGRESSION_MEMBER_COUNT + HIDDEN_HISTORICAL_ROW_COUNT) as u64
        );
        assert_eq!(
            regression_after_layout.row_count,
            regression_before_layout.row_count
        );
        assert!(
            regression_after_layout.fragment_count.saturating_mul(10)
                <= regression_before_layout.fragment_count,
            "native compaction must reduce the regression fixture by at least one order of magnitude"
        );

        let after_regression = measure_query_profile(
            &regression_index,
            role,
            &regression_filter,
            &regression_allowed,
            &updated,
            CONCURRENT_QUERY_GROUPS,
        )
        .await;
        let post_p95 = report_query_profile(
            "after",
            role,
            REGRESSION_MEMBER_COUNT,
            regression_filter.len(),
            &after_regression,
        );
        assert_eq!(after_regression.signature, before_regression.signature);
        assert!(
            post_p95 <= Duration::from_secs(2),
            "post-compaction single-query p95 must remain within two seconds: {post_p95:?}"
        );
        assert!(
            after_regression
                .concurrent_calls
                .iter()
                .all(|duration| *duration <= Duration::from_secs(5)),
            "every post-compaction three-way request must remain within five seconds"
        );

        let before_scale = measure_query_profile(
            &scale_index,
            role,
            &scale_filter,
            &scale_allowed,
            &scale_updated,
            0,
        )
        .await;
        report_query_profile(
            "before",
            role,
            PERFORMANCE_MEMBER_COUNT,
            scale_filter.len(),
            &before_scale,
        );
        let (scale_before_layout, scale_after_layout, _) =
            compact_performance_table(&scale_index, role).await;
        assert_eq!(
            scale_before_layout.row_count,
            (PERFORMANCE_MEMBER_COUNT + HIDDEN_HISTORICAL_ROW_COUNT) as u64
        );
        assert_eq!(scale_after_layout.row_count, scale_before_layout.row_count);
        assert!(
            scale_after_layout.fragment_count.saturating_mul(10)
                <= scale_before_layout.fragment_count,
            "native compaction must reduce the scale fixture by at least one order of magnitude"
        );
        let after_scale = measure_query_profile(
            &scale_index,
            role,
            &scale_filter,
            &scale_allowed,
            &scale_updated,
            0,
        )
        .await;
        report_query_profile(
            "after",
            role,
            PERFORMANCE_MEMBER_COUNT,
            scale_filter.len(),
            &after_scale,
        );
        assert_eq!(after_scale.signature, before_scale.signature);
    }

    drop(regression_index);
    drop(scale_index);
    factory
        .shutdown()
        .await
        .expect("close performance VCE bindings");
}
