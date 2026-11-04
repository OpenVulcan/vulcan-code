//! Size bounds and compression settings of the release formats; every other module refers to these.
//! 发布格式的大小上限与压缩设置；其他模块一律引用这里。
//!
//! The openvulcan-web static sync enforces the same bounds in `server/internal/staticsync/format.go`;
//! change both together.
//! openvulcan-web 的静态同步在 `server/internal/staticsync/format.go` 中执行相同的上限；两处须同时修改。

/// Largest channel pointer and largest detached signature file accepted.
/// 接受的通道指针与分离签名文件的最大字节数。
pub const MAX_POINTER_BYTES: u64 = 64 << 10;

/// Largest release manifest accepted.
/// 接受的版本清单最大字节数。
pub const MAX_MANIFEST_BYTES: u64 = 1 << 20;

/// Largest release notes document accepted.
/// 接受的更新说明最大字节数。
pub const MAX_NOTES_BYTES: u64 = 4 << 20;

/// Largest compressed component index accepted.
/// 接受的压缩后组件索引最大字节数。
pub const MAX_INDEX_BYTES: u64 = 16 << 20;

/// Largest decompressed component index accepted.
/// 接受的解压后组件索引最大字节数。
pub const MAX_EXPANDED_INDEX_BYTES: u64 = 128 << 20;

/// Largest pack accepted; a file whose frame exceeds the target occupies a pack of its own.
/// 接受的文件包最大字节数；帧超过目标大小的文件独占一个包。
pub const MAX_PACK_BYTES: u64 = 2 << 30;

/// Largest installer download accepted.
/// 接受的安装包下载最大字节数。
pub const MAX_INSTALLER_BYTES: u64 = 2 << 30;

/// Largest terminal install script accepted.
/// 接受的终端安装脚本最大字节数。
pub const MAX_SCRIPT_BYTES: u64 = 1 << 20;

/// Largest setup program a setup pointer may name.
/// 安装器指针可指向的最大安装器。
pub const MAX_SETUP_PROGRAM_BYTES: u64 = 64 << 20;

/// Length of a raw Ed25519 detached signature.
/// 原始 Ed25519 分离签名的长度。
pub const SIGNATURE_BYTES: usize = 64;

/// Default pack size the producer fills up to (plan decision D4).
/// 生成器填充文件包的默认目标大小（方案决策 D4）。
pub const DEFAULT_PACK_TARGET_BYTES: u64 = 16 << 20;

/// zstd level of every pack frame and component index.
/// 每个文件包帧与组件索引的 zstd 级别。
pub const COMPRESSION_LEVEL: i32 = 19;

/// Largest zstd window, as a power of two (128 MiB); decoders refuse anything larger.
/// zstd 窗口上限，以 2 的幂表示（128 MiB）；解码器拒绝更大的窗口。
pub const MAX_WINDOW_LOG: u32 = 27;

/// Files larger than this use long-distance matching with a window sized to the file.
/// 大于此值的文件启用长距离匹配，窗口按文件大小设置。
pub const LONG_MODE_THRESHOLD_BYTES: u64 = 8 << 20;

/// Frames of one pack separated by at most this many unneeded bytes are fetched in one request.
/// 同一文件包中间隔不超过此字节数的帧合并为一个请求获取。
pub const RANGE_MERGE_GAP_BYTES: u64 = 64 << 10;

/// Free space required beyond the bytes to write, in percent (plan section 8).
/// 除待写入字节外要求的剩余空间余量百分比（方案第八节）。
pub const DISK_HEADROOM_PERCENT: u64 = 10;

/// Longest relative path accepted in a component index, in bytes.
/// 组件索引接受的相对路径最大字节数。
pub const MAX_PATH_BYTES: usize = 1024;

/// Longest component name accepted.
/// 接受的组件名最大长度。
pub const MAX_COMPONENT_NAME_BYTES: usize = 64;
