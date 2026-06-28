//! Git diff text pathspec helpers.
//! Git diff 文本 pathspec 辅助逻辑。

use super::*;

pub(crate) fn ensure_webui_git_pathspec_text(pathspec: &str) -> Result<(), String> {
    let value = pathspec.trim();
    if value.is_empty()
        || value == "/dev/null"
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
        || value.starts_with('/')
        || value.starts_with('\\')
    {
        return Err(String::from(
            "invalid file path for WebUI diff detail request",
        ));
    }
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(String::from(
            "invalid file path for WebUI diff detail request",
        ));
    }
    Ok(())
}
