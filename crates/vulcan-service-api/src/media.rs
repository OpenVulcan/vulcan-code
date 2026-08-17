use super::*;

/// Framework-neutral media response returned by the Core Service media pipeline.
/// Core Service 媒体管线返回的框架无关媒体响应。
pub struct ServiceMediaResponse {
    /// Numeric HTTP status code.
    /// 数字 HTTP 状态码。
    pub status: u16,
    /// Response headers preserved in insertion order.
    /// 按插入顺序保留的响应头。
    pub headers: Vec<(String, String)>,
    /// Response body bytes.
    /// 响应体字节。
    pub body: Vec<u8>,
}

/// Decodes one URL-safe base64 media route segment into a local filesystem path.
/// 将单个 URL 安全 base64 媒体路由片段解码为本地文件系统路径。
pub fn service_media_path_from_encoded_route_segment(
    encoded_path: &str,
) -> Result<PathBuf, String> {
    let path_bytes = BASE64_URL_SAFE_NO_PAD
        .decode(encoded_path)
        .map_err(|error| format!("媒体路径解码失败：{error}"))?;
    let path_text = String::from_utf8(path_bytes)
        .map_err(|error| format!("媒体路径不是有效 UTF-8：{error}"))?;
    Ok(PathBuf::from(path_text))
}

/// Builds one shared media file response with optional HTTP range support.
/// 构建单个带可选 HTTP Range 支持的共享媒体文件响应。
pub fn build_service_media_file_response(
    media_path: PathBuf,
    is_head: bool,
    range_header: Option<&str>,
) -> Result<ServiceMediaResponse, String> {
    if !is_allowed_service_media_asset_path(&media_path) {
        return Ok(service_media_error_response(
            403,
            "媒体路径不是允许的媒体资源类型。",
        ));
    }

    let mut file = match File::open(&media_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(service_media_error_response(404, "媒体文件不存在。"));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(service_media_error_response(403, "没有权限读取媒体文件。"));
        }
        Err(error) => return Err(format!("无法打开媒体文件：{error}")),
    };
    let file_len = file
        .metadata()
        .map_err(|error| format!("无法读取媒体文件元数据：{error}"))?
        .len();
    let mime_type = infer_service_media_asset_mime_type(&media_path);
    let mut headers = service_media_base_headers(mime_type);

    if is_head {
        headers.push((String::from("content-length"), file_len.to_string()));
        return Ok(ServiceMediaResponse {
            status: 200,
            headers,
            body: Vec::new(),
        });
    }

    if let Some(range_header) = range_header {
        let range = single_service_media_http_range(range_header, file_len)?;
        let range_start = range.start;
        let range_end = range.start + range.length - 1;
        let mut body = Vec::with_capacity(range.length as usize);
        file.seek(SeekFrom::Start(range_start))
            .map_err(|error| format!("无法定位媒体范围：{error}"))?;
        file.take(range.length)
            .read_to_end(&mut body)
            .map_err(|error| format!("无法读取媒体范围：{error}"))?;
        headers.push((
            String::from("content-range"),
            format!("bytes {range_start}-{range_end}/{file_len}"),
        ));
        headers.push((String::from("content-length"), body.len().to_string()));
        return Ok(ServiceMediaResponse {
            status: 206,
            headers,
            body,
        });
    }

    let mut body = Vec::with_capacity(file_len.min(SERVICE_MEDIA_RANGE_MAX_BYTES) as usize);
    file.read_to_end(&mut body)
        .map_err(|error| format!("无法读取媒体文件：{error}"))?;
    headers.push((String::from("content-length"), file_len.to_string()));
    Ok(ServiceMediaResponse {
        status: 200,
        headers,
        body,
    })
}

/// Returns whether a media path belongs to the generated attachment storage tree.
/// 返回媒体路径是否属于生成附件存储树。
fn is_allowed_generated_service_media_path(path: &Path) -> bool {
    let normalized_components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str().map(|text| text.to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    normalized_components.windows(3).any(|window| {
        // Workspace attachments retain `.vulcan`, while user-profile attachments use the build-profile-aware root.
        // 工作区附件保留 `.vulcan`，用户配置附件则使用感知构建配置的根目录。
        (window[0] == ".vulcan"
            || window[0] == vulcan_runtime_assets::USER_VULCAN_ROOT_DIRECTORY_NAME)
            && window[1] == "code"
            && window[2] == "attachments"
    })
}

/// Returns whether a media path can be served by the local media route.
/// 返回媒体路径是否允许通过本地媒体路由提供。
fn is_allowed_service_media_asset_path(path: &Path) -> bool {
    if is_allowed_generated_service_media_path(path) {
        return true;
    }
    has_supported_service_media_asset_extension(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies generated JSON attachments remain servable from both authoritative attachment roots.
    /// 验证生成的 JSON 附件可从两个权威附件根目录提供。
    ///
    /// # Returns
    /// 返回值
    /// Returns normally after both roots are accepted and a lookalike root is rejected.
    /// 两个根目录均被接受且相似伪根目录被拒绝后正常返回。
    #[test]
    fn generated_json_attachments_accept_workspace_and_profile_roots() {
        // WorkspaceAttachment represents the unchanged workspace-local storage contract.
        // WorkspaceAttachment 表示保持不变的工作区本地存储契约。
        let workspace_attachment = PathBuf::from("workspace")
            .join(".vulcan")
            .join("code")
            .join("attachments")
            .join("payload.json");
        // ProfileAttachment represents the build-profile-aware user storage contract.
        // ProfileAttachment 表示感知构建配置的用户存储契约。
        let profile_attachment = PathBuf::from("profile")
            .join(vulcan_runtime_assets::USER_VULCAN_ROOT_DIRECTORY_NAME)
            .join("code")
            .join("attachments")
            .join("payload.json");
        // LookalikeAttachment proves only exact authoritative root components are accepted.
        // LookalikeAttachment 证明仅接受精确的权威根目录组件。
        let lookalike_attachment = PathBuf::from("profile")
            .join(".vulcan-debug-backup")
            .join("code")
            .join("attachments")
            .join("payload.json");

        assert!(is_allowed_service_media_asset_path(&workspace_attachment));
        assert!(is_allowed_service_media_asset_path(&profile_attachment));
        assert!(!is_allowed_service_media_asset_path(&lookalike_attachment));
    }
}

/// Returns whether a local path extension is supported by the media route.
/// 返回本地路径扩展名是否受媒体路由支持。
fn has_supported_service_media_asset_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png")
            | Some("jpg")
            | Some("jpeg")
            | Some("webp")
            | Some("gif")
            | Some("bmp")
            | Some("svg")
            | Some("mp4")
            | Some("webm")
            | Some("mov")
            | Some("mkv")
            | Some("mp3")
            | Some("wav")
            | Some("m4a")
            | Some("aac")
            | Some("flac")
            | Some("ogg")
            | Some("pdf")
    )
}

/// Parses one bounded HTTP range for media playback.
/// 解析一个有上限的媒体播放 HTTP 范围。
fn single_service_media_http_range(range_header: &str, file_len: u64) -> Result<HttpRange, String> {
    let mut ranges = HttpRange::parse(range_header, file_len)
        .map_err(|error| format!("媒体 Range 请求无效：{error:?}"))?;
    let Some(mut range) = ranges.drain(..).next() else {
        return Err(String::from("媒体 Range 请求为空。"));
    };
    if range.start >= file_len || range.length == 0 {
        return Err(String::from("媒体 Range 请求越界。"));
    }
    range.length = range.length.min(SERVICE_MEDIA_RANGE_MAX_BYTES);
    Ok(range)
}

/// Builds one plain text error response for the generated media route.
/// 为生成媒体路由构建一个纯文本错误响应。
pub fn service_media_error_response(status: u16, message: &str) -> ServiceMediaResponse {
    ServiceMediaResponse {
        status,
        headers: vec![
            (
                String::from("access-control-allow-origin"),
                String::from("*"),
            ),
            (
                String::from("content-type"),
                String::from("text/plain; charset=utf-8"),
            ),
        ],
        body: message.as_bytes().to_vec(),
    }
}

/// Builds common headers shared by media success responses.
/// 构建媒体成功响应共享的公共响应头。
fn service_media_base_headers(mime_type: String) -> Vec<(String, String)> {
    vec![
        (
            String::from("access-control-allow-origin"),
            String::from("*"),
        ),
        (
            String::from("access-control-expose-headers"),
            String::from("content-range"),
        ),
        (String::from("accept-ranges"), String::from("bytes")),
        (String::from("cache-control"), String::from("no-store")),
        (String::from("content-type"), mime_type),
    ]
}

/// Infers a conservative MIME type from a generated media path extension.
/// 根据生成媒体路径扩展名推断保守 MIME 类型。
fn infer_service_media_asset_mime_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => String::from("image/png"),
        Some("jpg") | Some("jpeg") => String::from("image/jpeg"),
        Some("webp") => String::from("image/webp"),
        Some("gif") => String::from("image/gif"),
        Some("bmp") => String::from("image/bmp"),
        Some("svg") => String::from("image/svg+xml"),
        Some("mp4") => String::from("video/mp4"),
        Some("webm") => String::from("video/webm"),
        Some("mov") => String::from("video/quicktime"),
        Some("mkv") => String::from("video/x-matroska"),
        Some("mp3") => String::from("audio/mpeg"),
        Some("wav") => String::from("audio/wav"),
        Some("m4a") => String::from("audio/mp4"),
        Some("aac") => String::from("audio/aac"),
        Some("flac") => String::from("audio/flac"),
        Some("ogg") => String::from("audio/ogg"),
        Some("pdf") => String::from("application/pdf"),
        _ => String::from("application/octet-stream"),
    }
}
