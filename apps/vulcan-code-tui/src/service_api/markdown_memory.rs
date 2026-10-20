// Mirror of `service/markdown-memory-service.ts`: the memory mode settings and Markdown memory.
// `service/markdown-memory-service.ts` 的镜像：记忆模式设置与 Markdown 记忆。

use serde_json::{Map, Value, json};

use super::{ServiceCall, Slot};

// A mode or budget change also changes the usage and project list the overview reports.
// 模式或预算变化也会改变总览报告的用量与项目列表。
const SETTINGS: &[Slot] = &[Slot::MemorySettings, Slot::MarkdownMemory];
// A node or file write changes both the node tree and the file text.
// 节点或文件写入同时改变节点树与文件文本。
const CONTENT: &[Slot] = &[Slot::MarkdownMemory, Slot::MarkdownMemoryFile];
// An import changes the notes and the merge the package preview reports.
// 导入会改变笔记以及导出包预览报告的合并结果。
const IMPORT: &[Slot] = &[Slot::MarkdownMemory, Slot::MarkdownMemoryImport];

/// Builds the request for `service_get_memory_settings`.
/// 构建 `service_get_memory_settings` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_memory_settings() -> ServiceCall {
    ServiceCall::load(
        "load_memory_settings",
        "service_get_memory_settings",
        json!({}),
        Slot::MemorySettings,
    )
}

/// Builds the request for `service_save_memory_settings`.
/// 构建 `service_save_memory_settings` 的请求。
/// `mode` is the chosen memory mode; `budget` is the complete injection budget.
/// `mode` 为所选记忆模式；`budget` 为完整注入预算。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_memory_settings(mode: &str, budget: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_memory_settings",
        "service_save_memory_settings",
        json!({ "request": { "mode": mode, "budget": budget } }),
        SETTINGS,
    )
}

/// Builds the request for `service_load_markdown_memory`.
/// 构建 `service_load_markdown_memory` 的请求。
/// `project_id` is the host project whose memory is shown; `None` shows only user and space.
/// `project_id` 为显示其记忆的宿主项目；`None` 只显示用户与空间。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_markdown_memory(project_id: Option<&str>) -> ServiceCall {
    ServiceCall::load(
        "load_markdown_memory",
        "service_load_markdown_memory",
        json!({ "request": target(project_id, None) }),
        Slot::MarkdownMemory,
    )
}

/// Builds the request for `service_write_markdown_memory`; the operations apply all or nothing.
/// 构建 `service_write_markdown_memory` 的请求；操作整体生效或整体不生效。
/// `project_id` is the host project of the page; `operations` follows `MarkdownMemoryOperation`.
/// `project_id` 为页面的宿主项目；`operations` 遵循 `MarkdownMemoryOperation`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn write_markdown_memory(project_id: Option<&str>, operations: Value) -> ServiceCall {
    let mut request = target(project_id, None);
    request.insert("operations".into(), operations);
    ServiceCall::mutate(
        "write_markdown_memory",
        "service_write_markdown_memory",
        json!({ "request": request }),
        CONTENT,
    )
}

/// Builds the request for `service_read_markdown_memory_file`.
/// 构建 `service_read_markdown_memory_file` 的请求。
/// `scope` is user, space or project; `memory_project_id` names the project file.
/// `scope` 为 user、space 或 project；`memory_project_id` 指定项目文件。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn read_markdown_memory_file(scope: &str, memory_project_id: Option<&str>) -> ServiceCall {
    let mut request = target(None, memory_project_id);
    request.insert("scope".into(), json!(scope));
    ServiceCall::load(
        "read_markdown_memory_file",
        "service_read_markdown_memory_file",
        json!({ "request": request }),
        Slot::MarkdownMemoryFile,
    )
}

/// Builds the request for `service_replace_markdown_memory_file`; `expected` is the text the edit
/// started from, so a concurrent change is refused instead of overwritten.
/// 构建 `service_replace_markdown_memory_file` 的请求；`expected` 为编辑起点的文本，因此并发变化会被
/// 拒绝而不是被覆盖。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn replace_markdown_memory_file(
    scope: &str,
    memory_project_id: Option<&str>,
    expected: &str,
    text: &str,
) -> ServiceCall {
    let mut request = target(None, memory_project_id);
    request.insert("scope".into(), json!(scope));
    request.insert("expected".into(), json!(expected));
    request.insert("text".into(), json!(text));
    ServiceCall::mutate(
        "replace_markdown_memory_file",
        "service_replace_markdown_memory_file",
        json!({ "request": request }),
        CONTENT,
    )
}

/// Builds the request for `service_export_markdown_memory`.
/// 构建 `service_export_markdown_memory` 的请求。
/// `request` follows `MarkdownMemoryExportRequest`.
/// `request` 遵循 `MarkdownMemoryExportRequest`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn export_markdown_memory(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "export_markdown_memory",
        "service_export_markdown_memory",
        json!({ "request": request }),
        &[],
    )
}

/// Builds the request for `service_preview_markdown_memory_import`; nothing is written.
/// 构建 `service_preview_markdown_memory_import` 的请求；不写入任何内容。
/// `archive_path` is the exact package path; `projects` lists the chosen `ProjectMapping`s.
/// `archive_path` 为精确导出包路径；`projects` 列出已选的 `ProjectMapping`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn preview_markdown_memory_import(archive_path: &str, projects: Value) -> ServiceCall {
    ServiceCall::load(
        "preview_markdown_memory_import",
        "service_preview_markdown_memory_import",
        json!({ "request": { "archivePath": archive_path, "projects": projects } }),
        Slot::MarkdownMemoryImport,
    )
}

/// Builds the request for `service_import_markdown_memory`; replaced files are backed up first.
/// 构建 `service_import_markdown_memory` 的请求；被替换的文件会先备份。
/// `request` follows `MarkdownMemoryImportRequest`.
/// `request` 遵循 `MarkdownMemoryImportRequest`。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn import_markdown_memory(request: Value) -> ServiceCall {
    ServiceCall::mutate(
        "import_markdown_memory",
        "service_import_markdown_memory",
        json!({ "request": request }),
        IMPORT,
    )
}

/// Builds the request for `service_apply_markdown_memory_project_action`.
/// 构建 `service_apply_markdown_memory_project_action` 的请求。
/// `action` follows `MarkdownMemoryProjectAction` (`link`, `create`, `ignore`, `set_enabled`, `rename`).
/// `action` 遵循 `MarkdownMemoryProjectAction`（`link`、`create`、`ignore`、`set_enabled`、`rename`）。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn apply_markdown_memory_project_action(action: Value) -> ServiceCall {
    ServiceCall::mutate(
        "apply_markdown_memory_project_action",
        "service_apply_markdown_memory_project_action",
        json!({ "request": action }),
        &[Slot::MarkdownMemory],
    )
}

/// Builds the target fields the WebUI sends, leaving absent identities out instead of null.
/// 构建 WebUI 发送的目标字段；缺失的身份直接省略而不是置空。
fn target(project_id: Option<&str>, memory_project_id: Option<&str>) -> Map<String, Value> {
    let mut request = Map::new();
    if let Some(id) = project_id.filter(|id| !id.is_empty()) {
        request.insert("projectId".into(), json!(id));
    }
    if let Some(id) = memory_project_id.filter(|id| !id.is_empty()) {
        request.insert("memoryProjectId".into(), json!(id));
    }
    request
}
