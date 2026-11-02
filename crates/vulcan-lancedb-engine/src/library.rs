//! Loads the native LanceDB library once per process and exchanges wire frames with it.
//! 每个进程加载一次原生 LanceDB 库，并与其交换传输帧。
//!
//! The library sits beside the running executable. Cargo places test binaries one level deeper, in
//! `<profile>/deps`, so for an executable inside a `deps` directory the profile directory is used;
//! this is the only alternative location and it never applies to a distribution's `bin` directory.
//! A missing library, a missing export or a different protocol version fails every operation with
//! [`LanceDbEngineError::LibraryUnavailable`]; nothing falls back to an in-process engine.
//!
//! 库位于运行中可执行文件旁边。Cargo 把测试二进制放在更深一层的 `<profile>/deps`，因此位于 `deps` 目录的
//! 可执行文件改用其 profile 目录；这是唯一的替代位置，且从不适用于发行物的 `bin` 目录。库缺失、导出缺失或协议
//! 版本不同时，每个操作都以 [`LanceDbEngineError::LibraryUnavailable`] 失败；不会回退到进程内引擎。

use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::OnceLock;

use vulcan_lancedb_contract::error::{EngineResult, LanceDbEngineError};
use vulcan_lancedb_contract::wire::{self, Request, Response};

/// Signature of the exported protocol-version function.
/// 导出的协议版本函数签名。
type ProtocolVersionFn = unsafe extern "C" fn() -> u32;

/// Signature of the exported call function.
/// 导出的调用函数签名。
type CallFn = unsafe extern "C" fn(*const u8, usize, *mut *mut u8, *mut usize);

/// Signature of the exported release function.
/// 导出的释放函数签名。
type FreeFn = unsafe extern "C" fn(*mut u8, usize);

/// One loaded native library; it is never unloaded, so its function pointers stay valid.
/// 一个已加载的原生库；它从不卸载，因此其函数指针始终有效。
pub(crate) struct NativeLibrary {
    /// Exported call function.
    /// 导出的调用函数。
    call: CallFn,
    /// Exported release function.
    /// 导出的释放函数。
    free: FreeFn,
    /// Keeps the library mapped for the life of the process.
    /// 使库在进程生命周期内保持映射。
    _library: libloading::Library,
}

/// Load outcome shared by every caller; a failure keeps its path and description.
/// 所有调用方共享的加载结果；失败时保留其路径与描述。
static LIBRARY: OnceLock<Result<NativeLibrary, (PathBuf, String)>> = OnceLock::new();

/// Returns the process-wide native library, loading it on first use.
/// 返回进程级原生库，首次使用时加载。
pub(crate) fn native() -> EngineResult<&'static NativeLibrary> {
    match LIBRARY.get_or_init(load) {
        Ok(library) => Ok(library),
        Err((path, detail)) => Err(LanceDbEngineError::LibraryUnavailable {
            path: path.clone(),
            detail: detail.clone(),
        }),
    }
}

/// Resolves the library path beside the running executable.
/// 解析运行中可执行文件旁边的库路径。
fn library_path() -> Result<PathBuf, (PathBuf, String)> {
    let name = wire::library_file_name();
    let executable = std::env::current_exe().map_err(|error| {
        (
            PathBuf::from(&name),
            format!("cannot resolve the running executable: {error}"),
        )
    })?;
    let Some(directory) = executable.parent() else {
        return Err((
            PathBuf::from(&name),
            format!("{} has no parent directory", executable.display()),
        ));
    };
    let directory = match directory.parent() {
        Some(profile) if directory.file_name() == Some(OsStr::new("deps")) => profile,
        _ => directory,
    };
    Ok(directory.join(name))
}

/// Loads the library, checks its protocol version and resolves its exports.
/// 加载库，校验其协议版本并解析其导出。
fn load() -> Result<NativeLibrary, (PathBuf, String)> {
    let path = library_path()?;
    let failure = |detail: String| (path.clone(), detail);
    // SAFETY: loading runs the library's initializers. The file is the `vulcan-lancedb-native`
    // build shipped beside this executable, whose initializers have no preconditions.
    let library = unsafe { libloading::Library::new(&path) }
        .map_err(|error| failure(format!("failed to load: {error}")))?;
    // SAFETY: the symbol types below are exactly the signatures exported by
    // `vulcan-lancedb-native/src/exports.rs`, and the copied pointers never outlive `library`,
    // which is stored with them and never unloaded.
    let version =
        *unsafe { library.get::<ProtocolVersionFn>(wire::PROTOCOL_VERSION_SYMBOL.as_bytes()) }
            .map_err(|error| failure(format!("missing protocol export: {error}")))?;
    // SAFETY: the protocol-version export takes no arguments and has no preconditions.
    let reported = unsafe { version() };
    if reported != wire::PROTOCOL_VERSION {
        return Err(failure(format!(
            "library speaks protocol {reported}, but this host requires protocol {}",
            wire::PROTOCOL_VERSION
        )));
    }
    // SAFETY: as above, the types match the library's exports.
    let call = *unsafe { library.get::<CallFn>(wire::CALL_SYMBOL.as_bytes()) }
        .map_err(|error| failure(format!("missing call export: {error}")))?;
    // SAFETY: as above, the types match the library's exports.
    let free = *unsafe { library.get::<FreeFn>(wire::FREE_SYMBOL.as_bytes()) }
        .map_err(|error| failure(format!("missing release export: {error}")))?;
    Ok(NativeLibrary {
        call,
        free,
        _library: library,
    })
}

impl NativeLibrary {
    /// Sends one request synchronously; a failure response becomes the restored error.
    /// 同步发送一个请求；失败响应被还原为对应错误。
    ///
    /// The call blocks until the library finishes, so asynchronous callers go through [`call`].
    /// 调用会阻塞到库完成为止，因此异步调用方经由 [`call`] 发送。
    pub(crate) fn exchange(
        &self,
        request: &Request,
        payload: &[u8],
    ) -> EngineResult<(Response, Vec<u8>)> {
        let frame = wire::encode_frame(request, payload).map_err(LanceDbEngineError::internal)?;
        let mut answer: *mut u8 = std::ptr::null_mut();
        let mut answer_len = 0_usize;
        // SAFETY: `frame` is a live slice of `frame.len()` bytes and both output locations are
        // valid for writes for the duration of the call.
        unsafe { (self.call)(frame.as_ptr(), frame.len(), &mut answer, &mut answer_len) };
        if answer.is_null() {
            return Err(LanceDbEngineError::internal(
                "LanceDB library returned no response frame",
            ));
        }
        // SAFETY: the library returned `answer_len` readable bytes that it owns until released.
        let decoded = wire::decode_frame::<Response>(unsafe {
            std::slice::from_raw_parts(answer, answer_len)
        })
        .map(|(response, rows)| (response, rows.to_vec()));
        // SAFETY: the pair came from this library's call function and is released exactly once,
        // after the borrowed bytes above were copied.
        unsafe { (self.free)(answer, answer_len) };
        match decoded.map_err(LanceDbEngineError::internal)? {
            (Response::Failed { error }, _) => Err(error.into()),
            answered => Ok(answered),
        }
    }
}

/// Sends one request from asynchronous code without blocking the caller's executor.
/// 从异步代码发送一个请求，而不阻塞调用方执行器。
///
/// Dropping the returned future does not cancel the operation; the library finishes it, exactly as
/// a write that already reached LanceDB would.
/// 丢弃返回的 future 不会取消操作；库会把它完成，与已到达 LanceDB 的写入一样。
pub(crate) async fn call(request: Request, payload: Vec<u8>) -> EngineResult<(Response, Vec<u8>)> {
    let library = native()?;
    tokio::task::spawn_blocking(move || library.exchange(&request, &payload))
        .await
        .map_err(|error| {
            LanceDbEngineError::internal(format!("LanceDB call did not complete: {error}"))
        })?
}

/// Builds the failure reported when the library answers with another operation's outcome.
/// 构造库以其他操作结果作答时报告的失败。
pub(crate) fn unexpected(response: Response) -> LanceDbEngineError {
    LanceDbEngineError::internal(format!("unexpected LanceDB response: {response:?}"))
}
