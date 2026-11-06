//! Closes the descriptors a Unix parent left open in this process. The desktop client starts the
//! Core through Node, which passes on every descriptor Electron did not mark close-on-exec: its
//! resource files and the socket pairs between its own processes. Held here, they keep the client's
//! helper processes alive after the client ends.
//! 关闭 Unix 父进程遗留在本进程中的描述符。桌面客户端经 Node 启动 Core，Node 会传递 Electron 未标记
//! close-on-exec 的全部描述符：其资源文件及其各进程之间的套接字对。留在这里会使客户端结束后其辅助进程
//! 仍然存活。

#![allow(unsafe_code)]

/// Closes every descriptor above standard error. Runs first in `main`, before any thread starts or
/// any descriptor of this process is opened, so only inherited ones are closed.
/// 关闭标准错误之后的全部描述符。在 `main` 中最先执行，早于任何线程启动或本进程打开任何描述符，
/// 因此只会关闭继承来的描述符。
pub(crate) fn close() {
    let listing = if cfg!(target_os = "linux") {
        "/proc/self/fd"
    } else {
        "/dev/fd"
    };
    let Ok(entries) = std::fs::read_dir(listing) else {
        return;
    };
    let inherited: Vec<libc::c_int> = entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .filter(|descriptor| *descriptor > libc::STDERR_FILENO)
        .collect();
    // The listing's own descriptor is among them and already closed once `entries` is dropped;
    // closing it again only returns EBADF, as nothing was opened in between.
    // 列表自身的描述符也在其中，并在 `entries` 释放后已关闭；由于期间未打开任何内容，再次关闭只返回 EBADF。
    for descriptor in inherited {
        // SAFETY: no thread runs yet and nothing in this process owns these descriptors.
        // SAFETY：此时没有线程运行，本进程中也没有任何对象拥有这些描述符。
        unsafe {
            libc::close(descriptor);
        }
    }
}
