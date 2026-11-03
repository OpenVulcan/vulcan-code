//! File packs `packs/<sha256>.pack`: concatenated zstd frames, one per file, with no header.
//! 文件包 `packs/<sha256>.pack`：多个 zstd 帧直接拼接，每个文件一帧，没有头部。
//!
//! Frame offsets live in the component indexes, so a client can fetch one frame with an HTTP range
//! request. The producer only packs content no published index already holds, and fills packs in
//! the order files are added (component, then directory, then path) up to a target size; a frame
//! at or above the target gets a pack of its own, and a short last pack is not padded.
//!
//! 帧偏移记录在组件索引中，客户端因此可以用 HTTP Range 请求单独取一帧。生成器只打包已发布索引中没有的
//! 内容，并按文件加入顺序（组件、目录、路径）填充文件包直到目标大小；达到或超过目标大小的帧独占一个包，
//! 最后一个不满的包不做填充。

use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use zstd::stream::read::Decoder;
use zstd::zstd_safe::CParameter;

use crate::digest::Sha256Digest;
use crate::error::{FormatError, FormatResult};
use crate::layout::pack_key;
use crate::limits::{
    COMPRESSION_LEVEL, DEFAULT_PACK_TARGET_BYTES, LONG_MODE_THRESHOLD_BYTES, MAX_PACK_BYTES,
    MAX_WINDOW_LOG,
};

/// Where one file's frame is stored.
/// 某文件的帧存放位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileLocation {
    /// Digest of the pack.
    /// 文件包摘要。
    pub pack: Sha256Digest,
    /// Offset of the frame in the pack.
    /// 帧在文件包中的偏移。
    pub offset: u64,
    /// Length of the frame.
    /// 帧的长度。
    pub length: u64,
}

/// Compresses `content` into one zstd frame that records its content size.
/// 将 `content` 压缩为一个记录内容大小的 zstd 帧。
///
/// Files above [`LONG_MODE_THRESHOLD_BYTES`] use long-distance matching with a window that covers
/// the whole file, capped at [`MAX_WINDOW_LOG`] so every decoder accepts it.
/// 大于 [`LONG_MODE_THRESHOLD_BYTES`] 的文件启用长距离匹配，窗口覆盖整个文件，上限为
/// [`MAX_WINDOW_LOG`]，保证所有解码器都能接受。
pub fn compress_frame(content: &[u8], level: i32) -> FormatResult<Vec<u8>> {
    let fail = |source| FormatError::io("compressing a frame", source);
    let mut compressor = zstd::bulk::Compressor::new(level).map_err(fail)?;
    let size = content.len() as u64;
    if size > LONG_MODE_THRESHOLD_BYTES {
        let covering_log = u64::BITS - (size - 1).leading_zeros();
        compressor
            .set_parameter(CParameter::EnableLongDistanceMatching(true))
            .map_err(fail)?;
        compressor
            .set_parameter(CParameter::WindowLog(covering_log.min(MAX_WINDOW_LOG)))
            .map_err(fail)?;
    }
    compressor.compress(content).map_err(fail)
}

/// Opens a decoder for exactly one frame whose window is at most [`MAX_WINDOW_LOG`].
/// 打开只解码一个帧、窗口不超过 [`MAX_WINDOW_LOG`] 的解码器。
fn frame_decoder<R: BufRead>(reader: R) -> FormatResult<Decoder<'static, R>> {
    let fail = |source| FormatError::io("opening a frame", source);
    let mut decoder = Decoder::with_buffer(reader).map_err(fail)?;
    decoder.window_log_max(MAX_WINDOW_LOG).map_err(fail)?;
    Ok(decoder.single_frame())
}

/// Decompresses one frame held in memory, refusing output beyond `limit` bytes.
/// 解压内存中的一个帧，输出超过 `limit` 字节即拒绝。
pub(crate) fn decompress_bounded(
    what: &'static str,
    frame: &[u8],
    limit: u64,
) -> FormatResult<Vec<u8>> {
    let mut output = Vec::new();
    frame_decoder(frame)?
        .take(limit + 1)
        .read_to_end(&mut output)
        .map_err(|source| FormatError::io(what, source))?;
    if output.len() as u64 > limit {
        return Err(FormatError::TooLarge { what, limit });
    }
    Ok(output)
}

/// Decompresses one file's frame from `frame` into `output`, checking its size and digest.
/// 从 `frame` 解压某文件的帧并写入 `output`，同时校验其大小与摘要。
///
/// `frame` should yield exactly the frame's bytes, for example a range response or a
/// [`Read::take`] over the pack. Output beyond `size` stops the copy at once; on any failure the
/// caller must discard what was written.
/// `frame` 应恰好提供该帧的字节，例如一个 Range 响应或对文件包的 [`Read::take`]。输出超过 `size` 时立即
/// 停止复制；任何失败后调用方都必须丢弃已写入的内容。
pub fn extract_frame<R: Read, W: Write>(
    frame: R,
    size: u64,
    sha256: Sha256Digest,
    mut output: W,
) -> FormatResult<()> {
    let mut decoder = frame_decoder(std::io::BufReader::new(frame))?;
    let mut hasher = Sha256::new();
    let mut written = 0_u64;
    let mut buffer = vec![0_u8; 64 << 10];
    loop {
        let read = decoder
            .read(&mut buffer)
            .map_err(|source| FormatError::io("decompressing a file", source))?;
        if read == 0 {
            break;
        }
        written += read as u64;
        if written > size {
            return Err(FormatError::SizeMismatch {
                what: "file",
                expected: size,
                actual: written,
            });
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(|source| FormatError::io("writing a file", source))?;
    }
    if written != size {
        return Err(FormatError::SizeMismatch {
            what: "file",
            expected: size,
            actual: written,
        });
    }
    sha256.expect_eq("file", Sha256Digest::finish(hasher))
}

/// What a finished [`PackBuilder`] produced.
/// [`PackBuilder`] 完成后的产物。
#[derive(Debug, Default)]
pub struct PackOutcome {
    /// Location of every file added, and of every previously published file.
    /// 每个已加入文件以及每个此前已发布文件的位置。
    pub locations: HashMap<Sha256Digest, FileLocation>,
    /// Digests of the packs written, in the order they were sealed.
    /// 已写入文件包的摘要，按封包顺序排列。
    pub new_packs: Vec<Sha256Digest>,
}

/// Packs new file contents into `<output>/packs/<sha256>.pack`; used by the release tooling.
/// 将新的文件内容打包为 `<output>/packs/<sha256>.pack`；供发布工具使用。
pub struct PackBuilder {
    /// Directory receiving the object layout.
    /// 接收对象布局的目录。
    output: PathBuf,
    /// Size the open pack is filled up to.
    /// 打开中的文件包填充到的大小。
    target_bytes: u64,
    /// zstd level of each frame.
    /// 每个帧的 zstd 级别。
    level: i32,
    /// Locations of published and sealed contents.
    /// 已发布与已封包内容的位置。
    sealed: HashMap<Sha256Digest, FileLocation>,
    /// Bytes of the open pack.
    /// 打开中文件包的字节。
    open: Vec<u8>,
    /// Frames of the open pack: digest, offset and length.
    /// 打开中文件包的帧：摘要、偏移与长度。
    open_frames: HashMap<Sha256Digest, (u64, u64)>,
    /// Packs written so far.
    /// 已写入的文件包。
    new_packs: Vec<Sha256Digest>,
}

impl PackBuilder {
    /// Starts a builder writing below `output`; `published` holds the location of every file
    /// content an already published index references, which is never packed again.
    /// 创建写入 `output` 之下的生成器；`published` 记录已发布索引引用的每份文件内容的位置，这些内容不会
    /// 再次打包。
    pub fn new(output: impl Into<PathBuf>, published: HashMap<Sha256Digest, FileLocation>) -> Self {
        Self {
            output: output.into(),
            target_bytes: DEFAULT_PACK_TARGET_BYTES,
            level: COMPRESSION_LEVEL,
            sealed: published,
            open: Vec::new(),
            open_frames: HashMap::new(),
            new_packs: Vec::new(),
        }
    }

    /// Sets the pack target size.
    /// 设置文件包目标大小。
    pub fn with_target_bytes(mut self, target_bytes: u64) -> Self {
        self.target_bytes = target_bytes;
        self
    }

    /// Sets the zstd level.
    /// 设置 zstd 级别。
    pub fn with_level(mut self, level: i32) -> Self {
        self.level = level;
        self
    }

    /// Adds one file's content and returns its digest; content already published or added is
    /// not stored twice.
    /// 加入一个文件的内容并返回其摘要；已发布或已加入的内容不会重复存储。
    pub fn add(&mut self, content: &[u8]) -> FormatResult<Sha256Digest> {
        let digest = Sha256Digest::of(content);
        if self.sealed.contains_key(&digest) || self.open_frames.contains_key(&digest) {
            return Ok(digest);
        }
        let frame = compress_frame(content, self.level)?;
        let length = frame.len() as u64;
        if length >= self.target_bytes {
            self.seal_open()?;
            self.open_frames.insert(digest, (0, length));
            self.open = frame;
            self.seal_open()?;
            return Ok(digest);
        }
        if self.open.len() as u64 + length > self.target_bytes {
            self.seal_open()?;
        }
        self.open_frames
            .insert(digest, (self.open.len() as u64, length));
        self.open.extend_from_slice(&frame);
        Ok(digest)
    }

    /// Seals the last pack and returns every location.
    /// 封存最后一个文件包并返回全部位置。
    pub fn finish(mut self) -> FormatResult<PackOutcome> {
        self.seal_open()?;
        Ok(PackOutcome {
            locations: self.sealed,
            new_packs: self.new_packs,
        })
    }

    /// Writes the open pack under its digest and records its frames.
    /// 以摘要为名写入打开中的文件包并记录其帧。
    fn seal_open(&mut self) -> FormatResult<()> {
        if self.open.is_empty() {
            return Ok(());
        }
        if self.open.len() as u64 > MAX_PACK_BYTES {
            return Err(FormatError::TooLarge {
                what: "pack",
                limit: MAX_PACK_BYTES,
            });
        }
        let pack = Sha256Digest::of(&self.open);
        write_object(&self.output, &pack_key(pack), &self.open)?;
        for (digest, (offset, length)) in self.open_frames.drain() {
            self.sealed.insert(
                digest,
                FileLocation {
                    pack,
                    offset,
                    length,
                },
            );
        }
        self.open.clear();
        self.new_packs.push(pack);
        Ok(())
    }
}

/// Writes an immutable object below `root`, through a temporary name so readers never see a part.
/// 在 `root` 之下写入不可变对象，经临时名写入，读取方因此不会看到不完整的文件。
pub fn write_object(root: &Path, key: &str, bytes: &[u8]) -> FormatResult<()> {
    let target = root.join(key);
    let fail = |source| FormatError::io("writing an object", source);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(fail)?;
    }
    let partial = target.with_extension("partial");
    std::fs::write(&partial, bytes).map_err(fail)?;
    std::fs::rename(&partial, &target).map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes zstd cannot shrink, so frame sizes follow content sizes in tests.
    /// zstd 无法压缩的字节，使测试中的帧大小跟随内容大小。
    fn noise(seed: u64, length: usize) -> Vec<u8> {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect()
    }

    fn read_back(root: &Path, location: FileLocation, content: &[u8]) -> FormatResult<Vec<u8>> {
        let pack = std::fs::read(root.join(pack_key(location.pack))).expect("pack");
        assert_eq!(Sha256Digest::of(&pack), location.pack);
        let start = location.offset as usize;
        let frame = &pack[start..start + location.length as usize];
        let mut output = Vec::new();
        extract_frame(
            frame,
            content.len() as u64,
            Sha256Digest::of(content),
            &mut output,
        )?;
        Ok(output)
    }

    #[test]
    fn packs_fill_to_the_target_and_large_frames_stand_alone() {
        let root = tempfile::tempdir().expect("temp dir");
        let small: Vec<Vec<u8>> = (0..5).map(|seed| noise(seed, 300)).collect();
        let large = noise(99, 2_000);
        let mut builder = PackBuilder::new(root.path(), HashMap::new())
            .with_target_bytes(1_000)
            .with_level(3);
        for content in &small[..3] {
            builder.add(content).expect("add");
        }
        builder.add(&large).expect("add large");
        for content in &small[3..] {
            builder.add(content).expect("add");
        }
        builder.add(&small[0]).expect("re-adding is free");
        let outcome = builder.finish().expect("finish");

        // Three frames of ~310 bytes fit in 1000; the large frame seals them and stands alone.
        // 三个约 310 字节的帧可放入 1000 字节；大帧使它们封包并独占一个包。
        assert_eq!(outcome.new_packs.len(), 3);
        let location = |content: &[u8]| outcome.locations[&Sha256Digest::of(content)];
        assert_eq!(location(&small[0]).pack, location(&small[2]).pack);
        assert_eq!(location(&small[0]).offset, 0);
        assert_ne!(location(&large).pack, location(&small[0]).pack);
        assert_ne!(location(&large).pack, location(&small[3]).pack);
        assert_eq!(location(&small[3]).pack, location(&small[4]).pack);
        for content in small.iter().chain([&large]) {
            assert_eq!(
                &read_back(root.path(), location(content), content).expect("extract"),
                content
            );
        }
    }

    #[test]
    fn published_contents_are_not_packed_again() {
        let root = tempfile::tempdir().expect("temp dir");
        let content = noise(1, 100);
        let published = FileLocation {
            pack: Sha256Digest::of(b"old pack"),
            offset: 7,
            length: 9,
        };
        let mut builder = PackBuilder::new(
            root.path(),
            HashMap::from([(Sha256Digest::of(&content), published)]),
        );
        builder.add(&content).expect("add");
        let outcome = builder.finish().expect("finish");
        assert!(outcome.new_packs.is_empty());
        assert_eq!(outcome.locations[&Sha256Digest::of(&content)], published);
    }

    #[test]
    fn extraction_rejects_wrong_digest_size_and_corruption() {
        let content = noise(2, 500);
        let frame = compress_frame(&content, 3).expect("frame");
        let digest = Sha256Digest::of(&content);
        let size = content.len() as u64;
        assert!(extract_frame(&frame[..], size, Sha256Digest::of(b"x"), Vec::new()).is_err());
        assert!(matches!(
            extract_frame(&frame[..], size - 1, digest, Vec::new()),
            Err(FormatError::SizeMismatch { .. })
        ));
        assert!(extract_frame(&frame[..], size + 1, digest, Vec::new()).is_err());
        assert!(extract_frame(&frame[..frame.len() - 4], size, digest, Vec::new()).is_err());
        let mut flipped = frame.clone();
        let middle = flipped.len() / 2;
        flipped[middle] ^= 0xFF;
        assert!(extract_frame(&flipped[..], size, digest, Vec::new()).is_err());
    }

    #[test]
    fn large_files_use_a_window_every_decoder_accepts() {
        let block = noise(3, 1 << 20);
        let content: Vec<u8> = block
            .iter()
            .chain(&vec![0_u8; LONG_MODE_THRESHOLD_BYTES as usize])
            .chain(&block)
            .copied()
            .collect();
        let frame = compress_frame(&content, 1).expect("frame");
        // Long-distance matching finds the repeated first megabyte nine megabytes later.
        // 长距离匹配能在 9 MiB 之后找到重复的第一个 1 MiB。
        assert!(frame.len() < (3 << 20) / 2);
        let mut output = Vec::new();
        extract_frame(
            &frame[..],
            content.len() as u64,
            Sha256Digest::of(&content),
            &mut output,
        )
        .expect("extract");
        assert_eq!(output, content);
    }

    #[test]
    fn bounded_decompression_refuses_oversized_output() {
        let frame = compress_frame(&[0_u8; 4_096], 3).expect("frame");
        assert!(decompress_bounded("test", &frame, 4_096).is_ok());
        assert!(matches!(
            decompress_bounded("test", &frame, 4_095),
            Err(FormatError::TooLarge { .. })
        ));
    }
}
