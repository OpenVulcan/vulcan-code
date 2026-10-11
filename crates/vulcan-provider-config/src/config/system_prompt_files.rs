//! User-level and project-level base system prompt files.
//! 用户级与项目级基础系统提示词文件。

use super::*;
use crate::atomic_file::write_file_atomically;

/// Directory below the user or project Vulcan Code root that stores editable prompt resources.
/// 用户或项目 Vulcan Code 根目录下存放可编辑提示词资源的目录。
pub const PROMPT_RESOURCE_DIRECTORY_NAME: &str = "prompt";

/// File name of an editable base system prompt override.
/// 可编辑基础系统提示词覆盖文件的文件名。
pub const SYSTEM_PROMPT_FILE_NAME: &str = "system-prompt.md";

/// Returns the user-level prompt resource directory.
/// 返回用户级提示词资源目录。
///
/// # Returns
/// 返回值
/// `<user-vulcan-root>/code/prompt`.
/// 返回 `<user-vulcan-root>/code/prompt`。
pub fn user_prompt_resource_directory() -> PathBuf {
    let home = vulcan_runtime_assets::user_home_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    runtime_assets::user_vulcan_code_root_for_home(&home).join(PROMPT_RESOURCE_DIRECTORY_NAME)
}

/// Returns the user-level base system prompt override path.
/// 返回用户级基础系统提示词覆盖文件路径。
pub fn user_system_prompt_file_path() -> PathBuf {
    user_prompt_resource_directory().join(SYSTEM_PROMPT_FILE_NAME)
}

/// Returns the project-level base system prompt override path.
/// 返回项目级基础系统提示词覆盖文件路径。
///
/// # Parameters
/// 参数
/// - `workspace_path`: Exact workspace directory that owns `.vulcan/code/`.
/// - `workspace_path`：拥有 `.vulcan/code/` 的精确工作区目录。
///
/// # Returns
/// 返回值
/// `<workspace>/.vulcan/code/prompt/system-prompt.md`.
/// 返回 `<workspace>/.vulcan/code/prompt/system-prompt.md`。
pub fn project_system_prompt_file_path(workspace_path: &Path) -> PathBuf {
    // The project prompt directory is a sibling of the project configuration file.
    // 项目提示词目录与项目配置文件位于同一目录下。
    let mut prompt_directory = project_configuration_file_path(workspace_path);
    prompt_directory.set_file_name(PROMPT_RESOURCE_DIRECTORY_NAME);
    prompt_directory.join(SYSTEM_PROMPT_FILE_NAME)
}

/// Reads one base system prompt override.
/// 读取一个基础系统提示词覆盖文件。
///
/// # Returns
/// 返回值
/// `None` when the file is missing or blank; read and UTF-8 failures are preserved.
/// 文件缺失或为空白时返回 `None`；保留读取与 UTF-8 失败。
pub fn read_system_prompt_file(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let text = text.trim();
            Ok((!text.is_empty()).then(|| text.to_string()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "Failed to read system prompt `{}`: {error}",
            path.display()
        )),
    }
}

/// Atomically saves one base system prompt override.
/// 原子保存一个基础系统提示词覆盖文件。
///
/// # Parameters
/// 参数
/// - `path`: Exact user or project override path.
/// - `path`：精确的用户或项目覆盖文件路径。
/// - `text`: Complete prompt body; blank text is rejected because removal is a separate action.
/// - `text`：完整提示词正文；空白正文被拒绝，因为移除是独立动作。
pub fn save_system_prompt_file(path: &Path, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(String::from(
            "System prompt text is empty; remove the override to use the inherited prompt.",
        ));
    }
    write_file_atomically(path, format!("{text}\n").as_bytes())
        .map_err(|error| format!("Failed to save system prompt `{}`: {error}", path.display()))
}

/// Removes one base system prompt override so the next layer takes effect.
/// 移除一个基础系统提示词覆盖文件，使下一层生效。
///
/// # Returns
/// 返回值
/// `Ok(())` when the file is removed or already absent.
/// 文件已移除或本就不存在时返回 `Ok(())`。
pub fn remove_system_prompt_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Failed to remove system prompt `{}`: {error}",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    //! System prompt file persistence tests.
    //! 系统提示词文件持久化测试。

    use super::*;

    /// Builds one absent, process-unique workspace directory below the host temp directory.
    /// 在宿主临时目录下构建一个不存在且进程唯一的工作区目录。
    fn unique_workspace(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vulcan-system-prompt-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("test clock should follow the Unix epoch")
                .as_nanos()
        ))
    }

    /// Verifies the project override lives in the prompt directory beside the project configuration.
    /// 验证项目覆盖文件位于项目配置旁的提示词目录中。
    #[test]
    fn project_path_is_prompt_directory_beside_project_configuration() {
        let workspace = unique_workspace("path");
        let path = project_system_prompt_file_path(&workspace);
        let configuration_directory = project_configuration_file_path(&workspace)
            .parent()
            .expect("project configuration should have a parent")
            .to_path_buf();
        assert_eq!(
            path,
            configuration_directory
                .join(PROMPT_RESOURCE_DIRECTORY_NAME)
                .join(SYSTEM_PROMPT_FILE_NAME)
        );
    }

    /// Verifies save, read, and remove form a complete override lifecycle.
    /// 验证保存、读取与移除构成完整的覆盖生命周期。
    #[test]
    fn override_lifecycle_saves_reads_and_removes() {
        let workspace = unique_workspace("lifecycle");
        let path = project_system_prompt_file_path(&workspace);

        assert_eq!(read_system_prompt_file(&path).expect("read absent"), None);
        save_system_prompt_file(&path, "  Project prompt  ").expect("save override");
        assert_eq!(
            read_system_prompt_file(&path).expect("read saved"),
            Some(String::from("Project prompt"))
        );
        remove_system_prompt_file(&path).expect("remove override");
        remove_system_prompt_file(&path).expect("remove absent override");
        assert_eq!(read_system_prompt_file(&path).expect("read removed"), None);

        let _ = fs::remove_dir_all(workspace);
    }

    /// Verifies blank text is rejected instead of creating an override that silently means "absent".
    /// 验证空白正文会被拒绝，而不是创建一个静默等同于缺失的覆盖文件。
    #[test]
    fn save_rejects_blank_text() {
        let workspace = unique_workspace("blank");
        let path = project_system_prompt_file_path(&workspace);
        assert!(save_system_prompt_file(&path, " \n\t").is_err());
        assert!(!path.exists());
    }
}
