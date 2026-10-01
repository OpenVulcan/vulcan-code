// Vulcan Code terminal client.
// Vulcan Code 终端客户端。
//
// `backend` is the only data boundary (mock today, core service later), `app` holds state and
// input handling, `ui` draws. `i18n` serves catalogs synced from the WebUI by `i18n_sync`.
// `backend` 是唯一的数据边界（当前为模拟后端，之后接入核心服务），`app` 保存状态与输入处理，
// `ui` 负责绘制。`i18n` 提供由 `i18n_sync` 从 WebUI 同步的文案目录。

pub mod app;
pub mod backend;
pub mod i18n;
pub mod i18n_sync;
pub mod model;
pub mod ui;
