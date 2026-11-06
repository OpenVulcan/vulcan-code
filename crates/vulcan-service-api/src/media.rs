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
        .map_err(|error| format!("Failed to decode the media path: {error}"))?;
    let path_text = String::from_utf8(path_bytes)
        .map_err(|error| format!("The media path is not valid UTF-8: {error}"))?;
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
            "The media path is not an allowed media resource type.",
        ));
    }

    let mut file = match File::open(&media_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(service_media_error_response(
                404,
                "The media file does not exist.",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(service_media_error_response(
                403,
                "No permission to read the media file.",
            ));
        }
        Err(error) => return Err(format!("Failed to open the media file: {error}")),
    };
    let file_len = file
        .metadata()
        .map_err(|error| format!("Failed to read media file metadata: {error}"))?
        .len();
    let mime_type = infer_service_media_asset_mime_type(&media_path);
    // SVG is an active document: opened in its own tab it would run script at the Core origin.
    // SVG 是活动文档：在独立标签页打开时会以 Core 来源运行脚本。
    let active_document = mime_type == "image/svg+xml";
    let mut headers = service_media_base_headers(mime_type);
    headers.push((
        String::from("x-content-type-options"),
        String::from("nosniff"),
    ));
    if active_document {
        headers.push((
            String::from("content-security-policy"),
            String::from("sandbox; default-src 'none'; style-src 'unsafe-inline'"),
        ));
    }

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
            .map_err(|error| format!("Failed to seek to the media range: {error}"))?;
        file.take(range.length)
            .read_to_end(&mut body)
            .map_err(|error| format!("Failed to read the media range: {error}"))?;
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
        .map_err(|error| format!("Failed to read the media file: {error}"))?;
    headers.push((String::from("content-length"), file_len.to_string()));
    Ok(ServiceMediaResponse {
        status: 200,
        headers,
        body,
    })
}

/// Builds one controlled immutable project-resource response from validated bytes.
/// 从已校验字节构建一个受控的不可变项目资源响应。
///
/// # Parameters
/// 参数
/// `bytes` is the complete digest-validated entity, `mime_type` is catalog metadata,
/// `is_head` selects header-only delivery, and `range_header` carries one optional byte range.
/// `bytes` 是完整且已通过摘要校验的实体，`mime_type` 是目录元数据，`is_head` 选择仅响应头传输，
/// `range_header` 携带一个可选字节范围。
///
/// # Returns
/// 返回值
/// Returns a no-store response that never exposes a host filesystem path.
/// 返回一个不缓存且绝不暴露宿主文件系统路径的响应。
pub fn build_service_project_resource_response(
    bytes: &[u8],
    mime_type: Option<&str>,
    is_head: bool,
    range_header: Option<&str>,
) -> Result<ServiceMediaResponse, String> {
    let file_len = u64::try_from(bytes.len())
        .map_err(|_| String::from("project resource byte length exceeds the supported range"))?;
    let content_type = safe_project_resource_mime_type(mime_type);
    let mut headers = service_media_base_headers(content_type.clone());
    headers.push((
        String::from("content-disposition"),
        if is_inline_project_resource_mime_type(&content_type) {
            String::from("inline")
        } else {
            String::from("attachment")
        },
    ));
    headers.push((
        String::from("x-content-type-options"),
        String::from("nosniff"),
    ));
    headers.push((
        String::from("content-security-policy"),
        String::from("sandbox; default-src 'none'"),
    ));

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
        let range_start = usize::try_from(range.start)
            .map_err(|_| String::from("project resource range start exceeds host limits"))?;
        let range_end_exclusive_u64 = range
            .start
            .checked_add(range.length)
            .ok_or_else(|| String::from("project resource range end overflowed"))?;
        let range_end_exclusive = usize::try_from(range_end_exclusive_u64)
            .map_err(|_| String::from("project resource range end exceeds host limits"))?;
        let body = bytes
            .get(range_start..range_end_exclusive)
            .ok_or_else(|| String::from("project resource range is outside validated bytes"))?
            .to_vec();
        let range_end = range_end_exclusive_u64 - 1;
        headers.push((
            String::from("content-range"),
            format!("bytes {}-{range_end}/{file_len}", range.start),
        ));
        headers.push((String::from("content-length"), body.len().to_string()));
        return Ok(ServiceMediaResponse {
            status: 206,
            headers,
            body,
        });
    }

    headers.push((String::from("content-length"), file_len.to_string()));
    Ok(ServiceMediaResponse {
        status: 200,
        headers,
        body: bytes.to_vec(),
    })
}

/// Normalizes one catalog MIME value without trusting malformed header content.
/// 规范化一个目录 MIME 值，且不信任格式错误的响应头内容。
fn safe_project_resource_mime_type(mime_type: Option<&str>) -> String {
    let Some(mime_type) = mime_type else {
        return String::from("application/octet-stream");
    };
    if mime_type.is_empty()
        || mime_type.len() > 127
        || mime_type.trim() != mime_type
        || mime_type.chars().any(char::is_control)
        || mime_type.contains(';')
    {
        return String::from("application/octet-stream");
    }
    let Some((kind, subtype)) = mime_type.split_once('/') else {
        return String::from("application/octet-stream");
    };
    if kind.is_empty()
        || subtype.is_empty()
        || mime_type.matches('/').count() != 1
        || !mime_type.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'/' | b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_'
                )
        })
    {
        return String::from("application/octet-stream");
    }
    mime_type.to_ascii_lowercase()
}

/// Returns whether one normalized resource MIME is safe for inline browser rendering.
/// 返回一个规范化资源 MIME 是否可安全用于浏览器内联渲染。
fn is_inline_project_resource_mime_type(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/png"
            | "image/jpeg"
            | "image/webp"
            | "image/gif"
            | "image/bmp"
            | "audio/mpeg"
            | "audio/wav"
            | "audio/mp4"
            | "audio/aac"
            | "audio/flac"
            | "audio/ogg"
            | "audio/opus"
            | "video/mp4"
            | "video/mpeg"
            | "video/webm"
            | "video/quicktime"
            | "video/x-m4v"
            | "video/x-matroska"
            | "video/x-msvideo"
            | "video/x-flv"
            | "video/x-ms-wmv"
            | "video/3gpp"
            | "video/3gpp2"
            | "application/pdf"
    )
}

/// Returns whether a media path belongs to the generated attachment storage tree.
/// 返回媒体路径是否属于生成附件存储树。
fn is_allowed_generated_service_media_path(path: &Path) -> bool {
    // `..` would leave the attachment tree after the marker components matched.
    // `..` 会在标记组件匹配后离开附件树。
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return false;
    }
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
            | Some("mpeg")
            | Some("mpg")
            | Some("webm")
            | Some("mov")
            | Some("m4v")
            | Some("mkv")
            | Some("avi")
            | Some("flv")
            | Some("wmv")
            | Some("3gp")
            | Some("3gpp")
            | Some("3g2")
            | Some("mp3")
            | Some("wav")
            | Some("m4a")
            | Some("aac")
            | Some("flac")
            | Some("ogg")
            | Some("pdf")
            | Some("docx")
    )
}

/// Parses one bounded HTTP range for media playback.
/// 解析一个有上限的媒体播放 HTTP 范围。
fn single_service_media_http_range(range_header: &str, file_len: u64) -> Result<HttpRange, String> {
    let mut ranges = HttpRange::parse(range_header, file_len)
        .map_err(|error| format!("Invalid media Range request: {error:?}"))?;
    let Some(mut range) = ranges.drain(..).next() else {
        return Err(String::from("The media Range request is empty."));
    };
    if range.start >= file_len || range.length == 0 {
        return Err(String::from("The media Range request is out of bounds."));
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
        Some("mpeg") | Some("mpg") => String::from("video/mpeg"),
        Some("webm") => String::from("video/webm"),
        Some("mov") => String::from("video/quicktime"),
        Some("m4v") => String::from("video/x-m4v"),
        Some("mkv") => String::from("video/x-matroska"),
        Some("avi") => String::from("video/x-msvideo"),
        Some("flv") => String::from("video/x-flv"),
        Some("wmv") => String::from("video/x-ms-wmv"),
        Some("3gp") | Some("3gpp") => String::from("video/3gpp"),
        Some("3g2") => String::from("video/3gpp2"),
        Some("mp3") => String::from("audio/mpeg"),
        Some("wav") => String::from("audio/wav"),
        Some("m4a") => String::from("audio/mp4"),
        Some("aac") => String::from("audio/aac"),
        Some("flac") => String::from("audio/flac"),
        Some("ogg") => String::from("audio/ogg"),
        Some("pdf") => String::from("application/pdf"),
        Some("docx") => {
            String::from("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        }
        _ => String::from("application/octet-stream"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns one response header value for exact assertions.
    /// 返回一个响应头值以进行精确断言。
    fn response_header<'response>(
        response: &'response ServiceMediaResponse,
        name: &str,
    ) -> Option<&'response str> {
        response
            .headers
            .iter()
            .find_map(|(candidate, value)| (candidate == name).then_some(value.as_str()))
    }

    /// Verifies validated raster resources are served inline without exposing storage paths.
    /// 验证已校验的光栅资源以内联方式提供且不暴露存储路径。
    #[test]
    fn project_resource_response_serves_safe_inline_content() {
        let response = build_service_project_resource_response(
            b"validated-image",
            Some("image/png"),
            false,
            None,
        )
        .expect("safe resource should build");

        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"validated-image");
        assert_eq!(
            response_header(&response, "content-type"),
            Some("image/png")
        );
        assert_eq!(
            response_header(&response, "content-disposition"),
            Some("inline")
        );
        assert_eq!(
            response_header(&response, "cache-control"),
            Some("no-store")
        );
        assert_eq!(
            response_header(&response, "x-content-type-options"),
            Some("nosniff")
        );
        assert!(
            response
                .headers
                .iter()
                .all(|(_, value)| !value.contains('\\') && !value.contains(':'))
        );
    }

    /// Verifies unsafe catalog MIME values become opaque attachment downloads.
    /// 验证不安全的目录 MIME 值会转为不透明附件下载。
    #[test]
    fn project_resource_response_normalizes_unsafe_mime() {
        let response = build_service_project_resource_response(
            b"payload",
            Some("text/html; charset=utf-8\r\nx-injected: yes"),
            false,
            None,
        )
        .expect("unsafe MIME should be normalized instead of copied");

        assert_eq!(
            response_header(&response, "content-type"),
            Some("application/octet-stream")
        );
        assert_eq!(
            response_header(&response, "content-disposition"),
            Some("attachment")
        );
    }

    /// Verifies one bounded byte range and HEAD response share validated entity length facts.
    /// 验证单一有界字节范围与 HEAD 响应共享已校验实体长度事实。
    #[test]
    fn project_resource_response_supports_range_and_head() {
        let ranged = build_service_project_resource_response(
            b"0123456789",
            Some("video/mp4"),
            false,
            Some("bytes=2-5"),
        )
        .expect("bounded range should build");
        let head = build_service_project_resource_response(
            b"0123456789",
            Some("video/mp4"),
            true,
            Some("bytes=2-5"),
        )
        .expect("HEAD should build from the full entity");

        assert_eq!(ranged.status, 206);
        assert_eq!(ranged.body, b"2345");
        assert_eq!(
            response_header(&ranged, "content-range"),
            Some("bytes 2-5/10")
        );
        assert_eq!(response_header(&ranged, "content-length"), Some("4"));
        assert_eq!(head.status, 200);
        assert!(head.body.is_empty());
        assert_eq!(response_header(&head, "content-length"), Some("10"));
    }

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
        // EscapingAttachment matches the marker components, then climbs out of the tree.
        // EscapingAttachment 先匹配标记组件，再爬出附件树。
        let escaping_attachment = PathBuf::from("workspace")
            .join(".vulcan")
            .join("code")
            .join("attachments")
            .join("..")
            .join("..")
            .join("..")
            .join("secrets.json");
        assert!(!is_allowed_service_media_asset_path(&escaping_attachment));
    }

    /// Verifies served SVG files cannot run script and no media response is MIME-sniffed.
    /// 验证提供的 SVG 文件无法运行脚本，且任何媒体响应都不会被 MIME 嗅探。
    #[test]
    fn svg_media_response_is_sandboxed_and_png_is_not() {
        let directory = std::env::temp_dir().join(format!(
            "vulcan-media-svg-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&directory).expect("temp directory should be created");
        let svg_path = directory.join("image.svg");
        let png_path = directory.join("image.png");
        fs::write(
            &svg_path,
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>",
        )
        .expect("svg should be written");
        fs::write(&png_path, b"\x89PNG\r\n\x1a\n").expect("png should be written");

        let svg = build_service_media_file_response(svg_path, false, None)
            .expect("svg response should build");
        let png = build_service_media_file_response(png_path, false, None)
            .expect("png response should build");
        let _ = fs::remove_dir_all(&directory);

        assert_eq!(svg.status, 200);
        assert!(
            response_header(&svg, "content-security-policy")
                .is_some_and(|policy| policy.contains("sandbox")),
            "SVG responses must carry a sandboxing CSP"
        );
        assert_eq!(
            response_header(&svg, "x-content-type-options"),
            Some("nosniff")
        );
        assert_eq!(png.status, 200);
        assert_eq!(response_header(&png, "content-security-policy"), None);
        assert_eq!(
            response_header(&png, "x-content-type-options"),
            Some("nosniff")
        );
    }

    /// Verifies the service media route recognizes every added Kimi video extension and canonical MIME type.
    /// 验证服务媒体路由识别所有新增 Kimi 视频扩展名及其规范 MIME 类型。
    ///
    /// # Returns
    /// 返回值
    /// Returns normally after every added extension is accepted and mapped without MIME ambiguity.
    /// 所有新增扩展名均被接受并无歧义地映射 MIME 后正常返回。
    #[test]
    fn kimi_video_extensions_are_served_with_canonical_mime_types() {
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.mpeg")),
            "video/mpeg"
        );
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.mpg")),
            "video/mpeg"
        );
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.avi")),
            "video/x-msvideo"
        );
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.flv")),
            "video/x-flv"
        );
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.wmv")),
            "video/x-ms-wmv"
        );
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.3gp")),
            "video/3gpp"
        );
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.3gpp")),
            "video/3gpp"
        );
        assert!(has_supported_service_media_asset_extension(Path::new(
            "clip.3gp"
        )));
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.m4v")),
            "video/x-m4v"
        );
        assert_eq!(
            infer_service_media_asset_mime_type(Path::new("clip.3g2")),
            "video/3gpp2"
        );
    }
}
