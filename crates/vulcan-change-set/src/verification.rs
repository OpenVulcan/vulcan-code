use crate::{
    ChangeSetFileChangeKind, ChangeSetMode, ChangeSetVerificationStatus,
    ChangeSetVerificationSummary, RawChangeSetFileRecord, RawChangeSetHunk, normalize_text,
    split_normalized_lines,
};
use std::fs;
use std::path::Path;

/// Verifies one raw file-record set against the current filesystem and returns one host-side warning snapshot.
/// 对照当前文件系统校验一组原始文件记录，并返回一份宿主侧警告快照。
pub(super) fn verify_change_set_records(
    mode: ChangeSetMode,
    files: &[RawChangeSetFileRecord],
) -> ChangeSetVerificationSummary {
    if mode == ChangeSetMode::Preview {
        return ChangeSetVerificationSummary {
            status: ChangeSetVerificationStatus::Skipped,
            checked_file_count: 0,
            warnings: Vec::new(),
        };
    }

    let mut checked_file_count = 0_usize;
    let mut warnings = Vec::new();
    for file in files {
        match file.change {
            ChangeSetFileChangeKind::Create => {
                if let Some(path) = file.path.as_deref() {
                    checked_file_count += 1;
                    verify_created_file(path, file.content.as_deref(), &mut warnings);
                }
            }
            ChangeSetFileChangeKind::Modify => {
                if let Some(path) = file.path.as_deref() {
                    checked_file_count += 1;
                    verify_modified_file(path, &file.hunks, &mut warnings);
                }
            }
            ChangeSetFileChangeKind::Delete => {
                if let Some(path) = file.path.as_deref() {
                    checked_file_count += 1;
                    verify_deleted_file(path, &mut warnings);
                }
            }
            ChangeSetFileChangeKind::Rename => {
                if let (Some(old_path), Some(new_path)) =
                    (file.old_path.as_deref(), file.new_path.as_deref())
                {
                    checked_file_count += 1;
                    verify_renamed_file(old_path, new_path, &mut warnings);
                }
            }
        }
    }

    let status = if warnings.is_empty() {
        ChangeSetVerificationStatus::Verified
    } else {
        ChangeSetVerificationStatus::Warning
    };
    ChangeSetVerificationSummary {
        status,
        checked_file_count,
        warnings,
    }
}

/// Verifies one created file against the expected on-disk state.
/// 对照预期落盘状态校验一份创建文件。
fn verify_created_file(path: &str, expected_content: Option<&str>, warnings: &mut Vec<String>) {
    let file_path = Path::new(path);
    if !file_path.exists() {
        warnings.push(format!("Created file is missing: {}", file_path.display()));
        return;
    }
    let Some(expected_content) = expected_content else {
        return;
    };
    match fs::read(file_path) {
        Ok(actual_bytes) => {
            if !file_bytes_match_expected_text(&actual_bytes, expected_content) {
                warnings.push(format!(
                    "Created file content does not match the reported change_set payload: {}",
                    file_path.display()
                ));
            }
        }
        Err(error) => warnings.push(format!(
            "Created file exists but could not be read for verification ({}): {}",
            file_path.display(),
            error
        )),
    }
}

/// Verifies one deleted file against the expected absence on disk.
/// 对照预期的磁盘缺失状态校验一份删除文件。
fn verify_deleted_file(path: &str, warnings: &mut Vec<String>) {
    let file_path = Path::new(path);
    if file_path.exists() {
        warnings.push(format!(
            "Deleted file still exists on disk: {}",
            file_path.display()
        ));
    }
}

/// Verifies one rename record against the expected old/new path state on disk.
/// 对照预期旧路径和新路径状态校验一条重命名记录。
fn verify_renamed_file(old_path: &str, new_path: &str, warnings: &mut Vec<String>) {
    let old_file = Path::new(old_path);
    let new_file = Path::new(new_path);
    if old_file.exists() {
        warnings.push(format!(
            "Renamed source path still exists on disk: {}",
            old_file.display()
        ));
    }
    if !new_file.exists() {
        warnings.push(format!(
            "Renamed destination path is missing on disk: {}",
            new_file.display()
        ));
    }
}

/// Verifies one modified file using exact inserted-line checks plus best-effort surrounding-context checks.
/// 通过精确插入行校验和尽力而为的上下文校验来验证一份修改文件。
fn verify_modified_file(path: &str, hunks: &[RawChangeSetHunk], warnings: &mut Vec<String>) {
    let file_path = Path::new(path);
    if !file_path.exists() {
        warnings.push(format!("Modified file is missing: {}", file_path.display()));
        return;
    }
    let actual_bytes = match fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            warnings.push(format!(
                "Modified file could not be read for verification ({}): {}",
                file_path.display(),
                error
            ));
            return;
        }
    };
    let actual_text = match std::str::from_utf8(&actual_bytes) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!(
                "Modified file could not be decoded as UTF-8 for hunk verification ({}): {}",
                file_path.display(),
                error
            ));
            return;
        }
    };
    let normalized_text = normalize_text(actual_text);
    let normalized_lines = split_normalized_lines(&normalized_text);

    for (hunk_index, hunk) in hunks.iter().enumerate() {
        if !hunk.before.trim().is_empty()
            && !normalized_text.contains(normalize_text(&hunk.before).as_str())
        {
            warnings.push(format!(
                "Modify hunk {} is missing its reported leading context in {}",
                hunk_index + 1,
                file_path.display()
            ));
        }
        if !hunk.after.trim().is_empty()
            && !normalized_text.contains(normalize_text(&hunk.after).as_str())
        {
            warnings.push(format!(
                "Modify hunk {} is missing its reported trailing context in {}",
                hunk_index + 1,
                file_path.display()
            ));
        }
        for inserted in &hunk.inserted {
            let actual_line = normalized_lines
                .get(inserted.line.saturating_sub(1))
                .map(String::as_str)
                .unwrap_or_default();
            if actual_line != normalize_text(&inserted.content) {
                warnings.push(format!(
                    "Modify hunk {} inserted line {} does not match the current file content in {}",
                    hunk_index + 1,
                    inserted.line,
                    file_path.display()
                ));
            }
        }
    }
}

/// Returns whether raw file bytes match the expected text exactly or after newline normalization.
/// 返回原始文件字节是否与期望文本完全一致，或在换行规范化后保持一致。
fn file_bytes_match_expected_text(actual_bytes: &[u8], expected_text: &str) -> bool {
    if actual_bytes == expected_text.as_bytes() {
        return true;
    }
    std::str::from_utf8(actual_bytes)
        .map(|text| normalize_text(text) == normalize_text(expected_text))
        .unwrap_or(false)
}
