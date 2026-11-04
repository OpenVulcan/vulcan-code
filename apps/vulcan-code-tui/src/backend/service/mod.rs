// Service backend: the running Vulcan Code service over HTTP RPC and the `/events` WebSocket.
// 服务后端：通过 HTTP RPC 与 `/events` WebSocket 连接运行中的 Vulcan Code 服务。
//
// The endpoint is discovered from the service's own discovery file, so the TUI follows whichever
// service instance the desktop shell or `make.ps1` started. Everything runs on one worker thread
// (see `worker`); the UI keeps talking only through `BackendHandle`.
// 端点由服务自身的发现文件得到，因此 TUI 会跟随桌面壳或 `make.ps1` 启动的服务实例。所有工作都在
// 单一工作线程上完成（见 `worker`）；界面仍只通过 `BackendHandle` 通信。

mod discovery;
mod events;
mod files;
mod git;
mod launch;
mod map;
mod rpc;
#[cfg(test)]
mod test_support;
mod wire;
mod worker;

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use super::BackendHandle;
use worker::Input;

/// Selects an optional service discovery root without exposing credential contents.
/// 选择可选服务发现根目录，不公开凭据内容。
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Directory holding `webui-service.json`; the default search covers the usual roots.
    /// 存放 `webui-service.json` 的目录；默认搜索覆盖常用根目录。
    pub root: Option<PathBuf>,
}

/// Creates the backend channels and starts the serial worker and command forwarder.
/// 创建后端通道并启动串行工作线程及命令转发器。
/// Returns `BackendHandle`.
/// 返回 `BackendHandle`。
pub fn spawn(options: Options) -> BackendHandle {
    // Command tx; Command rx.
    // 界面命令发送端；后端命令接收端。
    let (command_tx, command_rx) = mpsc::channel();
    // Event tx; Event rx.
    // 后端事件发送端；界面事件接收端。
    let (event_tx, event_rx) = mpsc::channel();
    // Input tx; Input rx.
    // 工作线程收件发送端；工作线程收件接收端。
    let (input_tx, input_rx) = mpsc::channel::<Input>();
    // Commands share the worker's single inbox with service frames and finished calls.
    // 命令与服务帧、已完成调用共用工作线程的唯一收件箱。
    let forward = input_tx.clone();
    thread::Builder::new()
        .name("vulcan-commands".into())
        .spawn(move || {
            for command in command_rx {
                if forward.send(Input::Command(command)).is_err() {
                    break;
                }
            }
            let _ = forward.send(Input::Shutdown);
        })
        .expect("spawn command forwarder");
    thread::Builder::new()
        .name("vulcan-service".into())
        .spawn(move || worker::run(event_tx, input_tx, options.root, input_rx))
        .expect("spawn service worker");
    BackendHandle {
        commands: command_tx,
        events: event_rx,
        label: "service".into(),
        is_mock: false,
    }
}
