//! Client handle for one embedded runtime inside the native LanceDB library.
//! 原生 LanceDB 库中一个嵌入式运行时的客户端句柄。
//!
//! The library runtime resolves a database name to a physical key, reuses one engine per key and
//! holds that root's lock for as long as the runtime lives. The client creates the library runtime
//! on the first open, so loading the library and authorizing the paths both report through the same
//! [`EngineResult`] as every other open failure, and releases it when the last clone is dropped,
//! which releases the root locks before the drop returns.
//!
//! 库运行时把数据库名解析为物理键，按键复用一个引擎，并在运行时存活期间持有该根的锁。客户端在首次打开时创建库
//! 运行时，使库加载与路径授权都与其他打开失败一样经由 [`EngineResult`] 报告；在最后一个克隆被丢弃时释放它，
//! 从而在丢弃返回之前释放根锁。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::OnceCell;
use vulcan_lancedb_contract::config::{DatabaseRuntimeConfig, LanceDbEngineOptions};
use vulcan_lancedb_contract::error::{EngineResult, LanceDbEngineError};
use vulcan_lancedb_contract::wire::{Request, Response};

use crate::engine::LanceDbEngine;
use crate::library::{NativeLibrary, call, native, unexpected};

/// Embedded LanceDB runtime managing database instances and engine construction.
/// 嵌入式 LanceDB 运行时，负责管理数据库实例与引擎构造。
#[derive(Clone)]
pub struct LanceDbRuntime {
    /// State shared by every clone.
    /// 所有克隆共享的状态。
    state: Arc<RuntimeState>,
}

/// Configuration and the lazily created library runtime.
/// 配置以及惰性创建的库运行时。
struct RuntimeState {
    /// Database configuration.
    /// 数据库配置。
    config: DatabaseRuntimeConfig,
    /// Options applied to every engine.
    /// 施加于每个引擎的选项。
    engine_options: LanceDbEngineOptions,
    /// Library runtime, created by the first open.
    /// 库运行时，由首次打开创建。
    handle: OnceCell<RuntimeHandle>,
}

/// One runtime handle issued by the library.
/// 库签发的一个运行时句柄。
struct RuntimeHandle {
    /// Library that issued the handle.
    /// 签发该句柄的库。
    library: &'static NativeLibrary,
    /// Runtime handle.
    /// 运行时句柄。
    runtime: u64,
}

impl Drop for RuntimeHandle {
    /// Releases the library runtime together with the root locks it holds.
    /// 释放库运行时及其持有的根锁。
    fn drop(&mut self) {
        // Releasing a live handle cannot fail, and a destructor has nowhere to report to.
        // 释放存活句柄不会失败，析构函数也无处报告。
        let _ = self.library.exchange(
            &Request::ReleaseRuntime {
                runtime: self.runtime,
            },
            &[],
        );
    }
}

impl LanceDbRuntime {
    /// Creates the embedded runtime from database configuration and engine options.
    /// 基于数据库配置与引擎选项创建嵌入式运行时。
    pub fn new(config: DatabaseRuntimeConfig, engine_options: LanceDbEngineOptions) -> Self {
        Self {
            state: Arc::new(RuntimeState {
                config,
                engine_options,
                handle: OnceCell::new(),
            }),
        }
    }

    /// Returns the engine options used by this runtime.
    /// 返回本运行时使用的引擎选项。
    pub fn engine_options(&self) -> &LanceDbEngineOptions {
        &self.state.engine_options
    }

    /// Resolves the physical path for a database name without opening a connection.
    /// 解析某个数据库名的物理路径，不建立连接。
    pub fn database_path_for_name(&self, database_name: Option<&str>) -> EngineResult<PathBuf> {
        self.state.config.database_path_for_name(database_name)
    }

    /// Opens the default database and its engine.
    /// 打开默认数据库及其引擎。
    pub async fn open_default_engine(&self) -> EngineResult<LanceDbEngine> {
        self.open_named_engine(None).await
    }

    /// Opens a named database and its engine; `None` means the default database.
    /// 按名称打开数据库及其引擎；`None` 表示默认数据库。
    pub async fn open_named_engine(
        &self,
        database_name: Option<&str>,
    ) -> EngineResult<LanceDbEngine> {
        let handle = self
            .state
            .handle
            .get_or_try_init(|| self.create_handle())
            .await?;
        let request = Request::OpenEngine {
            runtime: handle.runtime,
            database_name: database_name.map(str::to_string),
        };
        match call(request, Vec::new()).await? {
            (Response::Engine { engine, database }, _) => Ok(LanceDbEngine::from_handle(
                handle.library,
                engine,
                database,
                self.state.engine_options.clone(),
            )),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Creates the library runtime for this configuration.
    /// 为本配置创建库运行时。
    async fn create_handle(&self) -> EngineResult<RuntimeHandle> {
        let config = &self.state.config;
        require_unicode_path(&config.default_db_path)?;
        if let Some(root) = &config.db_root {
            require_unicode_path(root)?;
        }
        let library = native()?;
        let request = Request::CreateRuntime {
            config: config.clone(),
            options: self.state.engine_options.clone(),
        };
        match call(request, Vec::new()).await? {
            (Response::Runtime { runtime }, _) => Ok(RuntimeHandle { library, runtime }),
            (other, _) => Err(unexpected(other)),
        }
    }
}

/// Refuses a path the wire protocol cannot carry without changing its identity.
/// 拒绝传输协议无法在不改变其身份的前提下携带的路径。
fn require_unicode_path(path: &Path) -> EngineResult<()> {
    match path.to_str() {
        Some(_) => Ok(()),
        None => Err(LanceDbEngineError::path_not_authorized(format!(
            "LanceDB requires a Unicode local path and cannot open '{}' without changing its identity",
            path.display()
        ))),
    }
}
