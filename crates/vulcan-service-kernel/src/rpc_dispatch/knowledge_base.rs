//! Knowledge-base RPC routing.
//! 知识库 RPC 路由。

use super::*;

/// Exact public RPC wire names owned by the knowledge-base domain.
/// 知识库领域持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_list_knowledge_libraries",
    "service_create_knowledge_library",
    "service_set_knowledge_library_settings",
    "service_rename_knowledge_library",
    "service_delete_knowledge_library",
    "service_set_knowledge_project_binding",
    "service_list_knowledge_project_bindings",
    "service_list_project_knowledge_bindings",
    "service_list_knowledge_sources",
    "service_import_knowledge_directory",
    "service_cancel_knowledge_import",
    "service_get_knowledge_import_job",
    "service_rebuild_knowledge_index",
    "service_repair_knowledge_index",
    "service_get_knowledge_index_integrity",
    "service_list_knowledge_custom_words",
    "service_upsert_knowledge_custom_word",
    "service_remove_knowledge_custom_word",
    "service_list_knowledge_library_documents",
    "service_list_knowledge_library_folders",
    "service_read_knowledge_library_document",
    "service_search_knowledge_library",
    "service_build_knowledge_display_index",
    "service_build_knowledge_vector_index",
    "service_export_knowledge_package",
    "service_inspect_knowledge_package",
    "service_import_knowledge_package",
    "service_browse_knowledge_packages",
    "service_list_knowledge_documents",
    "service_read_knowledge_document",
    "service_search_knowledge",
    "service_set_session_knowledge_libraries",
    "service_get_session_knowledge_libraries",
];

/// Dispatches one exact knowledge-base RPC.
/// 分发一个精确知识库 RPC。
pub(super) async fn dispatch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, String> {
    let result = match method {
        "service_list_knowledge_libraries" => {
            let project_id = params
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string);
            service_list_knowledge_libraries(state, project_id).await
        }
        "service_create_knowledge_library" => {
            service_create_knowledge_library(app, state, rpc_required_param(params, "request")?)
                .await
        }
        "service_set_knowledge_library_settings" => {
            service_set_knowledge_library_settings(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await
        }
        "service_rename_knowledge_library" => {
            service_rename_knowledge_library(state, rpc_required_param(params, "request")?).await
        }
        "service_delete_knowledge_library" => {
            service_delete_knowledge_library(app, state, rpc_required_param(params, "libraryId")?)
                .await
        }
        "service_set_knowledge_project_binding" => {
            service_set_knowledge_project_binding(state, rpc_required_param(params, "request")?)
                .await
        }
        "service_list_knowledge_project_bindings" => {
            service_list_knowledge_project_bindings(state, rpc_required_param(params, "libraryId")?)
                .await
        }
        "service_list_project_knowledge_bindings" => {
            service_list_project_knowledge_bindings(state, rpc_required_param(params, "projectId")?)
                .await
        }
        "service_list_knowledge_sources" => {
            service_list_knowledge_sources(state, rpc_required_param(params, "libraryId")?).await
        }
        "service_import_knowledge_directory" => {
            service_import_knowledge_directory(state, rpc_required_param(params, "request")?).await
        }
        "service_cancel_knowledge_import" => {
            service_cancel_knowledge_import(
                state,
                rpc_required_param(params, "libraryId")?,
                rpc_required_param(params, "jobId")?,
            )
            .await
        }
        "service_get_knowledge_import_job" => {
            service_get_knowledge_import_job(state, rpc_required_param(params, "libraryId")?).await
        }
        "service_rebuild_knowledge_index" => {
            service_rebuild_knowledge_index(state, rpc_required_param(params, "libraryId")?).await
        }
        "service_repair_knowledge_index" => {
            service_repair_knowledge_index(state, rpc_required_param(params, "libraryId")?).await
        }
        "service_get_knowledge_index_integrity" => {
            service_get_knowledge_index_integrity(state, rpc_required_param(params, "libraryId")?)
                .await
        }
        "service_list_knowledge_custom_words" => {
            service_list_knowledge_custom_words(state, rpc_required_param(params, "libraryId")?)
                .await
        }
        "service_upsert_knowledge_custom_word" => {
            service_upsert_knowledge_custom_word(state, rpc_required_param(params, "request")?)
                .await
        }
        "service_remove_knowledge_custom_word" => {
            service_remove_knowledge_custom_word(
                state,
                rpc_required_param(params, "libraryId")?,
                rpc_required_param(params, "word")?,
            )
            .await
        }
        "service_list_knowledge_library_documents" => {
            service_list_knowledge_library_documents(state, rpc_required_param(params, "request")?)
                .await
        }
        "service_list_knowledge_library_folders" => {
            service_list_knowledge_library_folders(state, rpc_required_param(params, "libraryId")?)
                .await
        }
        "service_read_knowledge_library_document" => {
            service_read_knowledge_library_document(
                state,
                rpc_required_param(params, "libraryId")?,
                rpc_required_param(params, "documentId")?,
                rpc_optional_param(params, "corpus")?,
            )
            .await
        }
        "service_search_knowledge_library" => {
            service_search_knowledge_library(state, rpc_required_param(params, "request")?).await
        }
        "service_build_knowledge_display_index" => {
            service_build_knowledge_display_index(state, rpc_required_param(params, "libraryId")?)
                .await
        }
        "service_build_knowledge_vector_index" => {
            service_build_knowledge_vector_index(state, rpc_required_param(params, "libraryId")?)
                .await
        }
        "service_export_knowledge_package" => {
            service_export_knowledge_package(state, rpc_required_param(params, "request")?).await
        }
        "service_inspect_knowledge_package" => {
            service_inspect_knowledge_package(state, rpc_required_param(params, "request")?).await
        }
        "service_import_knowledge_package" => {
            service_import_knowledge_package(app, state, rpc_required_param(params, "request")?)
                .await
        }
        "service_browse_knowledge_packages" => service_browse_knowledge_packages(
            state,
            params
                .get("path")
                .and_then(Value::as_str)
                .map(str::to_string),
        ),
        "service_list_knowledge_documents" => {
            service_list_knowledge_documents(state, rpc_required_param(params, "request")?).await
        }
        "service_read_knowledge_document" => {
            service_read_knowledge_document(
                state,
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "libraryId")?,
                rpc_required_param(params, "documentId")?,
            )
            .await
        }
        "service_search_knowledge" => {
            service_search_knowledge(state, rpc_required_param(params, "request")?).await
        }
        "service_set_session_knowledge_libraries" => {
            service_set_session_knowledge_libraries(
                app,
                state,
                rpc_required_param(params, "request")?,
            )
            .await
        }
        "service_get_session_knowledge_libraries" => {
            service_get_session_knowledge_libraries(
                state,
                rpc_required_param(params, "conversationId")?,
            )
            .await
        }
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
