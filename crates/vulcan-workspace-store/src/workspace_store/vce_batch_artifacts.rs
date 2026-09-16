//! Exact VCE Batch files retained only for opt-in Provider diagnostics.
//! 仅为自愿开启的 Provider 诊断保留精确 VCE Batch 文件。

use super::session_v1::atomic_io::write_file_atomic;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

/// Serializes immutable artifact publication within this process.
/// 在当前进程内串行化不可变产物的发布。
static VCE_BATCH_ARTIFACT_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Saves one complete Batch file beside the VCE data directory under `temp/vce`.
/// 将一份完整 Batch 文件保存到 VCE 数据目录同级的 `temp/vce` 下。
///
/// `data_root` is the application data directory, `project_id` and `task_id` select the
/// exact VCE task, `file_name` is one of the closed artifact names, and `bytes` are unchanged.
/// `data_root` 是应用数据目录，`project_id` 和 `task_id` 选择精确 VCE 任务，
/// `file_name` 是封闭集合内的产物名，`bytes` 保持原样。
///
/// Returns the published file path or a precise storage error.
/// 返回已发布文件路径或明确的存储错误。
pub fn persist_vce_batch_artifact(
    data_root: &Path,
    project_id: &str,
    task_id: &str,
    file_name: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    if project_id.trim().is_empty() {
        return Err(String::from("VCE Batch project identity is empty"));
    }
    // The provider planner emits `batch-` plus a lowercase 64-character SHA-256 digest.
    // 供应商规划器生成 `batch-` 前缀与 64 字符的小写 SHA-256 摘要。
    if task_id.len() != 70
        || !task_id.starts_with("batch-")
        || !task_id[6..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(String::from(
            "VCE Batch task identity is not a deterministic task id",
        ));
    }
    // Terminal status files use their content digest so distinct final observations cannot overwrite.
    // 终态文件使用内容摘要，避免不同的最终观测相互覆盖。
    let terminal_status_digest = file_name
        .strip_prefix("status-")
        .and_then(|value| value.strip_suffix(".json"));
    if !terminal_status_digest.is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }) && !matches!(
        file_name,
        "request.jsonl" | "request.json" | "output.jsonl" | "output.json" | "error.jsonl"
    ) {
        return Err(String::from("VCE Batch artifact name is unsupported"));
    }
    if let Some(digest) = terminal_status_digest
        && hex::encode(Sha256::digest(bytes)) != digest
    {
        return Err(String::from(
            "VCE Batch terminal status filename does not match its bytes",
        ));
    }
    // Serialize the existence check and atomic publication within the process.
    // 在进程内串行化存在性检查与原子发布。
    let _guard = VCE_BATCH_ARTIFACT_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| String::from("VCE Batch artifact write lock is poisoned"))?;
    // Anchor every diagnostic child to the existing resolved application data root.
    // 将所有诊断子路径锚定到现有且已解析的应用数据根目录。
    let canonical_root = fs::canonicalize(data_root)
        .map_err(|error| format!("Resolve VCE Batch application data root: {error}"))?;
    verify_regular_directory(&canonical_root)?;
    // Hash the full project identity to keep the path private and injection free.
    // 对完整项目身份取哈希，使路径保持私有并避免路径注入。
    let project_key = hex::encode(Sha256::digest(project_id.as_bytes()));
    // Keep the temporary tree beside the authoritative `vce` tree.
    // 将临时树放在权威 `vce` 树的同级。
    let mut directory = canonical_root;
    for component in ["temp", "vce", project_key.as_str(), task_id] {
        directory.push(component);
        ensure_regular_directory(&directory)?;
    }
    // The closed file-name set prevents caller-controlled path traversal.
    // 封闭文件名集合阻止调用方控制路径穿越。
    let path = directory.join(file_name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata_is_link(&metadata) || !metadata.is_file() {
                return Err(format!(
                    "VCE Batch artifact is not a regular file: {}",
                    path.display()
                ));
            }
            if existing_artifact_matches(&path, bytes)? {
                return Ok(path);
            }
            return Err(format!(
                "VCE Batch artifact differs from its existing bytes: {}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Inspect VCE Batch artifact {}: {error}",
                path.display()
            ));
        }
    }
    write_file_atomic(&path, bytes, "VCE Batch artifact")
        .map_err(|error| format!("Persist VCE Batch artifact {}: {error}", path.display()))?;
    Ok(path)
}

/// Creates one owned path component and rejects links or non-directories.
/// 创建一个受控路径组件，并拒绝链接及非目录。
fn ensure_regular_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => verify_directory_metadata(path, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| {
                format!(
                    "Create VCE Batch artifact directory {}: {error}",
                    path.display()
                )
            })?;
            verify_regular_directory(path)
        }
        Err(error) => Err(format!(
            "Inspect VCE Batch artifact directory {}: {error}",
            path.display()
        )),
    }
}

/// Validates one existing owned directory without following a link.
/// 在不跟随链接的前提下校验一个现有受控目录。
fn verify_regular_directory(path: &Path) -> Result<(), String> {
    // Inspect the directory entry itself instead of following a link.
    // 检查目录项本身，而不跟随链接。
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Inspect VCE Batch artifact directory {}: {error}",
            path.display()
        )
    })?;
    verify_directory_metadata(path, &metadata)
}

/// Checks the file type and platform link marker of one directory.
/// 检查一个目录的文件类型和平台链接标记。
fn verify_directory_metadata(path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    if metadata_is_link(metadata) || !metadata.is_dir() {
        return Err(format!(
            "VCE Batch artifact path is not a regular directory: {}",
            path.display()
        ));
    }
    Ok(())
}

/// Detects symbolic links and Windows reparse points before writing diagnostic data.
/// 在写入诊断数据前识别符号链接和 Windows 重解析点。
fn metadata_is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Compares a retained artifact with the exact current bytes using bounded read memory.
/// 使用有界读取内存对比已保留产物与当前精确字节。
fn existing_artifact_matches(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    // Read a fixed-size window to avoid a second full artifact allocation.
    // 使用固定大小窗口读取，避免再次分配完整产物内存。
    let mut file = File::open(path)
        .map_err(|error| format!("Open VCE Batch artifact {}: {error}", path.display()))?;
    // Fixed comparison window independent of the provider file size.
    // 与供应商文件大小无关的固定对比窗口。
    let mut chunk = [0_u8; 8 * 1024];
    // Current byte offset into the exact new artifact.
    // 当前精确新产物的字节偏移。
    let mut offset = 0_usize;
    loop {
        // Compare every existing byte before accepting an idempotent replay.
        // 接受幂等重放之前逐字节比较所有已存内容。
        let read = file
            .read(&mut chunk)
            .map_err(|error| format!("Read VCE Batch artifact {}: {error}", path.display()))?;
        if read == 0 {
            return Ok(offset == bytes.len());
        }
        if bytes.get(offset..offset.saturating_add(read)) != Some(&chunk[..read]) {
            return Ok(false);
        }
        offset += read;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies exact bytes, task isolation, and refusal to replace forensic evidence.
    /// 验证精确字节、任务隔离以及拒绝覆盖取证产物。
    #[test]
    fn exact_batch_artifacts_are_immutable_and_project_scoped() {
        let root = tempfile::tempdir().expect("temporary data root");
        let task_id = format!("batch-{}", "a".repeat(64));
        let path = persist_vce_batch_artifact(
            root.path(),
            "project-A",
            &task_id,
            "request.jsonl",
            b"{\"input\":\"text\"}\n",
        )
        .expect("persist exact request");
        assert_eq!(
            fs::read(&path).expect("read request"),
            b"{\"input\":\"text\"}\n"
        );
        assert_eq!(
            path.parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str()),
            Some(task_id.as_str())
        );
        let canonical_root = fs::canonicalize(root.path()).expect("canonical data root");
        assert!(path.starts_with(canonical_root.join("temp").join("vce")));
        assert!(
            persist_vce_batch_artifact(
                root.path(),
                "project-A",
                &task_id,
                "request.jsonl",
                b"{\"input\":\"text\"}\n"
            )
            .is_ok()
        );
        assert!(
            persist_vce_batch_artifact(
                root.path(),
                "project-A",
                &task_id,
                "request.jsonl",
                b"different"
            )
            .is_err()
        );
        let output = persist_vce_batch_artifact(
            root.path(),
            "project-A",
            &task_id,
            "output.jsonl",
            b"{\"response\":1}\n",
        )
        .expect("persist complete output");
        assert_eq!(
            fs::read(output).expect("read output"),
            b"{\"response\":1}\n"
        );
        let status_bytes = b"{\"status\":\"failed\"}";
        let status_name = format!("status-{}.json", hex::encode(Sha256::digest(status_bytes)));
        let status = persist_vce_batch_artifact(
            root.path(),
            "project-A",
            &task_id,
            &status_name,
            status_bytes,
        )
        .expect("persist terminal status");
        assert_eq!(fs::read(status).expect("read status"), status_bytes);
        let other = persist_vce_batch_artifact(
            root.path(),
            "project-B",
            &task_id,
            "error.jsonl",
            b"error\n",
        )
        .expect("persist other project error");
        assert_eq!(fs::read(&other).expect("read error"), b"error\n");
        assert_ne!(other.parent(), path.parent());
    }
}
