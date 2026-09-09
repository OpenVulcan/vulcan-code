//! Filesystem-only immutable source adapter for Vulcan Context Engine.
//! Vulcan Context Engine 使用的纯文件系统不可变源码适配器。

use std::collections::{BTreeSet, HashSet};
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::time::UNIX_EPOCH;

use vulcan_context_engine::{
    FileObservation, FileSource, VceFuture, VcePortError, VcePortOperation, VceSourceDocument,
    VceSourcePort, VceSourceReadRequest, VceSourceReadResponse, blob_name, normalize_relative_path,
};
use vulcan_context_engine::{
    VceRefreshControl, VceRefreshPhase, VceRefreshProgress, VceRefreshUnit,
};
use vulcan_service_api::WorkspaceFileFilterOptions;

use crate::workspace_files::{
    workspace_file_canonical_root, workspace_file_walk_builder, workspace_path_is_inside_root,
};

/// Default upper bound for files represented by one full project manifest.
/// 单个完整项目清单允许表示的默认文件数量上限。
const DEFAULT_MANIFEST_FILE_LIMIT: usize = 200_000;

/// Host-owned limits that protect VCE manifest and deferred source reads.
/// 保护 VCE 清单与延迟源码读取的宿主持有限制。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VceSourceLimits {
    /// Maximum admitted source file size in bytes.
    /// 单个准入源码文件允许的最大字节数。
    pub(crate) max_file_bytes: u64,
    /// Maximum document count accepted by one deferred source request.
    /// 单次延迟源码请求允许的最大文档数量。
    pub(crate) max_batch_documents: usize,
    /// Maximum aggregate source bytes accepted by one deferred source request.
    /// 单次延迟源码请求允许的源码总字节数。
    pub(crate) max_batch_bytes: u64,
    /// Maximum file count represented by one full project scan.
    /// 单次完整项目扫描允许表示的最大文件数量。
    pub(crate) max_manifest_files: usize,
}

impl VceSourceLimits {
    /// Build limits aligned with one VCE engine configuration.
    /// 构建与一份 VCE 引擎配置对齐的限制。
    ///
    /// # Parameters
    /// 参数
    /// `max_file_bytes` is the VCE per-file indexing limit.
    /// `max_file_bytes` 是 VCE 单文件索引上限。
    /// `max_batch_documents` is the VCE staging Blob count limit.
    /// `max_batch_documents` 是 VCE 暂存 Blob 数量上限。
    /// `max_batch_bytes` is the VCE staging source byte limit.
    /// `max_batch_bytes` 是 VCE 暂存源码字节上限。
    ///
    /// # Returns
    /// 返回值
    /// A validated host source-limit contract.
    /// 返回经过校验的宿主源码限制契约。
    pub(crate) fn new(
        max_file_bytes: u64,
        max_batch_documents: usize,
        max_batch_bytes: u64,
    ) -> Result<Self, String> {
        let limits = Self {
            max_file_bytes,
            max_batch_documents,
            max_batch_bytes,
            max_manifest_files: DEFAULT_MANIFEST_FILE_LIMIT,
        };
        limits.validate()?;
        Ok(limits)
    }

    /// Validate that every source guard is finite and internally compatible.
    /// 校验每项源码保护限制均为有限正值且内部兼容。
    ///
    /// # Returns
    /// 返回值
    /// Success for a usable contract, otherwise one explicit diagnostic.
    /// 契约可用时返回成功，否则返回一条明确诊断。
    fn validate(&self) -> Result<(), String> {
        if self.max_file_bytes == 0 {
            return Err(String::from("VCE max_file_bytes must be greater than zero"));
        }
        if self.max_batch_documents == 0 {
            return Err(String::from(
                "VCE max_batch_documents must be greater than zero",
            ));
        }
        if self.max_batch_bytes < self.max_file_bytes {
            return Err(String::from(
                "VCE max_batch_bytes must cover at least one maximum-sized file",
            ));
        }
        if self.max_manifest_files == 0 {
            return Err(String::from(
                "VCE max_manifest_files must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// Metadata-only immutable manifest supplied to one full VCE refresh.
/// 提供给一次 VCE 全量刷新的仅元数据不可变清单。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VceSourceManifest {
    /// Deterministically ordered source observations.
    /// 按确定顺序排列的源码观察结果。
    pub(crate) observations: Vec<FileObservation>,
    /// Number of source files admitted for semantic indexing.
    /// 准入语义索引的源码文件数量。
    pub(crate) admitted_files: usize,
    /// Number of files recorded as intentionally skipped.
    /// 被明确记录为跳过的文件数量。
    pub(crate) skipped_files: usize,
    /// Aggregate UTF-8 byte size represented by admitted files.
    /// 已准入文件表示的 UTF-8 总字节数。
    pub(crate) admitted_bytes: u64,
}

/// Metadata delta supplied to one path-scoped VCE refresh.
/// 提供给一次路径级 VCE 刷新的元数据增量。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VcePathRefresh {
    /// Changed paths that still produce source observations.
    /// 仍可产生源码观察结果的变更路径。
    pub(crate) observations: Vec<FileObservation>,
    /// Changed paths that must be removed from the current VCE workspace state.
    /// 必须从当前 VCE 工作区状态移除的变更路径。
    pub(crate) removed_paths: Vec<String>,
    /// Whether an ignore file or directory change requires a full rescan.
    /// 忽略文件或目录变化是否要求执行完整重扫。
    pub(crate) requires_full_refresh: bool,
}

/// Project-scoped filesystem implementation of `VceSourcePort`.
/// 项目作用域的 `VceSourcePort` 文件系统实现。
#[derive(Clone, Debug)]
pub(crate) struct VceProjectSourceAdapter {
    /// Canonical project root used as the immutable security boundary.
    /// 作为不可变安全边界的规范项目根目录。
    project_root: Arc<PathBuf>,
    /// Source admission and deferred-read limits.
    /// 源码准入与延迟读取限制。
    limits: VceSourceLimits,
    /// Compiled project-owned ignore rules applied after mandatory hard exclusions.
    /// 在必需硬排除规则之后应用的已编译项目自有忽略规则。
    extra_ignore: Arc<Gitignore>,
}

impl VceProjectSourceAdapter {
    /// Activate a filesystem-only source adapter for one canonical project root.
    /// 为一个规范项目根目录激活纯文件系统源码适配器。
    ///
    /// # Parameters
    /// 参数
    /// `project_root` is the authoritative VulcanCode project root.
    /// `project_root` 是权威 VulcanCode 项目根目录。
    /// `limits` must match the VCE engine staging limits.
    /// `limits` 必须与 VCE 引擎暂存限制一致。
    ///
    /// # Returns
    /// 返回值
    /// A project-bound adapter or an explicit activation error.
    /// 返回项目绑定适配器或明确的激活错误。
    pub(crate) fn activate(
        project_root: impl AsRef<Path>,
        limits: VceSourceLimits,
        extra_ignore_patterns: &[String],
    ) -> Result<Self, String> {
        limits.validate()?;
        let root_text = project_root.as_ref().to_string_lossy();
        let project_root = workspace_file_canonical_root(&root_text)?;
        let extra_ignore = compile_extra_ignore(&project_root, extra_ignore_patterns)?;
        Ok(Self {
            project_root: Arc::new(project_root),
            limits,
            extra_ignore: Arc::new(extra_ignore),
        })
    }

    /// Return the canonical project root bound to this adapter.
    /// 返回此适配器绑定的规范项目根目录。
    ///
    /// # Returns
    /// 返回值
    /// The immutable canonical project-root path.
    /// 返回不可变的规范项目根路径。
    pub(crate) fn project_root(&self) -> &Path {
        self.project_root.as_path()
    }

    /// Scan one complete project into metadata-only VCE observations.
    /// 将一个完整项目扫描为仅元数据的 VCE 观察结果。
    ///
    /// # Returns
    /// 返回值
    /// A bounded deterministic manifest that does not retain source bodies.
    /// 返回不保留源码正文的有界确定性清单。
    #[cfg(test)]
    async fn scan_full(&self) -> Result<VceSourceManifest, String> {
        self.scan_full_observed(VceRefreshControl::default(), String::from("source-scan"))
            .await
    }

    /// Scan using the exact host job's cancellation and real file-progress observer.
    /// 使用精确宿主作业的取消与真实文件进度观察器扫描。
    ///
    /// `control` is shared with later indexing; `operation_id` identifies this scan. Returns metadata
    /// only after the blocking worker has completed or acknowledged cooperative cancellation.
    /// `control` 与后续索引共享；`operation_id` 标识此扫描。仅在阻塞工作完成或确认协作取消后返回元数据。
    pub(crate) async fn scan_full_observed(
        &self,
        control: VceRefreshControl,
        operation_id: String,
    ) -> Result<VceSourceManifest, String> {
        // adapter is an immutable clone owned by the blocking worker until its scan exits.
        // adapter 是由阻塞工作线程持有到扫描退出的不可变克隆。
        let adapter = self.clone();
        tokio::task::spawn_blocking(move || adapter.scan_full_blocking(&control, &operation_id))
            .await
            .map_err(|error| format!("VCE full source scan task failed: {error}"))?
    }

    /// Observe one normalized watcher path set for an incremental VCE refresh.
    /// 为一次增量 VCE 刷新观察一组规范化监听路径。
    ///
    /// # Parameters
    /// 参数
    /// `relative_paths` contains workspace-relative watcher paths only.
    /// `relative_paths` 仅包含工作区相对监听路径。
    ///
    /// # Returns
    /// 返回值
    /// Metadata observations, removals, and the full-refresh escalation flag.
    /// 返回元数据观察、删除项与全量刷新升级标记。
    #[cfg(test)]
    async fn scan_paths(&self, relative_paths: BTreeSet<String>) -> Result<VcePathRefresh, String> {
        self.scan_paths_observed(
            relative_paths,
            VceRefreshControl::default(),
            String::from("source-path-scan"),
        )
        .await
    }

    /// Observe a path delta under the same cancellation boundary as subsequent indexing.
    /// 在与后续索引相同的取消边界下观察路径差异。
    ///
    /// `relative_paths` is the exact dirty set; `control` and `operation_id` belong to the host job.
    /// Returns observations/removals only after the worker exits, without retaining detached scanning.
    /// `relative_paths` 是精确脏路径集；`control` 与 `operation_id` 属于宿主作业。
    /// 仅在工作线程退出后返回观察与删除项，不保留脱离作业的扫描。
    pub(crate) async fn scan_paths_observed(
        &self,
        relative_paths: BTreeSet<String>,
        control: VceRefreshControl,
        operation_id: String,
    ) -> Result<VcePathRefresh, String> {
        // adapter freezes the same root and filters for the bounded blocking worker.
        // adapter 为有界阻塞工作线程冻结相同根目录与过滤规则。
        let adapter = self.clone();
        tokio::task::spawn_blocking(move || {
            adapter.scan_paths_blocking(relative_paths, &control, &operation_id)
        })
        .await
        .map_err(|error| format!("VCE path source scan task failed: {error}"))?
    }

    /// Build the shared source walker, optionally pruning unrelated paths before descending.
    /// 构建共享源码遍历器，可选择在向下遍历之前剪除无关路径。
    /// `allowed_nodes` contains exact changed paths and their ancestors, or is absent for a full scan.
    /// `allowed_nodes` 包含精确变更路径及其祖先，全量扫描时为空。
    /// Returns the host walker with hierarchical ignore, hard exclusions, and project rules intact.
    /// 返回保留分层忽略、硬排除和项目规则的宿主遍历器。
    fn source_walk_builder(&self, allowed_nodes: Option<HashSet<PathBuf>>) -> ignore::WalkBuilder {
        // filters retain the authoritative host filesystem policy for both entry points.
        // filters 为两个入口保留权威宿主文件系统策略。
        let filters = WorkspaceFileFilterOptions::default();
        let mut walker = workspace_file_walk_builder(self.project_root(), &filters, None);
        walker.add_custom_ignore_filename(".oceignore");
        // root and extra_ignore remain immutable for the lifetime of the traversal closure.
        // root 与 extra_ignore 在遍历闭包的整个生命周期中保持不可变。
        let root = Arc::clone(&self.project_root);
        let extra_ignore = Arc::clone(&self.extra_ignore);
        walker.filter_entry(move |entry| {
            allowed_nodes
                .as_ref()
                .is_none_or(|nodes| nodes.contains(entry.path()))
                && !is_hard_ignored_entry(root.as_path(), entry.path())
                && !entry
                    .path()
                    .strip_prefix(root.as_path())
                    .ok()
                    .is_some_and(|relative| {
                        extra_ignore
                            .matched_path_or_any_parents(
                                relative,
                                entry.file_type().is_some_and(|kind| kind.is_dir()),
                            )
                            .is_ignore()
                    })
        });
        walker
    }

    /// Resolve admitted changed files through the same hierarchical policy as a complete scan.
    /// 通过与完整扫描相同的分层策略解析准入的变更文件。
    /// `relative_paths` is the dirty set; `control` cancels traversal. Returns admitted file identities.
    /// `relative_paths` 为脏路径集；`control` 取消遍历。返回已准入文件身份。
    /// Unrelated directories are pruned, and no source body is read by this admission pass.
    /// 无关目录被剪除，此准入阶段不读取任何源码正文。
    fn admitted_changed_paths(
        &self,
        relative_paths: &BTreeSet<String>,
        control: &VceRefreshControl,
    ) -> Result<HashSet<String>, String> {
        // allowed_nodes contains only the requested paths and root-bounded ancestors.
        // allowed_nodes 仅包含请求路径与受根边界限制的祖先。
        let mut allowed_nodes = HashSet::new();
        for relative_path in relative_paths {
            control.check().map_err(|error| error.to_string())?;
            let normalized = validate_relative_path(relative_path)?;
            let absolute = self.project_root().join(normalized);
            for ancestor in absolute.ancestors() {
                if !ancestor.starts_with(self.project_root()) {
                    break;
                }
                allowed_nodes.insert(ancestor.to_path_buf());
            }
        }
        // admitted_files is metadata-only; actual reads still verify the immutable source fingerprint.
        // admitted_files 仅含元数据身份；实际读取仍校验不可变源码指纹。
        let mut admitted_files = HashSet::new();
        for entry in self.source_walk_builder(Some(allowed_nodes)).build() {
            control.check().map_err(|error| error.to_string())?;
            let entry =
                entry.map_err(|error| format!("scan VCE changed path admission: {error}"))?;
            if entry.file_type().is_some_and(|kind| kind.is_file()) {
                admitted_files.insert(relative_path_text(self.project_root(), entry.path())?);
            }
        }
        control.check().map_err(|error| error.to_string())?;
        Ok(admitted_files)
    }

    /// Perform one complete blocking scan behind the asynchronous boundary.
    /// 在异步边界之后执行一次完整阻塞扫描。
    ///
    /// # Returns
    /// 返回值
    /// A complete bounded metadata manifest.
    /// 返回完整且有界的元数据清单。
    fn scan_full_blocking(
        &self,
        control: &VceRefreshControl,
        operation_id: &str,
    ) -> Result<VceSourceManifest, String> {
        control.check().map_err(|error| error.to_string())?;
        control.observe(VceRefreshProgress {
            phase: VceRefreshPhase::Scanning,
            operation_id: operation_id.to_owned(),
            unit: VceRefreshUnit::Files,
            completed: 0,
            total: None,
            overall_completed: 0,
            overall_total: None,
            current_path: None,
        });
        // walker uses the same admission rules as path-scoped refreshes without pruning.
        // walker 与路径级刷新使用同一准入规则，此处不剪枝。
        let walker = self.source_walk_builder(None);

        let mut observations = Vec::new();
        let mut admitted_files = 0usize;
        let mut skipped_files = 0usize;
        let mut admitted_bytes = 0u64;
        for walked_entry in walker.build() {
            control.check().map_err(|error| error.to_string())?;
            let walked_entry = walked_entry
                .map_err(|error| format!("scan VCE project source manifest: {error}"))?;
            if walked_entry.depth() == 0
                || !walked_entry.file_type().is_some_and(|kind| kind.is_file())
            {
                continue;
            }
            if observations.len() >= self.limits.max_manifest_files {
                return Err(format!(
                    "VCE source manifest exceeds {} files",
                    self.limits.max_manifest_files
                ));
            }
            let relative_path = relative_path_text(self.project_root(), walked_entry.path())?;
            match self.observe_present_path(&relative_path, None)? {
                ObservedPath::Observation(observation) => {
                    if observation.skip_reason.is_some() {
                        skipped_files += 1;
                    } else {
                        admitted_files += 1;
                        admitted_bytes =
                            admitted_bytes
                                .checked_add(observation.size)
                                .ok_or_else(|| {
                                    String::from("VCE source manifest byte count overflowed")
                                })?;
                    }
                    observations.push(observation);
                }
                ObservedPath::Removed => {}
                ObservedPath::RequiresFullRefresh => {
                    return Err(format!(
                        "VCE full scan encountered an unstable directory `{relative_path}`"
                    ));
                }
            }
            control.observe(VceRefreshProgress {
                phase: VceRefreshPhase::Scanning,
                operation_id: operation_id.to_owned(),
                unit: VceRefreshUnit::Files,
                completed: (admitted_files + skipped_files) as u64,
                total: None,
                overall_completed: 0,
                overall_total: None,
                current_path: Some(relative_path),
            });
        }
        control.check().map_err(|error| error.to_string())?;
        observations.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(VceSourceManifest {
            observations,
            admitted_files,
            skipped_files,
            admitted_bytes,
        })
    }

    /// Perform one path-scoped blocking scan behind the asynchronous boundary.
    /// 在异步边界之后执行一次路径级阻塞扫描。
    ///
    /// # Parameters
    /// 参数
    /// `relative_paths` is the deduplicated watcher path set.
    /// `relative_paths` 是已去重的监听路径集合。
    ///
    /// # Returns
    /// 返回值
    /// One deterministic path-refresh input.
    /// 返回一份确定性的路径刷新输入。
    fn scan_paths_blocking(
        &self,
        relative_paths: BTreeSet<String>,
        control: &VceRefreshControl,
        operation_id: &str,
    ) -> Result<VcePathRefresh, String> {
        // total is known for the exact admitted path set, unlike a filesystem walk.
        // total 对精确准入路径集已知，与文件系统遍历不同。
        let total = relative_paths.len() as u64;
        control.check().map_err(|error| error.to_string())?;
        control.observe(VceRefreshProgress {
            phase: VceRefreshPhase::Scanning,
            operation_id: operation_id.to_owned(),
            unit: VceRefreshUnit::Files,
            completed: 0,
            total: Some(total),
            overall_completed: 0,
            overall_total: Some(total),
            current_path: None,
        });
        let mut observations = Vec::new();
        let mut removed_paths = Vec::new();
        let mut requires_full_refresh = false;
        // admitted_paths comes from the production hierarchy-aware walker, never raw watcher events.
        // admitted_paths 来自生产分层规则遍历器，绝不直接信任原始监听事件。
        let admitted_paths = self.admitted_changed_paths(&relative_paths, control)?;
        for (position, relative_path) in relative_paths.into_iter().enumerate() {
            control.check().map_err(|error| error.to_string())?;
            let relative_path = validate_relative_path(&relative_path)?;
            if is_ignore_control_path(&relative_path) {
                requires_full_refresh = true;
            } else {
                match self.observe_present_path(&relative_path, Some(&admitted_paths))? {
                    ObservedPath::Observation(observation) => observations.push(observation),
                    ObservedPath::Removed => removed_paths.push(relative_path.clone()),
                    ObservedPath::RequiresFullRefresh => requires_full_refresh = true,
                }
            }
            control.observe(VceRefreshProgress {
                phase: VceRefreshPhase::Scanning,
                operation_id: operation_id.to_owned(),
                unit: VceRefreshUnit::Files,
                completed: position as u64 + 1,
                total: Some(total),
                overall_completed: position as u64 + 1,
                overall_total: Some(total),
                current_path: Some(relative_path),
            });
        }
        control.check().map_err(|error| error.to_string())?;
        observations.sort_by(|left, right| left.path.cmp(&right.path));
        removed_paths.sort();
        Ok(VcePathRefresh {
            observations,
            removed_paths,
            requires_full_refresh,
        })
    }

    /// Convert one current path into an admitted, skipped, removed, or escalation result.
    /// 将一个当前路径转换为准入、跳过、删除或升级结果。
    ///
    /// # Parameters
    /// 参数
    /// `relative_path` is already canonical and workspace relative.
    /// `relative_path` 已经规范化且相对于工作区。
    ///
    /// # Returns
    /// 返回值
    /// The exact watcher observation outcome for this path.
    /// 返回此路径对应的精确监听观察结果。
    fn observe_present_path(
        &self,
        relative_path: &str,
        // admitted_paths is absent only after a full walker has already admitted this entry.
        // admitted_paths 仅在全量遍历器已经准入此条目时为空。
        admitted_paths: Option<&HashSet<String>>,
    ) -> Result<ObservedPath, String> {
        let resolved = match resolve_existing_source_path(self.project_root(), relative_path) {
            Ok(path) => path,
            Err(SourcePathError::Missing) => return Ok(ObservedPath::Removed),
            Err(SourcePathError::Rejected(reason)) => return Err(reason),
        };
        let metadata = fs::metadata(&resolved)
            .map_err(|error| format!("read VCE source metadata `{relative_path}`: {error}"))?;
        if metadata.is_dir() {
            return Ok(ObservedPath::RequiresFullRefresh);
        }
        if !metadata.is_file()
            || admitted_paths.is_some_and(|paths| !paths.contains(relative_path))
            || is_hard_ignored_relative_path(relative_path)
            || self
                .extra_ignore
                .matched_path_or_any_parents(Path::new(relative_path), false)
                .is_ignore()
        {
            return Ok(ObservedPath::Removed);
        }
        let size = metadata.len();
        let mtime_ns = metadata_mtime_ns(&metadata);
        if size > self.limits.max_file_bytes {
            return Ok(ObservedPath::Observation(FileObservation::skipped(
                relative_path,
                size,
                mtime_ns,
                format!(
                    "file size {size} exceeds VCE limit {}",
                    self.limits.max_file_bytes
                ),
                FileSource::Filesystem,
            )));
        }
        let snapshot = read_utf8_snapshot(
            self.project_root(),
            relative_path,
            self.limits.max_file_bytes,
        )?;
        let Some(content) = snapshot.content else {
            return Ok(ObservedPath::Observation(FileObservation::skipped(
                relative_path,
                snapshot.size,
                snapshot.mtime_ns,
                snapshot
                    .skip_reason
                    .unwrap_or_else(|| String::from("source is not UTF-8 text")),
                FileSource::Filesystem,
            )));
        };
        let identity = blob_name(relative_path, &content)
            .map_err(|error| format!("compute VCE Blob identity `{relative_path}`: {error}"))?;
        Ok(ObservedPath::Observation(FileObservation::identified(
            relative_path,
            identity,
            snapshot.size,
            snapshot.mtime_ns,
            FileSource::Filesystem,
        )))
    }

    /// Read and validate one deferred source batch synchronously.
    /// 同步读取并校验一个延迟源码批次。
    ///
    /// # Parameters
    /// 参数
    /// `request` contains exact path, Blob, size, and mtime expectations.
    /// `request` 包含精确的路径、Blob、大小与 mtime 预期。
    ///
    /// # Returns
    /// 返回值
    /// Exactly ordered immutable documents or one explicit stale-source error.
    /// 返回严格有序的不可变文档或一条明确的源码过期错误。
    fn read_sources_blocking(
        &self,
        request: VceSourceReadRequest,
    ) -> Result<VceSourceReadResponse, VcePortError> {
        if request.operation_id.trim().is_empty() {
            return Err(rejected_source("source operation_id must not be empty"));
        }
        if request.expectations.len() > self.limits.max_batch_documents {
            return Err(rejected_source(format!(
                "source request contains {} documents, exceeding limit {}",
                request.expectations.len(),
                self.limits.max_batch_documents
            )));
        }
        let mut expected_paths = HashSet::with_capacity(request.expectations.len());
        let mut expected_bytes = 0u64;
        for expectation in &request.expectations {
            let normalized = validate_relative_path(&expectation.path).map_err(rejected_source)?;
            if normalized != expectation.path {
                return Err(rejected_source(format!(
                    "source path `{}` is not canonical",
                    expectation.path
                )));
            }
            if !expected_paths.insert(expectation.path.clone()) {
                return Err(rejected_source(format!(
                    "source path `{}` is duplicated",
                    expectation.path
                )));
            }
            if expectation.size > self.limits.max_file_bytes {
                return Err(rejected_source(format!(
                    "source path `{}` exceeds per-file byte limit",
                    expectation.path
                )));
            }
            expected_bytes = expected_bytes
                .checked_add(expectation.size)
                .ok_or_else(|| rejected_source("source request byte count overflowed"))?;
            if expected_bytes > self.limits.max_batch_bytes {
                return Err(rejected_source(format!(
                    "source request bytes exceed limit {}",
                    self.limits.max_batch_bytes
                )));
            }
        }

        let mut documents = Vec::with_capacity(request.expectations.len());
        for expectation in &request.expectations {
            let snapshot = read_utf8_snapshot(
                self.project_root(),
                &expectation.path,
                self.limits.max_file_bytes,
            )
            .map_err(rejected_source)?;
            let content = snapshot.content.ok_or_else(|| {
                rejected_source(format!(
                    "source path `{}` is no longer admitted text: {}",
                    expectation.path,
                    snapshot
                        .skip_reason
                        .as_deref()
                        .unwrap_or("unknown text admission failure")
                ))
            })?;
            let actual_blob = blob_name(&expectation.path, &content).map_err(|error| {
                rejected_source(format!(
                    "compute source Blob `{}`: {error}",
                    expectation.path
                ))
            })?;
            if snapshot.size != expectation.size
                || snapshot.mtime_ns != expectation.mtime_ns
                || actual_blob != expectation.blob_name
            {
                return Err(rejected_source(format!(
                    "source snapshot changed for `{}`; refresh must be requeued",
                    expectation.path
                )));
            }
            documents.push(VceSourceDocument {
                path: expectation.path.clone(),
                content,
                size: snapshot.size,
                mtime_ns: snapshot.mtime_ns,
            });
        }
        Ok(VceSourceReadResponse {
            operation_id: request.operation_id,
            documents,
        })
    }
}

impl VceSourcePort for VceProjectSourceAdapter {
    /// Read exactly one VCE-requested immutable source batch.
    /// 精确读取一个 VCE 请求的不可变源码批次。
    ///
    /// # Parameters
    /// 参数
    /// `request` identifies the exact source versions VCE is permitted to read.
    /// `request` 标识 VCE 被允许读取的精确源码版本。
    ///
    /// # Returns
    /// 返回值
    /// A future resolving to exact documents or an explicit boundary error.
    /// 返回解析为精确文档或明确边界错误的 Future。
    fn read_sources(
        &self,
        request: VceSourceReadRequest,
    ) -> VceFuture<Result<VceSourceReadResponse, VcePortError>> {
        let adapter = self.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || adapter.read_sources_blocking(request))
                .await
                .map_err(|error| VcePortError::Failed {
                    operation: VcePortOperation::SourceRead,
                    message: format!("VCE source read task failed: {error}"),
                })?
        })
    }
}

/// One fully read filesystem snapshot used only during a bounded operation.
/// 仅在有界操作期间使用的一份完整文件系统快照。
struct SourceSnapshot {
    /// UTF-8 body when the source is admitted.
    /// 源码准入时的 UTF-8 正文。
    content: Option<String>,
    /// Exact byte size observed before and after reading.
    /// 读取前后观察到的精确字节大小。
    size: u64,
    /// Exact modification time observed before and after reading.
    /// 读取前后观察到的精确修改时间。
    mtime_ns: Option<u128>,
    /// Explicit admission failure when no content is returned.
    /// 未返回正文时的明确准入失败原因。
    skip_reason: Option<String>,
}

/// Internal watcher-path classification.
/// 内部监听路径分类。
enum ObservedPath {
    /// A present or explicitly skipped file observation.
    /// 存在或明确跳过的文件观察结果。
    Observation(FileObservation),
    /// A path that is absent or no longer admitted.
    /// 已不存在或不再准入的路径。
    Removed,
    /// A path whose meaning requires a complete ignore-aware rescan.
    /// 其含义要求执行完整 ignore 感知重扫的路径。
    RequiresFullRefresh,
}

/// Internal path-resolution failure that distinguishes deletion from rejection.
/// 区分删除与拒绝的内部路径解析失败。
enum SourcePathError {
    /// The requested path does not currently exist.
    /// 请求路径当前不存在。
    Missing,
    /// The requested path violates the workspace security boundary.
    /// 请求路径违反工作区安全边界。
    Rejected(String),
}

/// Compiles bounded project-owned gitignore-style rules against one canonical root.
/// 针对一个规范根目录编译有界的项目自有 gitignore 风格规则。
///
/// # Parameters
/// 参数
/// `project_root` is the canonical matching root; `patterns` are already validated persisted rules.
/// `project_root` 是规范匹配根；`patterns` 是已经校验的持久化规则。
///
/// # Returns
/// 返回值
/// A compiled matcher or the exact invalid pattern diagnostic.
/// 返回已编译匹配器或精确的无效规则诊断。
fn compile_extra_ignore(project_root: &Path, patterns: &[String]) -> Result<Gitignore, String> {
    let mut builder = GitignoreBuilder::new(project_root);
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .map_err(|error| format!("invalid VCE ignore pattern `{pattern}`: {error}"))?;
    }
    builder
        .build()
        .map_err(|error| format!("compile VCE ignore patterns: {error}"))
}

/// Read one source while proving stable metadata and UTF-8 admission.
/// 读取一份源码，同时证明元数据稳定并执行 UTF-8 准入。
///
/// # Parameters
/// 参数
/// `project_root` is the canonical security boundary.
/// `project_root` 是规范安全边界。
/// `relative_path` is one validated workspace-relative identity.
/// `relative_path` 是经过校验的工作区相对身份。
/// `max_file_bytes` is the maximum permitted payload.
/// `max_file_bytes` 是允许的最大载荷。
///
/// # Returns
/// 返回值
/// One stable admitted or skipped source snapshot.
/// 返回一份稳定的准入或跳过源码快照。
fn read_utf8_snapshot(
    project_root: &Path,
    relative_path: &str,
    max_file_bytes: u64,
) -> Result<SourceSnapshot, String> {
    let resolved =
        resolve_existing_source_path(project_root, relative_path).map_err(|error| match error {
            SourcePathError::Missing => format!("source path `{relative_path}` no longer exists"),
            SourcePathError::Rejected(reason) => reason,
        })?;
    let before = fs::metadata(&resolved)
        .map_err(|error| format!("read source metadata `{relative_path}`: {error}"))?;
    if !before.is_file() {
        return Err(format!("source path `{relative_path}` is not a file"));
    }
    if before.len() > max_file_bytes {
        return Ok(SourceSnapshot {
            content: None,
            size: before.len(),
            mtime_ns: metadata_mtime_ns(&before),
            skip_reason: Some(format!(
                "file size {} exceeds VCE limit {max_file_bytes}",
                before.len()
            )),
        });
    }
    let read_limit = max_file_bytes
        .checked_add(1)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| String::from("VCE source read limit exceeds platform capacity"))?;
    let mut bytes = Vec::with_capacity(usize::try_from(before.len()).unwrap_or(read_limit));
    File::open(&resolved)
        .and_then(|file| file.take(read_limit as u64).read_to_end(&mut bytes))
        .map_err(|error| format!("read source body `{relative_path}`: {error}"))?;
    let after = fs::metadata(&resolved)
        .map_err(|error| format!("re-read source metadata `{relative_path}`: {error}"))?;
    let before_mtime = metadata_mtime_ns(&before);
    let after_mtime = metadata_mtime_ns(&after);
    if before.len() != after.len()
        || before_mtime != after_mtime
        || bytes.len() as u64 != after.len()
    {
        return Err(format!(
            "source snapshot changed while reading `{relative_path}`"
        ));
    }
    if bytes.len() as u64 > max_file_bytes {
        return Ok(SourceSnapshot {
            content: None,
            size: bytes.len() as u64,
            mtime_ns: after_mtime,
            skip_reason: Some(format!(
                "file size {} exceeds VCE limit {max_file_bytes}",
                bytes.len()
            )),
        });
    }
    if bytes.contains(&0) {
        return Ok(SourceSnapshot {
            content: None,
            size: bytes.len() as u64,
            mtime_ns: after_mtime,
            skip_reason: Some(String::from("content contains a NUL byte and is binary")),
        });
    }
    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(error) => {
            return Ok(SourceSnapshot {
                content: None,
                size: error.as_bytes().len() as u64,
                mtime_ns: after_mtime,
                skip_reason: Some(String::from("content is not valid UTF-8")),
            });
        }
    };
    Ok(SourceSnapshot {
        size: content.len() as u64,
        content: Some(content),
        mtime_ns: after_mtime,
        skip_reason: None,
    })
}

/// Resolve one existing source path without permitting workspace escape.
/// 解析一个现存源码路径且不允许逃逸工作区。
///
/// # Parameters
/// 参数
/// `project_root` is the canonical workspace boundary.
/// `project_root` 是规范工作区边界。
/// `relative_path` is the requested relative source identity.
/// `relative_path` 是请求的相对源码身份。
///
/// # Returns
/// 返回值
/// The canonical in-root target, a missing marker, or an explicit rejection.
/// 返回规范根内目标、缺失标记或明确拒绝。
fn resolve_existing_source_path(
    project_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, SourcePathError> {
    let normalized = validate_relative_path(relative_path).map_err(SourcePathError::Rejected)?;
    if normalized != relative_path {
        return Err(SourcePathError::Rejected(format!(
            "source path `{relative_path}` is not canonical"
        )));
    }
    let joined = project_root.join(relative_path);
    let symlink_metadata = match fs::symlink_metadata(&joined) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(SourcePathError::Missing);
        }
        Err(error) => {
            return Err(SourcePathError::Rejected(format!(
                "inspect source path `{relative_path}`: {error}"
            )));
        }
    };
    let canonical = fs::canonicalize(&joined).map_err(|error| {
        SourcePathError::Rejected(format!(
            "canonicalize source path `{relative_path}`: {error}"
        ))
    })?;
    if !workspace_path_is_inside_root(&canonical, project_root) {
        return Err(SourcePathError::Rejected(format!(
            "source path `{relative_path}` escapes the project root"
        )));
    }
    if symlink_metadata.file_type().is_symlink() && canonical.is_dir() {
        return Err(SourcePathError::Rejected(format!(
            "source directory symlink `{relative_path}` is unsupported"
        )));
    }
    Ok(canonical)
}

/// Convert one walked path into a canonical VCE relative identity.
/// 将一个遍历路径转换为规范 VCE 相对身份。
///
/// # Parameters
/// 参数
/// `project_root` is the walker root.
/// `project_root` 是遍历根目录。
/// `path` is one descendant returned by the walker.
/// `path` 是遍历器返回的一个后代路径。
///
/// # Returns
/// 返回值
/// A slash-normalized relative path.
/// 返回以斜杠规范化的相对路径。
fn relative_path_text(project_root: &Path, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(project_root).map_err(|error| {
        format!(
            "VCE walker returned path outside project root `{}`: {error}",
            path.display()
        )
    })?;
    let text = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    validate_relative_path(&text)
}

/// Validate one cross-platform workspace-relative source identity.
/// 校验一个跨平台工作区相对源码身份。
///
/// # Parameters
/// 参数
/// `path` is the untrusted path identity.
/// `path` 是不可信路径身份。
///
/// # Returns
/// 返回值
/// The normalized path or an explicit security diagnostic.
/// 返回规范化路径或明确安全诊断。
fn validate_relative_path(path: &str) -> Result<String, String> {
    let normalized = normalize_relative_path(path)
        .map_err(|error| format!("invalid VCE source path `{path}`: {error}"))?;
    #[cfg(windows)]
    validate_windows_components(&normalized)?;
    Ok(normalized)
}

/// Reject Windows alternate streams, ambiguous names, and device components.
/// 拒绝 Windows 备用数据流、歧义名称与设备组件。
///
/// # Parameters
/// 参数
/// `path` is already slash-normalized and relative.
/// `path` 已经以斜杠规范化且为相对路径。
///
/// # Returns
/// 返回值
/// Success only for an unambiguous non-device path.
/// 仅对明确且非设备路径返回成功。
#[cfg(windows)]
fn validate_windows_components(path: &str) -> Result<(), String> {
    for component in path.split('/') {
        if component.contains(':') || component.ends_with(['.', ' ']) {
            return Err(format!(
                "VCE source path `{path}` contains an unsupported Windows component"
            ));
        }
        let stem = component
            .split_once('.')
            .map_or(component, |(stem, _)| stem)
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit()
                && stem.as_bytes()[3] != b'0');
        if reserved {
            return Err(format!(
                "VCE source path `{path}` contains a Windows device component"
            ));
        }
    }
    Ok(())
}

/// Return a stable nanosecond modification timestamp when the platform exposes one.
/// 在平台可提供时返回稳定的纳秒修改时间。
///
/// # Parameters
/// 参数
/// `metadata` is one filesystem metadata snapshot.
/// `metadata` 是一份文件系统元数据快照。
///
/// # Returns
/// 返回值
/// Nanoseconds since Unix epoch, or `None` when unavailable.
/// 返回 Unix epoch 以来的纳秒数，无法取得时返回 `None`。
fn metadata_mtime_ns(metadata: &Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos())
}

/// Return whether one walker entry is excluded by non-overridable VCE rules.
/// 返回一个遍历条目是否被不可覆盖的 VCE 规则排除。
///
/// # Parameters
/// 参数
/// `project_root` is the canonical walker root.
/// `project_root` 是规范遍历根目录。
/// `path` is the candidate walker entry.
/// `path` 是候选遍历条目。
///
/// # Returns
/// 返回值
/// `true` for dependency, generated, VCS, secret, or media paths.
/// 对依赖、生成物、版本控制、敏感或媒体路径返回 `true`。
fn is_hard_ignored_entry(project_root: &Path, path: &Path) -> bool {
    if path == project_root {
        return false;
    }
    path.strip_prefix(project_root)
        .ok()
        .and_then(Path::to_str)
        .map(|value| is_hard_ignored_relative_path(&value.replace('\\', "/")))
        .unwrap_or(true)
}

/// Return whether one relative path matches VCE non-overridable admission rules.
/// 返回一个相对路径是否匹配 VCE 不可覆盖的准入规则。
///
/// # Parameters
/// 参数
/// `path` is one slash-normalized relative path.
/// `path` 是一个以斜杠规范化的相对路径。
///
/// # Returns
/// 返回值
/// `true` when the path must never be sent to semantic indexing.
/// 当路径绝不能发送到语义索引时返回 `true`。
fn is_hard_ignored_relative_path(path: &str) -> bool {
    let mut components = path.split('/');
    let file_name = path.rsplit('/').next().unwrap_or(path);
    if components.any(|component| {
        matches!(
            component.to_ascii_lowercase().as_str(),
            ".git"
                | ".hg"
                | ".svn"
                | ".idea"
                | ".vscode"
                | ".cache"
                | ".next"
                | ".nuxt"
                | ".turbo"
                | "node_modules"
                | "target"
                | "dist"
                | "build"
                | "coverage"
                | "vendor"
                | "__pycache__"
        )
    }) {
        return true;
    }
    let lower_name = file_name.to_ascii_lowercase();
    if lower_name == ".env"
        || lower_name.starts_with(".env.")
        || matches!(
            lower_name.as_str(),
            "id_rsa" | "id_ed25519" | "credentials" | "credentials.json"
        )
    {
        return true;
    }
    matches!(
        Path::new(&lower_name)
            .extension()
            .and_then(|value| value.to_str()),
        Some(
            "7z" | "a"
                | "avi"
                | "bin"
                | "bmp"
                | "class"
                | "db"
                | "dll"
                | "dylib"
                | "exe"
                | "gif"
                | "gz"
                | "ico"
                | "jar"
                | "jpeg"
                | "jpg"
                | "key"
                | "lockb"
                | "mov"
                | "mp3"
                | "mp4"
                | "o"
                | "obj"
                | "otf"
                | "pdf"
                | "pem"
                | "png"
                | "pyc"
                | "rar"
                | "so"
                | "sqlite"
                | "sqlite3"
                | "tar"
                | "ttf"
                | "wav"
                | "webm"
                | "webp"
                | "woff"
                | "woff2"
                | "xz"
                | "zip"
        )
    )
}

/// Return whether a watcher path changes hierarchical ignore semantics.
/// 返回监听路径是否改变分层忽略语义。
///
/// # Parameters
/// 参数
/// `path` is one canonical relative watcher path.
/// `path` 是一个规范相对监听路径。
///
/// # Returns
/// 返回值
/// `true` for supported ignore-control files.
/// 对支持的 ignore 控制文件返回 `true`。
fn is_ignore_control_path(path: &str) -> bool {
    matches!(
        path.rsplit('/').next(),
        Some(".gitignore" | ".ignore" | ".oceignore")
    ) || path == ".git/info/exclude"
}

/// Build one source-read rejection without losing the operation identity.
/// 构建一条不会丢失操作身份的源码读取拒绝错误。
///
/// # Parameters
/// 参数
/// `reason` is the exact boundary violation.
/// `reason` 是精确的边界违规原因。
///
/// # Returns
/// 返回值
/// A typed VCE source-read rejection.
/// 返回类型化 VCE 源码读取拒绝错误。
fn rejected_source(reason: impl Into<String>) -> VcePortError {
    VcePortError::Rejected {
        operation: VcePortOperation::SourceRead,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Create one unique temporary project directory owned by a test.
    /// 创建一个由测试持有的唯一临时项目目录。
    ///
    /// # Parameters
    /// 参数
    /// `label` distinguishes concurrent test fixtures.
    /// `label` 用于区分并发测试夹具。
    ///
    /// # Returns
    /// 返回值
    /// A newly created project root.
    /// 返回新建的项目根目录。
    fn temp_project(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "vulcan-vce-source-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create temporary VCE project");
        root
    }

    /// Build test limits small enough to exercise every boundary.
    /// 构建足够小以覆盖全部边界的测试限制。
    ///
    /// # Returns
    /// 返回值
    /// A valid source-limit contract.
    /// 返回有效的源码限制契约。
    fn test_limits() -> VceSourceLimits {
        VceSourceLimits::new(128, 4, 256).expect("valid test source limits")
    }

    /// Full scans retain identities while excluding hard-ignored dependencies.
    /// 全量扫描保留身份，同时排除硬忽略依赖目录。
    #[tokio::test]
    async fn full_scan_builds_metadata_only_manifest() {
        let root = temp_project("manifest");
        fs::create_dir_all(root.join("src")).expect("create source directory");
        fs::create_dir_all(root.join("target")).expect("create target directory");
        fs::write(root.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n")
            .expect("write source fixture");
        fs::write(root.join("target/generated.rs"), "generated").expect("write ignored fixture");
        fs::write(root.join("image.bin"), [0, 1, 2]).expect("write binary fixture");

        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[])
            .expect("activate source adapter");
        let manifest = adapter.scan_full().await.expect("scan source manifest");

        assert_eq!(manifest.admitted_files, 1);
        assert_eq!(manifest.skipped_files, 0);
        assert_eq!(manifest.observations.len(), 1);
        assert_eq!(manifest.observations[0].path, "src/lib.rs");
        assert!(manifest.observations[0].content.is_none());
        assert!(manifest.observations[0].blob_name.is_some());
        fs::remove_dir_all(root).expect("remove source fixture");
    }

    /// Project ignore rules filter matching source paths without weakening mandatory exclusions.
    /// 项目忽略规则会过滤匹配源码路径，且不会削弱必需排除规则。
    #[tokio::test]
    async fn full_scan_applies_project_ignore_rules_after_hard_exclusions() {
        let root = temp_project("project-ignore");
        fs::create_dir_all(root.join("src/generated")).expect("create generated directory");
        fs::create_dir_all(root.join("target")).expect("create mandatory ignored directory");
        fs::write(root.join("src/lib.rs"), "pub fn retained() {}\n")
            .expect("write retained source fixture");
        fs::write(
            root.join("src/generated/bindings.rs"),
            "pub fn generated() {}\n",
        )
        .expect("write project ignored fixture");
        fs::write(root.join("target/attempt.rs"), "pub fn excluded() {}\n")
            .expect("write hard ignored fixture");

        let patterns = vec![String::from("src/generated/**"), String::from("!target/**")];
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &patterns)
            .expect("activate source adapter with project ignore rules");
        let manifest = adapter.scan_full().await.expect("scan source manifest");

        assert_eq!(manifest.admitted_files, 1);
        assert_eq!(manifest.observations.len(), 1);
        assert_eq!(manifest.observations[0].path, "src/lib.rs");
        fs::remove_dir_all(root).expect("remove source fixture");
    }

    /// Keep watcher path admission identical to full scans across hierarchical ignore sources.
    /// 在分层忽略规则来源之间保持监听路径准入与全量扫描完全一致。
    /// Takes no parameters; returns unit after comparing isolated full and incremental manifests.
    /// 无参数；隔离的全量与增量清单对比通过后返回空值。
    #[tokio::test]
    async fn path_scan_preserves_hierarchical_ignore_admission() {
        // root contains only synthetic sources and local ignore rules, without provider calls.
        // root 仅包含合成源码与本地忽略规则，不调用供应商。
        let root = temp_project("delta-ignore");
        fs::create_dir_all(root.join(".git/info")).unwrap();
        fs::create_dir_all(root.join("src/nested")).unwrap();
        fs::write(root.join(".gitignore"), "git-hidden.rs\n").unwrap();
        fs::write(root.join(".ignore"), "ignore-hidden.rs\n").unwrap();
        fs::write(root.join(".oceignore"), "oce-hidden.rs\n").unwrap();
        fs::write(root.join(".git/info/exclude"), "exclude-hidden.rs\n").unwrap();
        fs::write(
            root.join("src/.gitignore"),
            "nested/*\n!nested/visible.rs\n",
        )
        .unwrap();
        // paths represent raw watcher events, including files that must never reach embedding.
        // paths 表示原始监听事件，包含绝不能进入向量处理的文件。
        let paths = BTreeSet::from([
            String::from("git-hidden.rs"),
            String::from("ignore-hidden.rs"),
            String::from("oce-hidden.rs"),
            String::from("exclude-hidden.rs"),
            String::from("src/nested/hidden.rs"),
            String::from("src/nested/visible.rs"),
        ]);
        for path in &paths {
            fs::write(root.join(path), "fn synthetic() {}\n").unwrap();
        }
        fs::create_dir_all(root.join("unrelated/deep")).unwrap();
        fs::write(
            root.join("unrelated/deep/untouched.rs"),
            "fn untouched() {}\n",
        )
        .unwrap();
        // adapter applies the same host-owned source contract to both scan entry points.
        // adapter 将同一个宿主持有源码契约应用于两个扫描入口。
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[]).unwrap();
        let full = adapter.scan_full().await.unwrap();
        let delta = adapter.scan_paths(paths.clone()).await.unwrap();
        // admitted proves the path pass does not return unrelated source branches.
        // admitted 证明路径准入阶段不会返回无关源码分支。
        let admitted = adapter
            .admitted_changed_paths(&paths, &VceRefreshControl::default())
            .unwrap();
        // expected excludes ignore control documents not present in this watcher batch.
        // expected 排除未出现在此监听批次中的忽略控制文档。
        let expected = full
            .observations
            .into_iter()
            .filter(|observation| paths.contains(&observation.path))
            .collect::<Vec<_>>();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(expected.len(), 1);
        assert_eq!(expected[0].path, "src/nested/visible.rs");
        assert_eq!(
            admitted,
            HashSet::from([String::from("src/nested/visible.rs")])
        );
        assert_eq!(delta.observations, expected);
        assert_eq!(delta.removed_paths.len(), 5);
        assert!(!delta.requires_full_refresh);
    }

    /// Verify a cancellation observed during scanning stops the joined worker before later files.
    /// 验证扫描期间观察到的取消在后续文件之前停止已汇合的工作线程。
    ///
    /// Takes no parameters and returns nothing; all files and observations belong to a temporary fixture.
    /// 无参数、无返回值；全部文件和观察属于临时夹具。
    #[tokio::test]
    async fn full_scan_reports_real_progress_and_cooperatively_cancels() {
        // root is isolated and unrelated to the user's workspace or existing indexes.
        // root 与用户工作区和现有索引隔离。
        let root = temp_project("scan-cancel");
        for number in 0..16 {
            fs::write(root.join(format!("source-{number}.rs")), "fn source() {}\n")
                .expect("write bounded scan fixture");
        }
        // adapter freezes this fixture's root and existing source limits.
        // adapter 冻结此夹具根与现有源码限制。
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[])
            .expect("activate scan fixture");
        // events records only real completions from the blocking worker.
        // events 仅记录阻塞工作线程的真实完成量。
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        // control and requested share cancellation without creating an observer ownership cycle.
        // control 与 requested 共享取消且不形成观察器归属循环。
        let control = VceRefreshControl::default();
        let requested = control.clone();
        let control = control.with_observer(Arc::new(move |progress: VceRefreshProgress| {
            if progress.completed == 1 {
                assert_eq!(
                    requested.cancel(),
                    vulcan_context_engine::VceCancelOutcome::Accepted
                );
            }
            sink.lock().unwrap().push(progress);
        }));
        let error = adapter
            .scan_full_observed(control, String::from("scan-job-1"))
            .await
            .expect_err("scan must acknowledge cancellation");
        assert_eq!(error, super::super::manager::VCE_REFRESH_CANCELLED_ERROR);
        // observed is read after worker completion; no detached traversal remains active.
        // observed 在工作线程完成后读取；不存在仍在活动的脱离任务遍历。
        let observed = events.lock().unwrap();
        assert_eq!(observed.last().unwrap().completed, 1);
        assert!(
            observed
                .iter()
                .all(|progress| progress.total.is_none() && progress.operation_id == "scan-job-1")
        );
        fs::remove_dir_all(root).expect("remove isolated scan fixture");
    }

    /// Deferred reads return exactly the manifest version requested by VCE.
    /// 延迟读取精确返回 VCE 请求的清单版本。
    #[tokio::test]
    async fn deferred_read_returns_exact_manifest_version() {
        let root = temp_project("read");
        fs::write(root.join("lib.rs"), "fn main() {}\n").expect("write source fixture");
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[])
            .expect("activate source adapter");
        let manifest = adapter.scan_full().await.expect("scan source manifest");
        let observation = &manifest.observations[0];
        let response = adapter
            .read_sources(VceSourceReadRequest {
                operation_id: String::from("read-1"),
                expectations: vec![vulcan_context_engine::VceSourceExpectation {
                    path: observation.path.clone(),
                    blob_name: observation.blob_name.clone().expect("Blob identity"),
                    size: observation.size,
                    mtime_ns: observation.mtime_ns,
                }],
            })
            .await
            .expect("read exact source");

        assert_eq!(response.operation_id, "read-1");
        assert_eq!(response.documents[0].content, "fn main() {}\n");
        fs::remove_dir_all(root).expect("remove source fixture");
    }

    /// Deferred reads reject content changed after manifest creation.
    /// 延迟读取拒绝清单创建后发生变化的内容。
    #[tokio::test]
    async fn deferred_read_rejects_stale_manifest() {
        let root = temp_project("stale");
        fs::write(root.join("lib.rs"), "fn old() {}\n").expect("write source fixture");
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[])
            .expect("activate source adapter");
        let manifest = adapter.scan_full().await.expect("scan source manifest");
        let observation = &manifest.observations[0];
        fs::write(root.join("lib.rs"), "fn new() {}\n").expect("change source fixture");

        let error = adapter
            .read_sources(VceSourceReadRequest {
                operation_id: String::from("read-stale"),
                expectations: vec![vulcan_context_engine::VceSourceExpectation {
                    path: observation.path.clone(),
                    blob_name: observation.blob_name.clone().expect("Blob identity"),
                    size: observation.size,
                    mtime_ns: observation.mtime_ns,
                }],
            })
            .await
            .expect_err("stale source must fail");

        assert!(matches!(error, VcePortError::Rejected { .. }));
        fs::remove_dir_all(root).expect("remove source fixture");
    }

    /// Path-scoped scans reject traversal before touching the filesystem.
    /// 路径级扫描在接触文件系统前拒绝目录穿越。
    #[tokio::test]
    async fn path_scan_rejects_parent_traversal() {
        let root = temp_project("traversal");
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[])
            .expect("activate source adapter");
        let error = adapter
            .scan_paths(BTreeSet::from([String::from("../outside.rs")]))
            .await
            .expect_err("parent traversal must fail");

        assert!(error.contains("parent traversal"));
        fs::remove_dir_all(root).expect("remove source fixture");
    }

    /// Full scans preserve explicit skipped records for oversized and binary source-like files.
    /// 全量扫描会为超大与二进制源码样式文件保留明确的跳过记录。
    #[tokio::test]
    async fn full_scan_records_oversized_and_binary_sources_as_skipped() {
        let root = temp_project("skipped-sources");
        fs::write(root.join("oversized.rs"), vec![b'x'; 129])
            .expect("write oversized source fixture");
        fs::write(root.join("binary.rs"), [b'f', b'n', 0, b'x'])
            .expect("write binary source fixture");
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[])
            .expect("activate source adapter");

        let manifest = adapter.scan_full().await.expect("scan skipped sources");

        assert_eq!(manifest.admitted_files, 0);
        assert_eq!(manifest.skipped_files, 2);
        assert_eq!(manifest.observations.len(), 2);
        assert!(
            manifest
                .observations
                .iter()
                .all(|item| item.content.is_none())
        );
        assert!(
            manifest
                .observations
                .iter()
                .all(|item| item.skip_reason.is_some())
        );
        fs::remove_dir_all(root).expect("remove source fixture");
    }

    /// Path scans model a filesystem rename as one removal plus one identified observation.
    /// 路径扫描将文件系统重命名建模为一项删除与一项已识别观察。
    #[tokio::test]
    async fn path_scan_models_rename_without_retaining_the_old_path() {
        let root = temp_project("rename");
        fs::write(root.join("old.rs"), "fn renamed() {}\n").expect("write old source fixture");
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[])
            .expect("activate source adapter");
        fs::rename(root.join("old.rs"), root.join("new.rs")).expect("rename source fixture");

        let delta = adapter
            .scan_paths(BTreeSet::from([
                String::from("new.rs"),
                String::from("old.rs"),
            ]))
            .await
            .expect("scan rename delta");

        assert_eq!(delta.removed_paths, vec![String::from("old.rs")]);
        assert_eq!(delta.observations.len(), 1);
        assert_eq!(delta.observations[0].path, "new.rs");
        fs::remove_dir_all(root).expect("remove source fixture");
    }

    /// Explicit watcher paths reject a linked directory whose canonical target escapes the project.
    /// 显式监听路径会拒绝规范目标逃逸项目根目录的链接目录。
    #[tokio::test]
    async fn path_scan_rejects_external_file_symlink() {
        let root = temp_project("symlink-root");
        let outside = temp_project("symlink-outside");
        let outside_file = outside.join("outside.rs");
        fs::write(&outside_file, "fn outside() {}\n").expect("write external source fixture");
        let link = root.join("linked");
        #[cfg(windows)]
        {
            // Directory junctions exercise canonical escape handling without requiring symlink privileges.
            // 目录联接无需符号链接特权即可验证规范路径逃逸处理。
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&outside)
                .status()
                .expect("launch junction creation");
            assert!(
                status.success(),
                "create external source directory junction"
            );
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link)
            .expect("create external source directory symlink");
        let adapter = VceProjectSourceAdapter::activate(&root, test_limits(), &[])
            .expect("activate source adapter");

        let error = adapter
            .scan_paths(BTreeSet::from([String::from("linked/outside.rs")]))
            .await
            .expect_err("external file symlink must be rejected");

        assert!(error.contains("escapes the project root"));
        fs::remove_dir(&link).expect("remove external source link");
        fs::remove_dir_all(root).expect("remove source fixture");
        fs::remove_dir_all(outside).expect("remove external fixture");
    }
}
