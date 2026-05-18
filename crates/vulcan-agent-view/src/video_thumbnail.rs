//! In-memory supported video-thumbnail extraction and caching helpers shared by transcript cards and host-side media previews.
//! 供转录卡片与宿主媒体预览共享的、驻留内存的受支持视频缩略图提取与缓存辅助模块。

use iced::widget::image::Handle as ImageHandle;
use image::RgbaImage;
use mp4::{MediaType, Mp4Reader, TrackType};
use rust_h264::decoder::{Frame, OrderedDecoder};
use rust_h264::nal::{parse_annex_b, parse_avcc};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Maximum number of MP4 samples scanned while searching for one first decodable preview frame.
/// 搜索首个可解码预览帧时最多扫描的 MP4 sample 数量。
const VIDEO_THUMBNAIL_SAMPLE_SCAN_LIMIT: u32 = 120;

/// One decoded in-memory video thumbnail preview that can be rendered directly by iced without touching any sidecar files.
/// 一份无需依赖任何旁路文件、即可被 iced 直接渲染的内存视频缩略图预览。
#[derive(Debug, Clone, PartialEq)]
pub struct VideoThumbnailPreview {
    /// RGBA image handle consumable by iced image widgets and custom renderers.
    /// 可供 iced 图片部件与自绘渲染器消费的 RGBA 图像句柄。
    pub handle: ImageHandle,
    /// Intrinsic width-height ratio of the decoded preview image.
    /// 解码后预览图的原始宽高比。
    pub aspect_ratio: f32,
}

/// Cache entry retained for one previously requested video path so repeated transcript re-renders do not decode the same file again.
/// 为已请求过的视频路径保留的缓存条目，避免转录反复重绘时重复解码同一文件。
#[derive(Debug, Clone, PartialEq)]
enum VideoThumbnailCacheEntry {
    /// One successfully decoded preview image.
    /// 一份成功解码的预览图像。
    Available(VideoThumbnailPreview),
    /// One negative cache marker meaning the path is unsupported or decoding already failed.
    /// 一条负缓存标记，表示该路径不受支持或此前已经解码失败。
    Unavailable,
}

/// Global process-local cache storing previously decoded supported video thumbnails.
/// 保存既有受支持视频缩略图解码结果的进程级本地缓存。
static VIDEO_THUMBNAIL_CACHE: OnceLock<Mutex<HashMap<String, VideoThumbnailCacheEntry>>> =
    OnceLock::new();

/// Resolves one supported in-memory video thumbnail preview for the provided local media path, lazily decoding and caching it on the first request.
/// 为给定本地媒体路径解析一份受支持的内存视频缩略图预览，并在首次请求时惰性解码后写入缓存。
pub fn resolve_video_thumbnail_preview(
    local_path: &str,
    mime_type: Option<&str>,
) -> Option<VideoThumbnailPreview> {
    let normalized_path = local_path.trim();
    if normalized_path.is_empty() {
        return None;
    }
    if !is_supported_video_thumbnail_source(normalized_path, mime_type) {
        return None;
    }

    let cache = VIDEO_THUMBNAIL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .expect("video thumbnail cache should lock")
        .get(normalized_path)
        .cloned()
    {
        return match cached {
            VideoThumbnailCacheEntry::Available(preview) => Some(preview),
            VideoThumbnailCacheEntry::Unavailable => None,
        };
    }

    let decoded = decode_video_thumbnail_preview(Path::new(normalized_path)).ok();
    let cache_entry = decoded
        .clone()
        .map(VideoThumbnailCacheEntry::Available)
        .unwrap_or(VideoThumbnailCacheEntry::Unavailable);
    cache
        .lock()
        .expect("video thumbnail cache should lock for insert")
        .insert(normalized_path.to_string(), cache_entry);
    decoded
}

/// Returns whether the provided local media path falls inside the deliberately narrow MP4/H.264 support window handled by the built-in dynamic thumbnail decoder.
/// 返回给定本地媒体路径是否落在内置动态缩略图解码器刻意支持的狭窄 MP4/H.264 范围内。
fn is_supported_video_thumbnail_source(local_path: &str, mime_type: Option<&str>) -> bool {
    let path = Path::new(local_path);
    if !path.is_file() {
        return false;
    }

    if let Some(normalized_mime_type) = mime_type.map(str::trim).filter(|value| !value.is_empty()) {
        return normalized_mime_type.eq_ignore_ascii_case("video/mp4");
    }

    path.extension()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("mp4"))
}

/// Decodes the first displayable frame from one supported MP4/H.264 file and converts it into an iced-compatible RGBA thumbnail handle.
/// 从一条受支持的 MP4/H.264 文件中解码首个可显示帧，并将其转换为兼容 iced 的 RGBA 缩略图句柄。
fn decode_video_thumbnail_preview(video_path: &Path) -> Result<VideoThumbnailPreview, String> {
    let file = File::open(video_path).map_err(|error| {
        format!(
            "打开视频文件 `{}` 以提取动态缩略图失败：{}。",
            video_path.display(),
            error
        )
    })?;
    let file_len = file
        .metadata()
        .map_err(|error| {
            format!(
                "读取视频文件 `{}` 元数据失败：{}。",
                video_path.display(),
                error
            )
        })?
        .len();
    let mut reader = Mp4Reader::read_header(BufReader::new(file), file_len)
        .map_err(|error| format!("解析 MP4 头部 `{}` 失败：{}。", video_path.display(), error))?;
    let track_id = select_supported_video_track_id(&reader)?;
    let decoder_length_size = track_length_size(&reader, track_id)?;
    let mut decoder = OrderedDecoder::new();
    feed_supported_h264_parameter_sets_into_decoder(&reader, track_id, &mut decoder)?;

    let sample_count = reader
        .sample_count(track_id)
        .map_err(|error| format!("读取视频轨 `{track_id}` 的 sample 计数失败：{}。", error))?;
    let scan_limit = sample_count.min(VIDEO_THUMBNAIL_SAMPLE_SCAN_LIMIT);
    for sample_id in 1..=scan_limit {
        let Some(sample) = reader.read_sample(track_id, sample_id).map_err(|error| {
            format!(
                "读取视频轨 `{track_id}` 的 sample `{sample_id}` 失败：{}。",
                error
            )
        })?
        else {
            continue;
        };
        let nals = parse_avcc(sample.bytes.as_ref(), decoder_length_size);
        if nals.is_empty() {
            continue;
        }
        for nal in &nals {
            let decoded_frames = decoder.decode_nal(nal).map_err(|error| {
                format!(
                    "解码视频轨 `{track_id}` 的 sample `{sample_id}` 失败：{}。",
                    error
                )
            })?;
            if let Some(frame) = decoded_frames.into_iter().next() {
                return frame_to_thumbnail_preview(&frame);
            }
        }
    }

    if let Some(frame) = decoder.flush().into_iter().next() {
        return frame_to_thumbnail_preview(&frame);
    }

    Err(format!(
        "视频文件 `{}` 在前 {} 个 sample 中未产出可解码首帧。",
        video_path.display(),
        scan_limit
    ))
}

/// Selects the first MP4 track that is simultaneously typed as video and identified as H.264/AVC.
/// 选择第一条同时标记为视频轨且被识别为 H.264/AVC 的 MP4 轨道。
fn select_supported_video_track_id<R: std::io::Read + std::io::Seek>(
    reader: &Mp4Reader<R>,
) -> Result<u32, String> {
    reader
        .tracks()
        .iter()
        .find_map(|(track_id, track)| {
            let is_video_track = track.track_type().ok() == Some(TrackType::Video);
            let is_h264_track = track.media_type().ok() == Some(MediaType::H264);
            (is_video_track && is_h264_track).then_some(*track_id)
        })
        .ok_or_else(|| String::from("当前视频未包含受支持的 MP4/H.264 视频轨。"))
}

/// Resolves the AVCC length-prefix byte count declared by the selected MP4 H.264 track.
/// 解析所选 MP4 H.264 视频轨声明的 AVCC 长度前缀字节数。
fn track_length_size<R: std::io::Read + std::io::Seek>(
    reader: &Mp4Reader<R>,
    track_id: u32,
) -> Result<usize, String> {
    let track = reader
        .tracks()
        .get(&track_id)
        .ok_or_else(|| format!("找不到视频轨 `{track_id}`。"))?;
    let avcc = track
        .trak
        .mdia
        .minf
        .stbl
        .stsd
        .avc1
        .as_ref()
        .map(|box_value| &box_value.avcc)
        .ok_or_else(|| format!("视频轨 `{track_id}` 缺少 avcC 配置。"))?;
    Ok((usize::from(avcc.length_size_minus_one)) + 1)
}

/// Feeds the selected track's SPS/PPS parameter sets into the decoder through a synthetic Annex B wrapper so MP4-contained AVCC samples can begin decoding immediately.
/// 通过合成的 Annex B 包装将所选轨道的 SPS/PPS 参数集送入解码器，使 MP4 内 AVCC sample 可以立即开始解码。
fn feed_supported_h264_parameter_sets_into_decoder<R: std::io::Read + std::io::Seek>(
    reader: &Mp4Reader<R>,
    track_id: u32,
    decoder: &mut OrderedDecoder,
) -> Result<(), String> {
    let track = reader
        .tracks()
        .get(&track_id)
        .ok_or_else(|| format!("找不到视频轨 `{track_id}`。"))?;
    for parameter_set in [
        track
            .sequence_parameter_set()
            .map_err(|error| format!("视频轨 `{track_id}` 缺少 SPS 参数集：{}。", error))?,
        track
            .picture_parameter_set()
            .map_err(|error| format!("视频轨 `{track_id}` 缺少 PPS 参数集：{}。", error))?,
    ] {
        let mut annex_b = Vec::with_capacity(parameter_set.len() + 4);
        annex_b.extend_from_slice(&[0, 0, 0, 1]);
        annex_b.extend_from_slice(parameter_set);
        for nal in parse_annex_b(&annex_b) {
            let _ = decoder
                .decode_nal(&nal)
                .map_err(|error| format!("初始化视频轨 `{track_id}` 参数集失败：{}。", error))?;
        }
    }
    Ok(())
}

/// Converts one decoded YUV420 frame into a cached RGBA thumbnail preview and its aspect ratio.
/// 将单帧解码后的 YUV420 图像转换为缓存用 RGBA 缩略图预览及其宽高比。
fn frame_to_thumbnail_preview(frame: &Frame) -> Result<VideoThumbnailPreview, String> {
    let mut rgba_image = frame_to_rgba_image(frame)?;
    decorate_video_thumbnail_with_play_overlay(&mut rgba_image);
    let width = rgba_image.width();
    let height = rgba_image.height();
    let aspect_ratio = width as f32 / height.max(1) as f32;
    Ok(VideoThumbnailPreview {
        handle: ImageHandle::from_rgba(width, height, rgba_image.into_raw()),
        aspect_ratio,
    })
}

/// Converts one decoded YUV420 frame into an RGBA raster image using the common limited-range BT.601 transform, which is sufficient for lightweight transcript thumbnails.
/// 使用常见的有限范围 BT.601 变换将单帧解码后的 YUV420 图像转换为 RGBA 光栅图，这对轻量转录缩略图已足够。
fn frame_to_rgba_image(frame: &Frame) -> Result<RgbaImage, String> {
    if frame.width == 0 || frame.height == 0 {
        return Err(String::from("解码首帧尺寸无效。"));
    }

    let width = frame.width as usize;
    let height = frame.height as usize;
    let expected_luma_len = width.saturating_mul(height);
    let expected_chroma_len = width.saturating_mul(height) / 4;
    if frame.y.len() < expected_luma_len
        || frame.u.len() < expected_chroma_len
        || frame.v.len() < expected_chroma_len
    {
        return Err(String::from("解码首帧像素平面长度不足。"));
    }

    let mut image = RgbaImage::new(frame.width, frame.height);
    for y in 0..height {
        for x in 0..width {
            let luma_index = y * width + x;
            let chroma_index = (y / 2) * (width / 2) + (x / 2);
            let rgba = yuv420_to_rgba_pixel(
                frame.y[luma_index],
                frame.u[chroma_index],
                frame.v[chroma_index],
            );
            image.put_pixel(x as u32, y as u32, rgba);
        }
    }

    Ok(image)
}

/// Paints one translucent centered play affordance directly into the decoded preview frame so video cards always keep a visible playback cue even when later renderer layers reorder image and quad draws.
/// 直接在解码后的预览帧中绘制半透明居中播放提示，从而让视频卡片即使在后续渲染层对图片与几何绘制重排时也能稳定保留可见播放提示。
fn decorate_video_thumbnail_with_play_overlay(image: &mut RgbaImage) {
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return;
    }

    let radius = ((width.min(height) as f32) * 0.18).clamp(20.0, 36.0);
    let center_x = width as f32 / 2.0;
    let center_y = height as f32 / 2.0;
    let border_alpha = 228u8;
    let fill_alpha = 84u8;
    let border_thickness = (radius * 0.06).max(1.2);

    for pixel_y in 0..height {
        for pixel_x in 0..width {
            let dx = pixel_x as f32 + 0.5 - center_x;
            let dy = pixel_y as f32 + 0.5 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > radius {
                continue;
            }
            let overlay = if distance >= (radius - border_thickness).max(0.0) {
                image::Rgba([255, 255, 255, border_alpha])
            } else {
                image::Rgba([255, 255, 255, fill_alpha])
            };
            blend_rgba_pixel(image.get_pixel_mut(pixel_x, pixel_y), overlay);
        }
    }

    let triangle_height = radius * 0.92;
    let triangle_width = triangle_height * 0.78;
    let triangle_left = center_x - triangle_width * 0.30;
    let triangle_top = center_y - triangle_height / 2.0;
    let triangle_right = triangle_left + triangle_width;
    let triangle_mid_y = center_y;
    for pixel_y in 0..height {
        for pixel_x in 0..width {
            let pixel_center = (pixel_x as f32 + 0.5, pixel_y as f32 + 0.5);
            if point_inside_triangle(
                pixel_center,
                (triangle_left, triangle_top),
                (triangle_left, triangle_top + triangle_height),
                (triangle_right, triangle_mid_y),
            ) {
                blend_rgba_pixel(
                    image.get_pixel_mut(pixel_x, pixel_y),
                    image::Rgba([255, 255, 255, 248]),
                );
            }
        }
    }
}

/// Blends one RGBA source color over the destination pixel using straight-alpha compositing so the overlay stays crisp without discarding the thumbnail underneath.
/// 使用直通 Alpha 合成将源 RGBA 颜色叠加到目标像素上，从而在保留底层缩略图的同时保持覆盖层清晰。
fn blend_rgba_pixel(destination: &mut image::Rgba<u8>, source: image::Rgba<u8>) {
    let source_alpha = source[3] as f32 / 255.0;
    if source_alpha <= f32::EPSILON {
        return;
    }
    let destination_alpha = destination[3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    for channel_index in 0..3 {
        let source_channel = source[channel_index] as f32 / 255.0;
        let destination_channel = destination[channel_index] as f32 / 255.0;
        let output_channel = if output_alpha <= f32::EPSILON {
            0.0
        } else {
            (source_channel * source_alpha
                + destination_channel * destination_alpha * (1.0 - source_alpha))
                / output_alpha
        };
        destination[channel_index] = (output_channel * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    destination[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Returns whether one point lies inside the provided play-triangle geometry by comparing signed edge orientations.
/// 通过比较有向边符号判断一个点是否位于给定播放三角形几何内部。
fn point_inside_triangle(
    point: (f32, f32),
    vertex_a: (f32, f32),
    vertex_b: (f32, f32),
    vertex_c: (f32, f32),
) -> bool {
    let cross_ab = triangle_edge_cross(point, vertex_a, vertex_b);
    let cross_bc = triangle_edge_cross(point, vertex_b, vertex_c);
    let cross_ca = triangle_edge_cross(point, vertex_c, vertex_a);
    let has_negative = cross_ab < 0.0 || cross_bc < 0.0 || cross_ca < 0.0;
    let has_positive = cross_ab > 0.0 || cross_bc > 0.0 || cross_ca > 0.0;
    !(has_negative && has_positive)
}

/// Computes one oriented edge cross-product used by the point-in-triangle helper.
/// 计算点三角形判定辅助逻辑使用的单条有向边叉积值。
fn triangle_edge_cross(point: (f32, f32), edge_start: (f32, f32), edge_end: (f32, f32)) -> f32 {
    (point.0 - edge_end.0) * (edge_start.1 - edge_end.1)
        - (edge_start.0 - edge_end.0) * (point.1 - edge_end.1)
}

/// Converts one YUV420 pixel triplet into an RGBA pixel using an inexpensive broadcast-range matrix.
/// 使用轻量广播级矩阵将单个 YUV420 像素三元组转换为 RGBA 像素。
fn yuv420_to_rgba_pixel(y: u8, u: u8, v: u8) -> image::Rgba<u8> {
    let luma = (i32::from(y) - 16).max(0);
    let chroma_u = i32::from(u) - 128;
    let chroma_v = i32::from(v) - 128;
    let red = clamp_color_channel((298 * luma + 409 * chroma_v + 128) >> 8);
    let green = clamp_color_channel((298 * luma - 100 * chroma_u - 208 * chroma_v + 128) >> 8);
    let blue = clamp_color_channel((298 * luma + 516 * chroma_u + 128) >> 8);
    image::Rgba([red, green, blue, 255])
}

/// Clamps one signed color-channel accumulator into the `u8` range expected by RGBA raster buffers.
/// 将单个有符号色彩通道累加值钳制到 RGBA 光栅缓冲区期望的 `u8` 范围。
fn clamp_color_channel(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

#[cfg(test)]
mod tests {
    use super::{decorate_video_thumbnail_with_play_overlay, is_supported_video_thumbnail_source};
    use image::RgbaImage;
    use std::fs;

    /// Verifies only MP4 inputs are routed through the built-in dynamic thumbnail decoder while other video containers continue to use the placeholder-and-system-player fallback.
    /// 验证只有 MP4 输入会经过内置动态缩略图解码器，其余视频容器继续走占位图与系统播放器兜底链路。
    #[test]
    fn only_mp4_video_sources_are_marked_as_supported() {
        let temporary_directory = std::env::temp_dir();
        let video_path = temporary_directory.join("vulcan-agent-view-test-video.mp4");
        fs::write(&video_path, [0_u8, 1, 2, 3]).expect("temporary MP4 should write");

        assert!(is_supported_video_thumbnail_source(
            &video_path.to_string_lossy(),
            Some("video/mp4")
        ));
        assert!(!is_supported_video_thumbnail_source(
            &video_path.to_string_lossy(),
            Some("video/webm")
        ));
        assert!(!is_supported_video_thumbnail_source(
            &video_path.with_extension("mov").to_string_lossy(),
            Some("video/quicktime")
        ));

        let _ = fs::remove_file(video_path);
    }

    /// Verifies the decoded video preview receives a baked-in play cue so transcript rendering no longer depends on a separate overlay draw pass remaining visible.
    /// 验证解码后的视频预览会获得烘焙进去的播放提示，从而让转录渲染不再依赖额外覆盖层绘制通道必须保持可见。
    #[test]
    fn decoded_video_thumbnail_receives_baked_play_overlay() {
        let mut image = RgbaImage::from_pixel(240, 136, image::Rgba([12, 24, 36, 255]));
        let original_center = *image.get_pixel(120, 68);
        let original_ring = *image.get_pixel(120, 44);

        decorate_video_thumbnail_with_play_overlay(&mut image);

        let decorated_center = *image.get_pixel(120, 68);
        let decorated_ring = *image.get_pixel(120, 44);

        assert_ne!(decorated_center, original_center);
        assert_ne!(decorated_ring, original_ring);
        assert!(decorated_center[0] > original_center[0]);
        assert!(decorated_ring[0] > original_ring[0]);
    }
}
