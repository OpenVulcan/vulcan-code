//! C ABI entry points and the handle registry that serve the client's wire protocol.
//! 服务客户端传输协议的 C ABI 入口与句柄注册表。
//!
//! The library owns its own Tokio executor: the host's executor is a separate copy of Tokio that this
//! library cannot see, so every asynchronous operation is driven to completion here and the call
//! blocks its calling thread. Runtimes and engines stay in a process-wide registry until the client
//! releases their handles; releasing a runtime drops it synchronously, which is what releases its
//! database root locks before the call returns.
//!
//! 本库拥有自己的 Tokio 执行器：宿主执行器是本库看不到的另一份 Tokio，因此每个异步操作都在此处执行完毕，
//! 调用会阻塞其调用线程。运行时与引擎保存在进程级注册表中，直到客户端释放其句柄；释放运行时会同步丢弃它，
//! 这正是在调用返回前释放其数据库根锁的方式。

use std::any::Any;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use vulcan_lancedb_contract::wire::{self, Request, Response, WireError};

use crate::engine::LanceDbEngine;
use crate::error::{EngineResult, LanceDbEngineError};
use crate::runtime::LanceDbRuntime;

/// Executor driving every library operation; a build failure is reported by each call.
/// 驱动每个库操作的执行器；构建失败时由每次调用报告。
static EXECUTOR: LazyLock<Result<tokio::runtime::Runtime, String>> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("vulcan-lancedb")
        .build()
        .map_err(|error| format!("failed to start the LanceDB executor: {error}"))
});

/// Live handles issued to the client.
/// 已签发给客户端的存活句柄。
static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);

/// Runtimes and engines addressed by client handles.
/// 由客户端句柄寻址的运行时与引擎。
#[derive(Default)]
struct Registry {
    /// Last issued handle; handles are never reused.
    /// 最近签发的句柄；句柄从不复用。
    last: u64,
    /// Runtimes by handle.
    /// 按句柄索引的运行时。
    runtimes: HashMap<u64, LanceDbRuntime>,
    /// Engines by handle.
    /// 按句柄索引的引擎。
    engines: HashMap<u64, LanceDbEngine>,
}

impl Registry {
    /// Issues one new handle.
    /// 签发一个新句柄。
    fn issue(&mut self) -> u64 {
        self.last += 1;
        self.last
    }
}

/// Locks the registry; a panic elsewhere never leaves it half-updated, so poisoning is ignored.
/// 锁定注册表；其他位置的 panic 不会使其处于半更新状态，因此忽略中毒。
fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Returns a clone of one registered runtime.
/// 返回一个已注册运行时的克隆。
fn runtime(handle: u64) -> EngineResult<LanceDbRuntime> {
    registry().runtimes.get(&handle).cloned().ok_or_else(|| {
        LanceDbEngineError::internal(format!("unknown LanceDB runtime handle {handle}"))
    })
}

/// Returns a clone of one registered engine.
/// 返回一个已注册引擎的克隆。
fn engine(handle: u64) -> EngineResult<LanceDbEngine> {
    registry().engines.get(&handle).cloned().ok_or_else(|| {
        LanceDbEngineError::internal(format!("unknown LanceDB engine handle {handle}"))
    })
}

/// Runs one request to completion on the library executor.
/// 在库执行器上把一个请求执行完毕。
fn execute(request: Request, payload: &[u8]) -> EngineResult<(Response, Vec<u8>)> {
    let executor = EXECUTOR
        .as_ref()
        .map_err(|detail| LanceDbEngineError::internal(detail.clone()))?;
    let response = match request {
        Request::CreateRuntime { config, options } => {
            let created = LanceDbRuntime::new(config, options);
            let mut registry = registry();
            let handle = registry.issue();
            registry.runtimes.insert(handle, created);
            Response::Runtime { runtime: handle }
        }
        Request::ReleaseRuntime { runtime } => {
            let released = registry().runtimes.remove(&runtime);
            // Dropping inside the executor context lets LanceDB release resources that expect one.
            // 在执行器上下文中丢弃，使需要上下文的 LanceDB 资源得以释放。
            let _context = executor.enter();
            drop(released);
            Response::Released
        }
        Request::OpenEngine {
            runtime: handle,
            database_name,
        } => {
            let source = runtime(handle)?;
            let opened = executor.block_on(source.open_named_engine(database_name.as_deref()))?;
            let database = Arc::as_ptr(&opened.connection()) as usize as u64;
            let mut registry = registry();
            let issued = registry.issue();
            registry.engines.insert(issued, opened);
            Response::Engine {
                engine: issued,
                database,
            }
        }
        Request::ReleaseEngine { engine } => {
            let released = registry().engines.remove(&engine);
            let _context = executor.enter();
            drop(released);
            Response::Released
        }
        Request::CreateTable {
            engine: handle,
            input,
        } => Response::TableCreated {
            result: executor.block_on(engine(handle)?.create_table(input))?,
        },
        Request::VectorUpsert {
            engine: handle,
            mut input,
        } => {
            input.data = payload.to_vec();
            Response::Upserted {
                result: executor.block_on(engine(handle)?.vector_upsert(input))?,
            }
        }
        Request::MaintainMergeKeyIndex {
            engine: handle,
            table_name,
            column,
        } => {
            executor.block_on(engine(handle)?.maintain_merge_key_index(&table_name, &column))?;
            Response::IndexMaintained
        }
        Request::VectorSearch {
            engine: handle,
            input,
            metric,
        } => {
            let mut result =
                executor.block_on(engine(handle)?.vector_search_with_metric(input, metric))?;
            let rows = std::mem::take(&mut result.data);
            return Ok((Response::Searched { result }, rows));
        }
        Request::Delete {
            engine: handle,
            input,
        } => Response::Deleted {
            result: executor.block_on(engine(handle)?.delete(input))?,
        },
        Request::DropTable {
            engine: handle,
            input,
        } => Response::TableDropped {
            result: executor.block_on(engine(handle)?.drop_table(input))?,
        },
        Request::TableStatistics {
            engine: handle,
            input,
        } => Response::Statistics {
            result: executor.block_on(engine(handle)?.table_statistics(input))?,
        },
        Request::CompactTable {
            engine: handle,
            input,
        } => Response::Compacted {
            result: executor.block_on(engine(handle)?.compact_table(input))?,
        },
    };
    Ok((response, Vec::new()))
}

/// Describes a caught panic payload.
/// 描述捕获的 panic 载荷。
fn panic_message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
}

/// Answers one request frame with one response frame; failures and panics become failure responses.
/// 以一个响应帧回答一个请求帧；失败与 panic 都转为失败响应。
fn respond(frame: &[u8]) -> Vec<u8> {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let (request, payload) =
            wire::decode_frame::<Request>(frame).map_err(LanceDbEngineError::internal)?;
        execute(request, payload)
    }));
    let (response, payload) = match outcome {
        Ok(Ok(answered)) => answered,
        Ok(Err(error)) => (
            Response::Failed {
                error: error.into(),
            },
            Vec::new(),
        ),
        Err(panic) => (
            Response::Failed {
                error: WireError::Internal {
                    detail: format!("LanceDB library panicked: {}", panic_message(&*panic)),
                },
            },
            Vec::new(),
        ),
    };
    wire::encode_frame(&response, &payload).unwrap_or_else(|detail| {
        // A failure response holds only strings and paths, so it always encodes.
        // 失败响应只含字符串与路径，因此总能编码。
        wire::encode_frame(
            &Response::Failed {
                error: WireError::Internal { detail },
            },
            &[],
        )
        .unwrap_or_default()
    })
}

/// Reports the wire protocol version this library speaks.
/// 报告本库使用的传输协议版本。
#[unsafe(no_mangle)]
pub extern "C" fn vulcan_lancedb_protocol_version() -> u32 {
    wire::PROTOCOL_VERSION
}

/// Executes one request frame and hands back one response frame owned by this library.
/// 执行一个请求帧，并交回一个由本库拥有的响应帧。
///
/// # Safety
/// `request` must point to `request_len` readable bytes (it may be null only when the length is
/// zero), and `response` and `response_len` must be valid for writes. The returned frame must be
/// released exactly once with [`vulcan_lancedb_free`].
/// `request` 必须指向 `request_len` 个可读字节（仅当长度为零时可为空），`response` 与 `response_len` 必须
/// 可写。返回的帧必须恰好通过 [`vulcan_lancedb_free`] 释放一次。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vulcan_lancedb_call(
    request: *const u8,
    request_len: usize,
    response: *mut *mut u8,
    response_len: *mut usize,
) {
    let frame = if request_len == 0 {
        &[][..]
    } else {
        // SAFETY: the caller guarantees `request` points to `request_len` readable bytes.
        unsafe { std::slice::from_raw_parts(request, request_len) }
    };
    let answer = respond(frame).into_boxed_slice();
    let length = answer.len();
    // SAFETY: the caller guarantees both output pointers are valid for writes.
    unsafe {
        *response_len = length;
        *response = Box::into_raw(answer).cast::<u8>();
    }
}

/// Releases one response frame returned by [`vulcan_lancedb_call`].
/// 释放一个由 [`vulcan_lancedb_call`] 返回的响应帧。
///
/// # Safety
/// `response` and `response_len` must be exactly a pair returned by [`vulcan_lancedb_call`] that has
/// not been released yet.
/// `response` 与 `response_len` 必须恰好是 [`vulcan_lancedb_call`] 返回且尚未释放的一对值。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vulcan_lancedb_free(response: *mut u8, response_len: usize) {
    if response.is_null() {
        return;
    }
    // SAFETY: the pair was produced by `Box::into_raw` on a boxed slice of exactly this length.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(response, response_len)) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DatabaseRuntimeConfig, LanceDbEngineOptions};
    use crate::db_lock::DatabaseRootLock;
    use crate::types::{LanceDbColumnDef, LanceDbColumnType, LanceDbCreateTableInput};

    /// Sends one request through the same frame path the C entry point uses.
    /// 通过 C 入口使用的同一帧路径发送一个请求。
    fn send(request: &Request) -> Response {
        let answer = respond(&wire::encode_frame(request, &[]).expect("encode request"));
        wire::decode_frame::<Response>(&answer)
            .expect("decode response")
            .0
    }

    /// Verifies the handle lifecycle: an engine works through its handle, shares one database
    /// identity per root, and releasing the runtime frees the root lock before the call returns.
    /// 验证句柄生命周期：引擎可通过句柄使用，同一根共享一个数据库身份，释放运行时会在调用返回前释放根锁。
    #[test]
    fn handles_drive_the_engine_and_runtime_release_frees_the_root_lock() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let root = directory.path().join("lance");
        let Response::Runtime { runtime } = send(&Request::CreateRuntime {
            config: DatabaseRuntimeConfig {
                default_db_path: root.clone(),
                db_root: Some(root.clone()),
                read_consistency_interval_ms: None,
            },
            options: LanceDbEngineOptions::default(),
        }) else {
            panic!("expected a runtime handle");
        };
        let open = || match send(&Request::OpenEngine {
            runtime,
            database_name: None,
        }) {
            Response::Engine { engine, database } => (engine, database),
            other => panic!("expected an engine handle, got {other:?}"),
        };
        let (first, first_database) = open();
        let (second, second_database) = open();
        assert_ne!(first, second, "every open issues its own handle");
        assert_eq!(first_database, second_database, "one root has one engine");

        let created = send(&Request::CreateTable {
            engine: first,
            input: LanceDbCreateTableInput {
                table_name: "items".to_string(),
                columns: vec![LanceDbColumnDef {
                    name: "id".to_string(),
                    column_type: LanceDbColumnType::String,
                    vector_dim: 0,
                    nullable: false,
                }],
                overwrite_if_exists: false,
            },
        });
        assert!(
            matches!(created, Response::TableCreated { .. }),
            "{created:?}"
        );

        assert!(
            DatabaseRootLock::acquire(&root).is_err(),
            "the runtime holds the root lock"
        );
        for engine in [first, second] {
            assert!(matches!(
                send(&Request::ReleaseEngine { engine }),
                Response::Released
            ));
        }
        assert!(matches!(
            send(&Request::ReleaseRuntime { runtime }),
            Response::Released
        ));
        DatabaseRootLock::acquire(&root).expect("releasing the runtime frees the root lock");

        let stale = send(&Request::OpenEngine {
            runtime,
            database_name: None,
        });
        assert!(
            matches!(stale, Response::Failed { .. }),
            "released handles are refused"
        );
    }
}
