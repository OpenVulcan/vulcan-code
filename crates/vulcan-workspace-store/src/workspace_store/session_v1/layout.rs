//! Exact path layout of one V1 session directory.
//! 单个 V1 会话目录的精确路径布局。
//!
//! The unique path function is `root + relative_path`; no runtime code ever
//! guesses a path from tags, dates, or uuids and no loader scans for
//! candidates.
//! 唯一路径函数是 `root + relative_path`；运行时代码绝不根据标签、日期或
//! uuid 猜测路径，加载器也绝不扫描候选目录。

use std::fmt;
use std::path::PathBuf;

/// Subdirectory of the segments tree inside one session directory.
/// 会话目录内分片树的子目录名。
const SEGMENTS_DIR: &str = "segments";

/// Subdirectory of immutable context generations inside one session.
/// 单个会话内不可变上下文 generation 的子目录。
const CONTEXT_DIR: &str = "context";

/// File name of the metadata document.
/// 元数据文档文件名。
pub const METADATA_FILE: &str = "metadata.json";

/// File name of the runtime lock document.
/// 运行时锁定文档文件名。
pub const RUNTIME_FILE: &str = "runtime.json";

/// File name of the compactions document.
/// 压缩节点文档文件名。
pub const COMPACTIONS_FILE: &str = "compactions.json";

/// First bank suffix of an immutable active-tail candidate.
/// 不可变活动尾段候选的第一个 bank 后缀。
const SEGMENT_BANK_A: &str = "a";

/// Second bank suffix of an immutable active-tail candidate.
/// 不可变活动尾段候选的第二个 bank 后缀。
const SEGMENT_BANK_B: &str = "b";

/// Exact locator of one V1 session directory.
/// 一个 V1 会话目录的精确定位器。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionV1Layout {
    /// Absolute storage root of session directories.
    /// 会话目录的绝对存储根。
    pub root: PathBuf,
    /// Project partition subdirectory.
    /// 项目分区子目录。
    pub project_partition: String,
    /// Human-readable date bucket used by the directory name.
    /// 目录名使用的人类可读日期桶。
    pub date_bucket: String,
    /// Cross-system session uuid.
    /// 跨系统会话 uuid。
    pub session_id: String,
}

impl SessionV1Layout {
    /// Builds one layout from its exact locator parts.
    /// 从精确定位器部件构建一个布局。
    pub fn new(
        root: impl Into<PathBuf>,
        project_partition: impl Into<String>,
        date_bucket: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            project_partition: project_partition.into(),
            date_bucket: date_bucket.into(),
            session_id: session_id.into(),
        }
    }

    /// Returns the canonical relative path `{partition}/{date_bucket}-{session_id}`.
    /// 返回规范相对路径 `{partition}/{date_bucket}-{session_id}`。
    pub fn relative_path(&self) -> String {
        format!(
            "{}/{}-{}",
            self.project_partition, self.date_bucket, self.session_id
        )
    }

    /// Returns the absolute session directory path.
    /// 返回绝对会话目录路径。
    pub fn session_dir(&self) -> PathBuf {
        self.root.join(self.relative_path())
    }

    /// Returns the absolute metadata path.
    /// 返回绝对 metadata 路径。
    pub fn metadata_path(&self) -> PathBuf {
        self.session_dir().join(METADATA_FILE)
    }

    /// Returns the absolute context generation directory path.
    /// 返回绝对上下文 generation 目录路径。
    pub fn context_generation_dir(&self, generation: u64) -> PathBuf {
        self.session_dir()
            .join(CONTEXT_DIR)
            .join(format!("c{:07}", generation))
    }

    /// Returns the runtime path of one immutable context generation.
    /// 返回一个不可变上下文 generation 的 runtime 路径。
    pub fn runtime_path(&self, generation: u64) -> PathBuf {
        self.context_generation_dir(generation).join(RUNTIME_FILE)
    }

    /// Returns the compactions path of one immutable context generation.
    /// 返回一个不可变上下文 generation 的 compactions 路径。
    pub fn compactions_path(&self, generation: u64) -> PathBuf {
        self.context_generation_dir(generation)
            .join(COMPACTIONS_FILE)
    }

    /// Returns the absolute generation directory path.
    /// 返回绝对 generation 目录路径。
    pub fn generation_dir(&self, generation: u64) -> PathBuf {
        self.session_dir()
            .join(SEGMENTS_DIR)
            .join(format!("g{:07}", generation))
    }

    /// Returns the absolute segment file path in one generation.
    /// 返回某个 generation 内分片文件的绝对路径。
    pub fn segment_path(&self, generation: u64, segment_number: u64) -> PathBuf {
        self.segment_entry_path(generation, &Self::segment_file_name(segment_number))
    }

    /// Returns the absolute path of one metadata-selected segment file.
    /// 返回 metadata 选中的一个分片文件绝对路径。
    pub fn segment_entry_path(&self, generation: u64, file_name: &str) -> PathBuf {
        self.generation_dir(generation).join(file_name)
    }

    /// Returns the fixed candidate generation directory used by history edits
    /// (`current_generation + 1` only).
    /// 历史编辑使用的固定候选 generation 目录(仅 `current_generation + 1`)。
    pub fn candidate_generation_dir(&self, current_generation: u64) -> PathBuf {
        self.generation_dir(current_generation + 1)
    }

    /// Returns the fixed candidate context directory (`current + 1`).
    /// 返回固定候选上下文目录(`current + 1`)。
    pub fn candidate_context_dir(&self, current_generation: u64) -> PathBuf {
        self.context_generation_dir(current_generation + 1)
    }

    /// Returns the provider API trace directory under `temp`.
    /// 返回 `temp` 下的 Provider API 追踪目录。
    pub fn trace_dir(&self) -> PathBuf {
        self.session_dir().join("temp").join("provider-api-trace")
    }

    /// Converts one segment number to its initial banked file name.
    /// 把一个分片序号转换为初始 bank 文件名。
    pub fn segment_file_name(segment_number: u64) -> String {
        format!("s{segment_number:07}-{SEGMENT_BANK_A}.jsonl")
    }

    /// Returns the unpublished opposite bank for one published segment entry.
    /// 返回一个已发布分片条目的未发布对侧 bank。
    pub fn alternate_segment_file_name(
        segment_number: u64,
        current_file_name: &str,
    ) -> Option<String> {
        let bank_a = Self::segment_file_name(segment_number);
        let bank_b = format!("s{segment_number:07}-{SEGMENT_BANK_B}.jsonl");
        if current_file_name == bank_a {
            Some(bank_b)
        } else if current_file_name == bank_b {
            Some(bank_a)
        } else {
            None
        }
    }

    /// Validates that one file name is an exact banked name for the segment.
    /// 校验一个文件名是该分片的精确 bank 文件名。
    pub fn is_segment_file_name(segment_number: u64, file_name: &str) -> bool {
        Self::alternate_segment_file_name(segment_number, file_name).is_some()
    }
}

impl fmt::Display for SessionV1Layout {
    /// Formats the layout as its canonical relative path.
    /// 把布局格式化为其规范相对路径。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.relative_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical path and file names are exact and stable.
    /// 规范路径与文件名精确且稳定。
    #[test]
    fn layout_paths_are_exact() {
        let layout =
            SessionV1Layout::new("/root/code/sessions", "project-1", "2026-08-26", "abc-123");
        assert_eq!(layout.relative_path(), "project-1/2026-08-26-abc-123");
        assert_eq!(
            layout.session_dir(),
            PathBuf::from("/root/code/sessions/project-1/2026-08-26-abc-123")
        );
        assert_eq!(
            layout.segment_path(1, 1),
            PathBuf::from(
                "/root/code/sessions/project-1/2026-08-26-abc-123/segments/g0000001/s0000001-a.jsonl"
            )
        );
        assert_eq!(layout.candidate_generation_dir(2), layout.generation_dir(3));
        assert_eq!(
            layout.runtime_path(1),
            PathBuf::from(
                "/root/code/sessions/project-1/2026-08-26-abc-123/context/c0000001/runtime.json"
            )
        );
        assert_eq!(
            layout.compactions_path(1),
            PathBuf::from(
                "/root/code/sessions/project-1/2026-08-26-abc-123/context/c0000001/compactions.json"
            )
        );
        assert_eq!(
            layout.candidate_context_dir(2),
            layout.context_generation_dir(3)
        );
    }

    /// Banked segment names alternate exactly without directory scanning.
    /// bank 分片名称在不扫描目录的前提下精确交替。
    #[test]
    fn banked_segment_name_alternation_is_exact() {
        assert_eq!(
            SessionV1Layout::alternate_segment_file_name(7, "s0000007-a.jsonl"),
            Some(String::from("s0000007-b.jsonl"))
        );
        assert!(SessionV1Layout::is_segment_file_name(7, "s0000007-b.jsonl"));
        assert!(!SessionV1Layout::is_segment_file_name(7, "s0000007.jsonl"));
        assert!(!SessionV1Layout::is_segment_file_name(7, "metadata.json"));
    }
}
