// Mirror of `service/project-actions-service.ts` for the calls system pages make.
// `service/project-actions-service.ts` 中系统页面所用调用的镜像。
//
// Sidebar calls of the same file (select, rename, pin, archive, remove ...) are typed `Command`
// variants with the same names, because the backend tracks selection and draft state around them.
// 同一文件中的侧栏调用（选择、重命名、置顶、归档、移除……）是同名的类型化 `Command` 变体，因为后端
// 需要围绕它们维护选中与草稿状态。

use serde_json::json;

use super::{ServiceCall, Slot};

/// Builds the request for `service_restore_archived_conversations`.
/// 构建 `service_restore_archived_conversations` 的请求。
/// `conversation_ids` is the exact archived conversation identities to restore or delete.
/// `conversation_ids` 为需要恢复或删除的准确归档会话身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn restore_archived_conversations(conversation_ids: &[String]) -> ServiceCall {
    ServiceCall::mutate(
        "restore_archived_conversations",
        "service_restore_archived_conversations",
        json!({ "conversationIds": conversation_ids }),
        &[Slot::Workbench],
    )
}

/// Builds the request for `service_delete_archived_conversations`.
/// 构建 `service_delete_archived_conversations` 的请求。
/// `conversation_ids` is the exact archived conversation identities to restore or delete.
/// `conversation_ids` 为需要恢复或删除的准确归档会话身份。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn delete_archived_conversations(conversation_ids: &[String]) -> ServiceCall {
    ServiceCall::mutate(
        "delete_archived_conversations",
        "service_delete_archived_conversations",
        json!({ "conversationIds": conversation_ids }),
        &[Slot::Workbench],
    )
}

/// Builds the request for `service_open_external_url`; the service opens the URL on its host.
/// 构建 `service_open_external_url` 的请求；由服务在其宿主上打开该 URL。
/// `url` is the exact external URL the user chose to open.
/// `url` 为用户选择打开的准确外部 URL。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn open_external_url(url: &str) -> ServiceCall {
    ServiceCall::mutate(
        "open_external_url",
        "service_open_external_url",
        json!({ "url": url }),
        &[],
    )
}
