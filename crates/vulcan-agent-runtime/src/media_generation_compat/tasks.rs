//! MiniMax task polling and binary download helpers.
//! MiniMax 任务轮询与二进制下载辅助能力。

use reqwest::header::AUTHORIZATION;
use serde_json::Value;
use tokio::time::{Duration, sleep};

use crate::provider_config::PersistedAiCapabilityVendorEntry;

use super::{
    codecs::truncate_error_text, endpoints::resolve_minimax_file_or_task_endpoint,
    http::parse_json_response,
};

/// Polls the MiniMax video-task endpoint until the task finishes or times out.
/// 轮询 MiniMax 视频任务端点，直到任务完成或超时。
pub(super) async fn poll_minimax_video_task(
    client: &reqwest::Client,
    vendor: &PersistedAiCapabilityVendorEntry,
    api_key: &str,
    task_id: &str,
) -> Result<Value, String> {
    let mut elapsed_seconds = 0u64;
    loop {
        let endpoint = resolve_minimax_file_or_task_endpoint(
            vendor,
            "/v1/query/video_generation",
            &[("task_id", task_id)],
        )?;
        let response = client
            .get(endpoint)
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .send()
            .await
            .map_err(|error| format!("MiniMax Video Task Query failed: {error}"))?;
        let payload = parse_json_response(response, "MiniMax Video Task Query").await?;
        let status = payload
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match status {
            "Success" => return Ok(payload),
            "Failed" => {
                return Err(format!(
                    "MiniMax video task failed: {}",
                    truncate_error_text(&payload.to_string(), 240)
                ));
            }
            _ => {}
        }
        if elapsed_seconds >= 300 {
            return Err(String::from(
                "Timed out while waiting for the MiniMax video task to finish (300 seconds).",
            ));
        }
        sleep(Duration::from_secs(5)).await;
        elapsed_seconds += 5;
    }
}

/// Fetches the final download URL for one MiniMax file identifier.
/// 获取单个 MiniMax 文件标识的最终下载 URL。
pub(super) async fn fetch_minimax_file_download_url(
    client: &reqwest::Client,
    vendor: &PersistedAiCapabilityVendorEntry,
    api_key: &str,
    file_id: &str,
) -> Result<String, String> {
    let endpoint = resolve_minimax_file_or_task_endpoint(
        vendor,
        "/v1/files/retrieve",
        &[("file_id", file_id)],
    )?;
    let response = client
        .get(endpoint)
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|error| format!("MiniMax File Download URL Query failed: {error}"))?;
    let payload = parse_json_response(response, "MiniMax File Download URL Query").await?;
    payload
        .pointer("/file/download_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            String::from("MiniMax file download URL response is missing `download_url`.")
        })
}

/// Downloads one binary payload from the provided URL.
/// 从给定 URL 下载一份二进制载荷。
pub(super) async fn download_binary(url: &str, label: &str) -> Result<Vec<u8>, String> {
    let response = reqwest::get(url)
        .await
        .map_err(|error| format!("{label} request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body_text = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("Unable to read the response body"));
        return Err(format!(
            "{label} returned HTTP {}: {}",
            status,
            truncate_error_text(&body_text, 240)
        ));
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("Failed to read response bytes from {label}: {error}"))
}
