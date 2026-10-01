// The Vulcan logo reproduced from the WebUI's PNG, pixel for pixel, as half-block terminal cells.
// 以半块终端字符逐像素复刻 WebUI 的 PNG 标志。
//
// Contract owner: `apps/vulcan-code-web/public/vulcan-logo.png` in vulcan-code, copied byte for byte
// into `assets/vulcan-logo.png` by `vulcan-tui-assets sync` (see `assets/sync.json`). Never edit the
// local copy; re-run the sync when the upstream logo changes.
// 契约归属：vulcan-code 的 `apps/vulcan-code-web/public/vulcan-logo.png`，由 `vulcan-tui-assets sync`
// 逐字节复制到 `assets/vulcan-logo.png`（见 `assets/sync.json`）。不得修改本地副本；上游标志变化
// 时重新执行同步。
//
// Each cell shows two vertical pixels with `▀` (top = foreground, bottom = background). Terminal
// cells are about twice as tall as wide, so a `cols`-wide logo uses `cols` × `cols` pixels in
// `cols / 2` rows and keeps the square tile square. Downsampling is an alpha-weighted box filter,
// and transparent corners blend into whatever background the logo sits on.
// 每个单元格用 `▀` 显示上下两个像素（上 = 前景，下 = 背景）。终端单元格高约为宽的两倍，因此
// `cols` 列宽的标志使用 `cols` × `cols` 像素、占 `cols / 2` 行，使方形图块保持方形。缩放采用按 alpha
// 加权的盒式滤波，透明圆角与所在背景混合。

use std::sync::{Arc, Mutex, OnceLock};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::theme::mix;

const LOGO_PNG: &[u8] = include_bytes!("../../assets/vulcan-logo.png");

/// Decoded source image as straight (non-premultiplied) RGBA.
/// 解码后的源图像，直通（非预乘）RGBA。
struct Image {
    width: usize,
    height: usize,
    rgba: Vec<[u8; 4]>,
}

fn decode(bytes: &[u8]) -> Image {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().expect("embedded logo is a valid PNG");
    let mut buffer = vec![
        0;
        reader
            .output_buffer_size()
            .expect("logo size fits in memory")
    ];
    let info = reader
        .next_frame(&mut buffer)
        .expect("embedded logo decodes");
    let bytes = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect(),
        png::ColorType::Rgb => bytes
            .chunks_exact(3)
            .map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => bytes
            .chunks_exact(2)
            .map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => bytes.iter().map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => unreachable!("normalize_to_color8 expands palettes"),
    };
    Image {
        width: info.width as usize,
        height: info.height as usize,
        rgba,
    }
}

fn source() -> &'static Image {
    static IMAGE: OnceLock<Image> = OnceLock::new();
    IMAGE.get_or_init(|| decode(LOGO_PNG))
}

/// The logo downsampled to a fixed cell width; cached per width.
/// 缩放到固定列宽的标志；按宽度缓存。
pub struct Logo {
    pub cols: u16,
    /// `cols` × `cols` pixels: straight RGB plus coverage (0..1).
    /// `cols` × `cols` 像素：直通 RGB 与覆盖率（0..1）。
    pixels: Vec<(u8, u8, u8, f32)>,
}

impl Logo {
    /// The logo `cols` cells wide (rounded up to even so rows are whole), from a small cache.
    /// 宽 `cols` 列的标志（向上取偶数以使行数为整数），来自小型缓存。
    pub fn new(cols: u16) -> Arc<Logo> {
        static CACHE: OnceLock<Mutex<Vec<Arc<Logo>>>> = OnceLock::new();
        let cols = (cols.max(2) + 1) & !1;
        let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
        let mut cache = cache.lock().expect("logo cache lock poisoned");
        if let Some(logo) = cache.iter().find(|l| l.cols == cols) {
            return logo.clone();
        }
        let logo = Arc::new(Logo::downsample(source(), cols));
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push(logo.clone());
        logo
    }

    fn downsample(image: &Image, cols: u16) -> Logo {
        let size = cols as usize;
        let mut pixels = Vec::with_capacity(size * size);
        for py in 0..size {
            let y0 = py * image.height / size;
            let y1 = ((py + 1) * image.height / size).max(y0 + 1);
            for px in 0..size {
                let x0 = px * image.width / size;
                let x1 = ((px + 1) * image.width / size).max(x0 + 1);
                // Alpha-weighted average so transparent pixels do not darken edges.
                // 按 alpha 加权平均，避免透明像素使边缘发暗。
                let (mut r, mut g, mut b, mut a) = (0f32, 0f32, 0f32, 0f32);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let [pr, pg, pb, pa] = image.rgba[y * image.width + x];
                        let alpha = pa as f32 / 255.0;
                        r += pr as f32 * alpha;
                        g += pg as f32 * alpha;
                        b += pb as f32 * alpha;
                        a += alpha;
                    }
                }
                let count = ((y1 - y0) * (x1 - x0)) as f32;
                let pixel = if a > 0.0 {
                    (
                        (r / a).round() as u8,
                        (g / a).round() as u8,
                        (b / a).round() as u8,
                        a / count,
                    )
                } else {
                    (0, 0, 0, 0.0)
                };
                pixels.push(pixel);
            }
        }
        Logo { cols, pixels }
    }

    pub fn rows(&self) -> u16 {
        self.cols / 2
    }

    fn color(&self, x: u16, y: u16, bg: Color, glow: f32) -> Option<Color> {
        let (r, g, b, coverage) = self.pixels[y as usize * self.cols as usize + x as usize];
        if coverage < 0.02 {
            return None;
        }
        let mut color = Color::Rgb(r, g, b);
        // Flame pixels (strongly orange) brighten with `glow` for the splash pulse.
        // 火焰像素（明显的橙色）随 `glow` 变亮，用于启动画面的呼吸效果。
        if glow > 0.0 && r > 180 && (r as i16 - g as i16) > 70 {
            color = mix(Color::Rgb(255, 196, 120), color, glow * 0.45);
        }
        Some(mix(color, bg, coverage))
    }

    /// Draws the logo with its top-left corner at `area`, clipped to it.
    /// 以 `area` 左上角为起点绘制标志，并裁剪到该区域内。
    pub fn render(&self, area: Rect, buf: &mut Buffer, bg: Color, glow: f32) {
        for row in 0..self.rows().min(area.height) {
            for col in 0..self.cols.min(area.width) {
                let top = self.color(col, row * 2, bg, glow);
                let bottom = self.color(col, row * 2 + 1, bg, glow);
                let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) else {
                    continue;
                };
                match (top, bottom) {
                    (None, None) => {
                        cell.set_char(' ').set_bg(bg);
                    }
                    (top, bottom) => {
                        cell.set_char('▀')
                            .set_fg(top.unwrap_or(bg))
                            .set_bg(bottom.unwrap_or(bg));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_decodes_and_keeps_tile_flame_and_chevron() {
        let image = source();
        assert!(
            image.width > 0 && image.width == image.height,
            "the WebUI logo is a square tile"
        );
        let logo = Logo::new(24);
        assert_eq!(logo.rows(), 12);
        // Corners are transparent (rounded tile); the tile, the white chevron and the orange flame
        // all survive downsampling.
        // 四角透明（圆角图块）；缩放后图块、白色箭头与橙色火焰都应保留。
        assert!(logo.pixels[0].3 < 0.5);
        assert!(
            logo.pixels
                .iter()
                .any(|p| p.3 > 0.9 && p.0 < 40 && p.1 < 40)
        );
        assert!(
            logo.pixels
                .iter()
                .any(|p| p.0 > 220 && p.1 > 220 && p.2 > 220)
        );
        assert!(
            logo.pixels
                .iter()
                .any(|p| p.0 > 200 && p.1 < 140 && p.2 < 90)
        );
    }
}
