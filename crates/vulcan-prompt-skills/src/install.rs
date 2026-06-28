//! Host-managed system prompt skill installation helpers.
//! 宿主管理系统提示词技能安装辅助逻辑。

use include_dir::Dir;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::{
    PromptSkillLoadError, PromptSkillsGlobalConfig, SKILL_FILE_NAME,
    SystemPromptSkillsInstallReport, configured_system_skill_root,
};

/// Embedded VulcanCode-adapted system prompt skills.
/// 已内嵌且适配 VulcanCode 的系统提示词技能。
const SYSTEM_SKILLS_DIR: Dir = include_dir::include_dir!("$CARGO_MANIFEST_DIR/src/assets/system");

/// Marker file used to skip unchanged system skill installs.
/// 用于跳过未变化系统技能安装的标记文件。
const SYSTEM_SKILLS_MARKER_FILENAME: &str = ".vulcan-system-skills.marker";

/// Fingerprint salt for the current embedded system skill install shape.
/// 当前内置系统技能安装结构的指纹盐值。
const SYSTEM_SKILLS_MARKER_SALT: &str = "v1";

/// Installs embedded system prompt skills into the configured `.system` root.
/// 将内置系统提示词技能安装到配置后的 `.system` 根目录。
pub fn install_embedded_system_prompt_skills(
    config: &PromptSkillsGlobalConfig,
) -> Result<SystemPromptSkillsInstallReport, PromptSkillLoadError> {
    install_embedded_system_prompt_skills_to_root(configured_system_skill_root(config))
}

/// Installs embedded system prompt skills into one explicit system root.
/// 将内置系统提示词技能安装到一个显式系统根目录。
pub fn install_embedded_system_prompt_skills_to_root(
    system_root: PathBuf,
) -> Result<SystemPromptSkillsInstallReport, PromptSkillLoadError> {
    let skill_names = embedded_system_skill_names();
    let fingerprint = embedded_system_skills_fingerprint();
    let marker_path = system_root.join(SYSTEM_SKILLS_MARKER_FILENAME);
    if system_root.is_dir() && read_marker(&marker_path).is_ok_and(|marker| marker == fingerprint) {
        return Ok(SystemPromptSkillsInstallReport {
            system_root,
            installed: false,
            fingerprint,
            skill_names,
        });
    }

    if system_root.exists() {
        fs::remove_dir_all(&system_root).map_err(|error| PromptSkillLoadError {
            path: system_root.clone(),
            message: format!("Failed to remove old system prompt skills: {error}"),
        })?;
    }
    fs::create_dir_all(&system_root).map_err(|error| PromptSkillLoadError {
        path: system_root.clone(),
        message: format!("Failed to create system prompt skills root: {error}"),
    })?;
    write_embedded_dir(&SYSTEM_SKILLS_DIR, &system_root)?;
    fs::write(&marker_path, format!("{fingerprint}\n")).map_err(|error| PromptSkillLoadError {
        path: marker_path,
        message: format!("Failed to write system prompt skills marker: {error}"),
    })?;

    Ok(SystemPromptSkillsInstallReport {
        system_root,
        installed: true,
        fingerprint,
        skill_names,
    })
}

/// Installs system prompt skills from one packaged filesystem resource root.
/// 从一个打包后的文件系统资源根目录安装系统提示词技能。
pub fn install_system_prompt_skills_from_source(
    source_root: PathBuf,
    system_root: PathBuf,
) -> Result<SystemPromptSkillsInstallReport, PromptSkillLoadError> {
    if !source_root.is_dir() {
        return Err(PromptSkillLoadError {
            path: source_root,
            message: String::from("System prompt skills source root is not a directory."),
        });
    }

    let skill_names = system_skill_names_from_source_root(&source_root)?;
    let fingerprint = system_skills_source_fingerprint(&source_root)?;
    let marker_path = system_root.join(SYSTEM_SKILLS_MARKER_FILENAME);
    if system_root.is_dir() && read_marker(&marker_path).is_ok_and(|marker| marker == fingerprint) {
        return Ok(SystemPromptSkillsInstallReport {
            system_root,
            installed: false,
            fingerprint,
            skill_names,
        });
    }

    if system_root.exists() {
        fs::remove_dir_all(&system_root).map_err(|error| PromptSkillLoadError {
            path: system_root.clone(),
            message: format!("Failed to remove old system prompt skills: {error}"),
        })?;
    }
    fs::create_dir_all(&system_root).map_err(|error| PromptSkillLoadError {
        path: system_root.clone(),
        message: format!("Failed to create system prompt skills root: {error}"),
    })?;
    copy_source_dir_contents(&source_root, &system_root)?;
    fs::write(&marker_path, format!("{fingerprint}\n")).map_err(|error| PromptSkillLoadError {
        path: marker_path,
        message: format!("Failed to write system prompt skills marker: {error}"),
    })?;

    Ok(SystemPromptSkillsInstallReport {
        system_root,
        installed: true,
        fingerprint,
        skill_names,
    })
}

/// Reads a marker file and normalizes its trailing whitespace.
/// 读取标记文件并规范化尾部空白。
fn read_marker(path: &Path) -> Result<String, PromptSkillLoadError> {
    Ok(fs::read_to_string(path)
        .map_err(|error| PromptSkillLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to read system prompt skills marker: {error}"),
        })?
        .trim()
        .to_string())
}

/// Returns embedded system skill names in stable order.
/// 返回稳定顺序的内置系统技能名称。
pub fn embedded_system_skill_names() -> Vec<String> {
    let mut names = SYSTEM_SKILLS_DIR
        .dirs()
        .filter_map(|dir| dir.path().file_name()?.to_str().map(ToOwned::to_owned))
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Returns packaged system skill names from one filesystem root in stable order.
/// 从一个文件系统根目录返回稳定顺序的打包系统技能名称。
fn system_skill_names_from_source_root(root: &Path) -> Result<Vec<String>, PromptSkillLoadError> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| PromptSkillLoadError {
        path: root.to_path_buf(),
        message: format!("Failed to read system prompt skills source root: {error}"),
    })? {
        let entry = entry.map_err(|error| PromptSkillLoadError {
            path: root.to_path_buf(),
            message: format!("Failed to read system prompt skill source entry: {error}"),
        })?;
        let path = entry.path();
        if path.is_dir()
            && path.join(SKILL_FILE_NAME).is_file()
            && let Some(name) = path.file_name().and_then(|name| name.to_str())
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

/// Builds one fingerprint for all embedded system skill resources.
/// 为所有内置系统技能资源构建一个指纹。
fn embedded_system_skills_fingerprint() -> String {
    let mut items = Vec::new();
    collect_fingerprint_items(&SYSTEM_SKILLS_DIR, &mut items);
    items.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

    let mut hasher = DefaultHasher::new();
    SYSTEM_SKILLS_MARKER_SALT.hash(&mut hasher);
    for (path, contents_hash) in items {
        path.hash(&mut hasher);
        contents_hash.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

/// Builds one fingerprint for a packaged system skill source directory.
/// 为一个打包系统技能源目录构建指纹。
fn system_skills_source_fingerprint(root: &Path) -> Result<String, PromptSkillLoadError> {
    let mut items = Vec::new();
    collect_source_fingerprint_items(root, root, &mut items)?;
    items.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));

    let mut hasher = DefaultHasher::new();
    SYSTEM_SKILLS_MARKER_SALT.hash(&mut hasher);
    for (path, contents_hash) in items {
        path.hash(&mut hasher);
        contents_hash.hash(&mut hasher);
    }
    Ok(format!("{:x}", hasher.finish()))
}

/// Collects relative embedded resource paths and file content hashes.
/// 收集内置资源相对路径与文件内容哈希。
fn collect_fingerprint_items(dir: &Dir<'_>, items: &mut Vec<(String, Option<u64>)>) {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(subdir) => {
                items.push((subdir.path().to_string_lossy().to_string(), None));
                collect_fingerprint_items(subdir, items);
            }
            include_dir::DirEntry::File(file) => {
                let mut file_hasher = DefaultHasher::new();
                file.contents().hash(&mut file_hasher);
                items.push((
                    file.path().to_string_lossy().to_string(),
                    Some(file_hasher.finish()),
                ));
            }
        }
    }
}

/// Collects relative filesystem resource paths and file content hashes.
/// 收集文件系统资源相对路径与文件内容哈希。
fn collect_source_fingerprint_items(
    root: &Path,
    current: &Path,
    items: &mut Vec<(String, Option<u64>)>,
) -> Result<(), PromptSkillLoadError> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| PromptSkillLoadError {
            path: current.to_path_buf(),
            message: format!("Failed to read system prompt skill source directory: {error}"),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PromptSkillLoadError {
            path: current.to_path_buf(),
            message: format!("Failed to read system prompt skill source entry: {error}"),
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            items.push((relative_path, None));
            collect_source_fingerprint_items(root, &path, items)?;
        } else if path.is_file() {
            let contents = fs::read(&path).map_err(|error| PromptSkillLoadError {
                path: path.clone(),
                message: format!("Failed to read system prompt skill source file: {error}"),
            })?;
            let mut file_hasher = DefaultHasher::new();
            contents.hash(&mut file_hasher);
            items.push((relative_path, Some(file_hasher.finish())));
        }
    }

    Ok(())
}

/// Writes one embedded include-dir tree into the destination directory.
/// 将一个内置 include-dir 树写入目标目录。
fn write_embedded_dir(dir: &Dir<'_>, dest: &Path) -> Result<(), PromptSkillLoadError> {
    fs::create_dir_all(dest).map_err(|error| PromptSkillLoadError {
        path: dest.to_path_buf(),
        message: format!("Failed to create system prompt skills directory: {error}"),
    })?;

    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(subdir) => {
                write_embedded_dir(subdir, dest)?;
            }
            include_dir::DirEntry::File(file) => {
                let path = dest.join(file.path());
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| PromptSkillLoadError {
                        path: parent.to_path_buf(),
                        message: format!("Failed to create system prompt skill parent: {error}"),
                    })?;
                }
                fs::write(&path, file.contents()).map_err(|error| PromptSkillLoadError {
                    path,
                    message: format!("Failed to write system prompt skill file: {error}"),
                })?;
            }
        }
    }

    Ok(())
}

/// Copies direct filesystem source contents into the destination directory.
/// 将文件系统来源目录的直属内容复制到目标目录。
fn copy_source_dir_contents(source: &Path, dest: &Path) -> Result<(), PromptSkillLoadError> {
    for entry in fs::read_dir(source).map_err(|error| PromptSkillLoadError {
        path: source.to_path_buf(),
        message: format!("Failed to read system prompt skills source directory: {error}"),
    })? {
        let entry = entry.map_err(|error| PromptSkillLoadError {
            path: source.to_path_buf(),
            message: format!("Failed to read system prompt skill source entry: {error}"),
        })?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if source_path.is_dir() {
            copy_source_dir_recursive(&source_path, &dest_path)?;
        } else if source_path.is_file() {
            fs::copy(&source_path, &dest_path).map_err(|error| PromptSkillLoadError {
                path: source_path,
                message: format!("Failed to copy system prompt skill source file: {error}"),
            })?;
        }
    }
    Ok(())
}

/// Copies one filesystem source directory recursively into the destination directory.
/// 将一个文件系统来源目录递归复制到目标目录。
fn copy_source_dir_recursive(source: &Path, dest: &Path) -> Result<(), PromptSkillLoadError> {
    fs::create_dir_all(dest).map_err(|error| PromptSkillLoadError {
        path: dest.to_path_buf(),
        message: format!("Failed to create system prompt skill destination directory: {error}"),
    })?;
    copy_source_dir_contents(source, dest)
}
