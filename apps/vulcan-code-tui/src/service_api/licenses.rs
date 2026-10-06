// Mirror of `service/licenses-service.ts`.
// `service/licenses-service.ts` 的镜像。

use serde_json::{Value, json};

use super::{ServiceCall, Slot};

/// Builds the request for `service_license_catalog`.
/// 构建 `service_license_catalog` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_license_catalog() -> ServiceCall {
    ServiceCall::load(
        "load_license_catalog",
        "service_license_catalog",
        Value::Null,
        Slot::Licenses,
    )
}

/// One license text; `expected_sha256` comes from the catalog entry.
/// 一份许可证正文；`expected_sha256` 来自目录条目。
/// Builds the request for `service_license_body`.
/// 构建 `service_license_body` 的请求。
/// `section_id` is the license section ID returned by the catalog.
/// `section_id` 为许可证目录返回的章节标识。
/// `expected_sha256` is the catalog-issued hash guarding the license body.
/// `expected_sha256` 为目录签发、用于保护许可证正文的哈希。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_license_body(section_id: &str, expected_sha256: &str) -> ServiceCall {
    ServiceCall::load(
        "load_license_body",
        "service_license_body",
        json!({ "sectionId": section_id, "expectedSha256": expected_sha256 }),
        Slot::LicenseBody,
    )
}
