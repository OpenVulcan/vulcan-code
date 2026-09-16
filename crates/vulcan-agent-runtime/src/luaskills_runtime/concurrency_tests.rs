//! Deterministic concurrency contracts for the shared host Lua engine handle.
//! 共享宿主 Lua 引擎句柄的确定性并发契约。
//!
//! These tests prove the host-side read/write split with barriers and in-flight accounting rather
//! than wall-clock comparisons, so a regression back to one exclusive guard fails deterministically
//! instead of merely looking slow. Every wait is bounded, so a violation reports a panic message
//! instead of hanging the suite.
//! 这些测试用屏障与同时在途计数（而非墙钟比较）证明宿主侧读写分离，因此回退为单一独占守卫时
//! 会确定性失败，而不只是显得慢。每处等待都有上限，违反契约时给出 panic 信息而不是挂住测试套件。

use super::*;
use luaskills::{RuntimeHostToolAction, set_host_tool_callback};
use std::path::Path;
use std::sync::Barrier;
use std::sync::Condvar;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

/// Upper bound for every concurrency observation in this module.
/// 本模块内每次并发观测的时间上限。
const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(10);

/// Window during which a queued writer must stay excluded while a reader holds its guard.
/// 读者持有守卫期间，排队写者必须保持被排除的观察窗口。
const WRITER_EXCLUSION_WINDOW: Duration = Duration::from_millis(250);

/// Builds one real LuaSkills engine rooted at an isolated runtime layout.
/// 以隔离运行布局和指定主池上限构建一个真实 LuaSkills 引擎。
///
/// `runtime_root` owns the layout, `max_size` bounds main leases, and `system_root` selects System trust; returns the engine.
/// `runtime_root` 持有布局，`max_size` 限制主池租约，`system_root` 选择 System 信任根；返回引擎。
fn test_engine(runtime_root: &Path, max_size: usize, system_root: Option<&Path>) -> LuaEngine {
    // native_runtime_root keeps the test Lua package paths free of Windows verbatim prefixes.
    // native_runtime_root 使测试 Lua package 路径不包含 Windows 逐字前缀。
    let native_runtime_root = crate::runtime_assets::native_host_path(
        &runtime_root
            .canonicalize()
            .expect("canonicalize test LuaSkills runtime root"),
    );
    let mut host_options = LuaRuntimeHostOptions::with_runtime_root(native_runtime_root);
    host_options.runlua_pool_config = Some(LuaRuntimeRunLuaPoolConfig {
        min_size: 1,
        max_size: 2,
        idle_ttl_secs: 60,
    });
    host_options.system_lua_lib_dir = system_root.map(|root| {
        crate::runtime_assets::native_host_path(
            &root.canonicalize().expect("canonicalize System trust root"),
        )
    });
    LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size: 1,
            max_size,
            idle_ttl_secs: 60,
        },
        host_options,
    ))
    .expect("create LuaSkills test engine")
}

/// Builds one agent-facing handle that shares the supplied engine.
/// 构建共享所给引擎的 Agent 句柄。
fn test_handle(engine: Arc<RwLock<LuaEngine>>, runtime_root: PathBuf) -> LuaSkillsRuntimeHandle {
    LuaSkillsRuntimeHandle::from_shared_engine(
        engine,
        runtime_root,
        Vec::new(),
        0,
        Vec::new(),
        true,
        true,
        8,
        2,
    )
}

/// Verifies each host admission class mirrors only its owning upstream capacity.
/// 验证每个宿主入场类别只镜像其所属的上游容量。
#[test]
fn runtime_handle_separates_skill_runlua_and_session_admission() {
    // runtime_root owns every path derived by the test handle.
    // runtime_root 持有测试句柄派生的全部路径。
    let runtime_root = tempfile::tempdir().expect("create admission runtime root");
    // handle uses the production constructor with 8 Skill workers and 2 one-shot RunLua workers.
    // handle 使用生产构造器配置 8 路 Skill 与 2 路一次性 RunLua 工作线程。
    let handle = test_handle(
        Arc::new(RwLock::new(test_engine(runtime_root.path(), 8, None))),
        runtime_root.path().to_path_buf(),
    );

    assert_eq!(handle.skill_call_admission.available_permits(), 8);
    assert_eq!(handle.runlua_call_admission.available_permits(), 2);
    assert_eq!(
        handle.runlua_session_call_admission.available_permits(),
        LUASKILLS_RUNTIME_SESSION_LEASE_CAPACITY
    );
    assert!(!Arc::ptr_eq(
        &handle.runlua_call_admission,
        &handle.runlua_session_call_admission
    ));
}

/// Count threads in this exact Windows test process for isolated host-pressure diagnostics.
/// 统计当前 Windows 测试进程的线程数，用于隔离宿主压力诊断。
#[cfg(windows)]
fn process_thread_count() -> usize {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-Process -Id {}).Threads.Count", std::process::id()),
        ])
        .output()
        .expect("query exact test-process thread count");
    assert!(output.status.success(), "process thread query must succeed");
    String::from_utf8(output.stdout)
        .expect("decode thread count")
        .trim()
        .parse()
        .expect("parse thread count")
}

/// Verifies host admission queues before spawning workers and survives async waiter cancellation.
/// 验证宿主入场会在线程创建前排队，并在异步等待方取消后继续约束工作线程。
#[tokio::test]
async fn host_admission_bounds_workers_and_preserves_worker_owned_permits() {
    // admission matches a two-VM execution class and therefore allows exactly two worker bodies.
    // admission 对应两 VM 执行类别，因此只允许两个工作线程体进入。
    let admission = Arc::new(tokio::sync::Semaphore::new(2));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let active_workers = Arc::new(AtomicUsize::new(0));
    let peak_workers = Arc::new(AtomicUsize::new(0));
    let (entered_sender, mut entered_receiver) = tokio::sync::mpsc::unbounded_channel();

    // spawn_call schedules one async waiter while the production helper alone owns OS-thread creation.
    // spawn_call 调度一个异步等待方，系统线程只由生产辅助函数创建。
    let spawn_call = |call_id: usize| {
        let call_admission = Arc::clone(&admission);
        let call_release = Arc::clone(&release);
        let call_active_workers = Arc::clone(&active_workers);
        let call_peak_workers = Arc::clone(&peak_workers);
        let call_entered_sender = entered_sender.clone();
        tokio::spawn(async move {
            run_luaskills_blocking_task_async_with_diagnostics(
                "luaskills-admission-test",
                format!("admission-{call_id}"),
                call_admission,
                move |_stage, _invocation_id| {
                    let active = call_active_workers.fetch_add(1, Ordering::AcqRel) + 1;
                    call_peak_workers.fetch_max(active, Ordering::AcqRel);
                    call_entered_sender
                        .send(call_id)
                        .map_err(|error| error.to_string())?;
                    let (lock, wake) = &*call_release;
                    let held = lock.lock().map_err(|error| error.to_string())?;
                    let (released, wait) = wake
                        .wait_timeout_while(held, OBSERVATION_TIMEOUT, |released| !*released)
                        .map_err(|error| error.to_string())?;
                    call_active_workers.fetch_sub(1, Ordering::AcqRel);
                    if wait.timed_out() && !*released {
                        return Err(String::from("admission test release timed out"));
                    }
                    Ok(call_id)
                },
            )
            .await
        })
    };

    let first = spawn_call(1);
    let second = spawn_call(2);
    let first_entered = tokio::time::timeout(OBSERVATION_TIMEOUT, entered_receiver.recv())
        .await
        .expect("first admitted worker must enter")
        .expect("first admitted worker identity");
    let second_entered = tokio::time::timeout(OBSERVATION_TIMEOUT, entered_receiver.recv())
        .await
        .expect("second admitted worker must enter")
        .expect("second admitted worker identity");
    assert_ne!(first_entered, second_entered);
    assert_eq!(active_workers.load(Ordering::Acquire), 2);
    assert_eq!(admission.available_permits(), 0);

    let cancelled_while_queued = spawn_call(3);
    assert!(
        tokio::time::timeout(WRITER_EXCLUSION_WINDOW, entered_receiver.recv())
            .await
            .is_err(),
        "a queued request must not create or enter an OS worker"
    );
    cancelled_while_queued.abort();
    assert!(
        cancelled_while_queued
            .await
            .expect_err("queued request must be cancelled")
            .is_cancelled()
    );

    let (lock, wake) = &*release;
    *lock.lock().expect("release admitted workers") = true;
    wake.notify_all();
    assert_eq!(first.await.expect("join first async waiter").unwrap(), 1);
    assert_eq!(second.await.expect("join second async waiter").unwrap(), 2);
    assert_eq!(peak_workers.load(Ordering::Acquire), 2);
    assert_eq!(active_workers.load(Ordering::Acquire), 0);
    assert_eq!(admission.available_permits(), 2);

    // A cancelled receiver cannot release one permit still owned by its running worker.
    // 已取消的接收方不能释放仍由运行中工作线程持有的许可。
    *lock.lock().expect("reset worker gate") = false;
    let admitted_then_cancelled = spawn_call(4);
    assert_eq!(
        tokio::time::timeout(OBSERVATION_TIMEOUT, entered_receiver.recv())
            .await
            .expect("admitted worker must enter")
            .expect("admitted worker identity"),
        4
    );
    admitted_then_cancelled.abort();
    assert!(
        admitted_then_cancelled
            .await
            .expect_err("admitted waiter must be cancelled")
            .is_cancelled()
    );
    let follower = spawn_call(5);
    let concurrent_follower = spawn_call(6);
    let first_follower_id = tokio::time::timeout(OBSERVATION_TIMEOUT, entered_receiver.recv())
        .await
        .expect("one remaining permit admits a follower")
        .expect("follower identity");
    assert!([5, 6].contains(&first_follower_id));
    assert!(
        tokio::time::timeout(WRITER_EXCLUSION_WINDOW, entered_receiver.recv())
            .await
            .is_err(),
        "cancelled async waiter must not release the running worker permit"
    );
    *lock.lock().expect("release cancelled worker and follower") = true;
    wake.notify_all();
    let second_follower_id = tokio::time::timeout(OBSERVATION_TIMEOUT, entered_receiver.recv())
        .await
        .expect("queued follower must enter after worker completion")
        .expect("queued follower identity");
    assert_ne!(first_follower_id, second_follower_id);
    assert!([5, 6].contains(&second_follower_id));
    assert_eq!(follower.await.expect("join follower").unwrap(), 5);
    assert_eq!(
        concurrent_follower.await.expect("join follower").unwrap(),
        6
    );
    assert_eq!(active_workers.load(Ordering::Acquire), 0);
    assert_eq!(admission.available_permits(), 2);
}

/// Verify that independent host read guards overlap on one shared engine.
/// 验证相互独立的宿主读守卫可在同一共享引擎上重叠。
///
/// Four readers must all enter the guarded section at the same time. A single exclusive guard can
/// only ever admit one reader, so that regression fails on the second bounded receive.
/// 四个读者必须同时进入受保护区。单一独占守卫最多只能放进一个读者，因此该回退会在第二次有界接收时失败。
#[test]
fn independent_read_guards_overlap_on_one_shared_engine() {
    const READER_COUNT: usize = 4;

    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    fs::create_dir_all(&runtime_root).expect("create test runtime root");
    let handle = test_handle(
        Arc::new(RwLock::new(test_engine(&runtime_root, 1, None))),
        runtime_root,
    );

    // entered reports one reader that is inside the guarded section; release keeps every guard held
    // until the main thread has observed all of them.
    // entered 上报一个已进入受保护区的读者；release 让所有守卫保持持有，直到主线程观测到全部读者。
    let (entered_sender, entered_receiver) = mpsc::channel::<usize>();
    let release = Arc::new(Barrier::new(READER_COUNT + 1));
    let mut readers = Vec::with_capacity(READER_COUNT);
    for index in 0..READER_COUNT {
        let reader_handle = handle.clone();
        let sender = entered_sender.clone();
        let reader_release = Arc::clone(&release);
        readers.push(std::thread::spawn(move || {
            let _guard = lock_runtime_engine(&reader_handle).expect("acquire host read guard");
            sender.send(index).expect("report guarded entry");
            reader_release.wait();
        }));
    }
    drop(entered_sender);

    for seen in 0..READER_COUNT {
        entered_receiver
            .recv_timeout(OBSERVATION_TIMEOUT)
            .unwrap_or_else(|_| {
                panic!(
                    "only {seen} of {READER_COUNT} readers entered the guarded section at once; \
                     the shared host engine guard is still exclusive"
                )
            });
    }

    release.wait();
    for reader in readers {
        reader
            .join()
            .expect("reader thread must finish without deadlock");
    }
}

/// Verify a real LuaSkills reload waits for readers and completes under the host write guard.
/// 验证真实 LuaSkills 重载等待读者排空，并在宿主写守卫内完成。
#[test]
fn write_guard_excludes_readers_until_they_drain() {
    let temp = tempfile::tempdir().expect("create temp directory");
    let runtime_root = temp.path().join("runtime");
    fs::create_dir_all(&runtime_root).expect("create test runtime root");
    let skill_root = runtime_root.join("root_skills");
    fs::create_dir_all(&skill_root).expect("create isolated reload root");
    let handle = test_handle(
        Arc::new(RwLock::new(test_engine(&runtime_root, 1, None))),
        runtime_root,
    );

    // The main thread keeps one read guard so the writer cannot enter until it is dropped.
    // 主线程持有一个读守卫，因此写者在它被释放前无法进入。
    let reader_guard = lock_runtime_engine(&handle).expect("acquire host read guard");
    let (writer_sender, writer_receiver) = mpsc::channel::<()>();
    let writer_handle = handle.clone();
    let writer = std::thread::spawn(move || {
        let mut engine = lock_runtime_engine_mut(&writer_handle).expect("acquire host write guard");
        engine
            .reload_from_roots(&[RuntimeSkillRoot {
                name: String::from("ROOT"),
                skills_dir: skill_root,
            }])
            .expect("complete real LuaSkills reload under host write guard");
        writer_sender.send(()).expect("report write entry");
    });

    assert!(
        writer_receiver
            .recv_timeout(WRITER_EXCLUSION_WINDOW)
            .is_err(),
        "the write guard must not start while a read guard is still held"
    );

    drop(reader_guard);
    writer_receiver
        .recv_timeout(OBSERVATION_TIMEOUT)
        .expect("the write guard must start once every read guard is drained");
    writer
        .join()
        .expect("writer thread must finish without deadlock");
}

/// Verify a queued writer completes while new short readers continue to arrive.
/// 验证持续有短读者进入时，已排队的写者仍能在有界时间内完成。
#[test]
fn queued_writer_is_not_starved_by_new_short_readers() {
    let temp = tempfile::tempdir().expect("create isolated runtime directory");
    let runtime_root = temp.path().join("runtime");
    fs::create_dir_all(&runtime_root).expect("create runtime root");
    let handle = test_handle(
        Arc::new(RwLock::new(test_engine(&runtime_root, 8, None))),
        runtime_root,
    );
    let first_reader = lock_runtime_engine(&handle).expect("hold initial reader");
    let (queued_sender, queued_receiver) = mpsc::channel();
    let (writer_sender, writer_receiver) = mpsc::channel();
    let writer_handle = handle.clone();
    let writer = std::thread::spawn(move || {
        queued_sender.send(()).expect("announce queued writer");
        let started_at = Instant::now();
        let _guard = lock_runtime_engine_mut(&writer_handle).expect("acquire host write guard");
        writer_sender
            .send(started_at.elapsed())
            .expect("report writer wait");
    });
    queued_receiver
        .recv_timeout(OBSERVATION_TIMEOUT)
        .expect("writer must begin lock attempt");
    let continue_reading = Arc::new(AtomicBool::new(true));
    let mut readers = Vec::new();
    for _ in 0..4 {
        let reader_handle = handle.clone();
        let reading = Arc::clone(&continue_reading);
        readers.push(std::thread::spawn(move || {
            while reading.load(Ordering::Acquire) {
                let _guard = lock_runtime_engine(&reader_handle).expect("acquire short reader");
                std::thread::sleep(Duration::from_millis(2));
            }
        }));
    }
    drop(first_reader);
    let result = writer_receiver.recv_timeout(Duration::from_secs(5));
    continue_reading.store(false, Ordering::Release);
    for reader in readers {
        reader.join().expect("join short reader");
    }
    writer.join().expect("join queued writer");
    let wait = result.expect("queued writer must not starve behind arriving readers");
    assert!(wait <= Duration::from_secs(5));
    eprintln!(
        "[luaskills-runtime][writer-fairness-test] wait_ms={}",
        wait.as_millis()
    );
}

/// Verify skill calls overlap each isolated runtime while their cwd-sensitive paths remain ordered.
/// 验证技能调用能分别与隔离运行时重叠，而需要切换工作目录的两条路径保持有序。
///
/// This test owns LuaSkills' process-wide host callback and must run as one isolated test process.
/// 本测试占用 LuaSkills 的进程级宿主回调，必须在隔离的测试进程中单独运行。
#[test]
#[ignore = "mutates the process-wide LuaSkills host callback; run by exact name with one test thread"]
fn real_skill_runlua_and_system_calls_overlap_inside_host_callbacks() {
    let temp = tempfile::tempdir().expect("create isolated runtime root");
    let runtime_root = temp.path().join("runtime");
    let skill_root = runtime_root.join("root_skills");
    let skill_dir = skill_root.join("concurrency-test");
    fs::create_dir_all(skill_dir.join("runtime")).expect("create skill runtime directory");
    fs::write(
        skill_dir.join("skill.yaml"),
        "name: concurrency-test\nversion: 0.1.0\nenable: true\ndebug: false\nentries:\n  - name: ping\n    description: Concurrency probe.\n    lua_entry: runtime/ping.lua\n    lua_module: concurrency-test.ping\n",
    )
    .expect("write skill metadata");
    fs::write(
        skill_dir.join("runtime").join("ping.lua"),
        "return function(args)\n  local result = vulcan.host.call('concurrency.probe', { origin = 'skill' })\n  return result.ok and 'ready' or 'rejected'\nend\n",
    )
    .expect("write skill entry");

    let system_root = runtime_root.join("system_lua_lib");
    let system_package = system_root.join("concurrency-plugin");
    fs::create_dir_all(&system_package).expect("create strict System package root");
    fs::write(system_package.join("dependencies.yaml"), "{}\n")
        .expect("write System package dependencies");

    let mut engine = test_engine(&runtime_root, 8, Some(&system_root));
    let roots = vec![RuntimeSkillRoot {
        name: String::from("ROOT"),
        skills_dir: skill_root,
    }];
    engine.load_from_roots(&roots).expect("load probe skill");
    let system_package_path = crate::runtime_assets::native_host_path(
        &system_package
            .canonicalize()
            .expect("canonicalize System package root"),
    );
    let system_create_request = json!({
        "sid": "concurrent-system",
        "ttl_sec": 60,
        "system_package": {
            "id": "concurrency-plugin",
            "root": system_package_path,
            "dependencies_file": "dependencies.yaml",
        },
    });
    let system_created: Value = serde_json::from_str(
        &engine
            .create_system_runtime_lease_json(&system_create_request.to_string())
            .expect("create real System lease"),
    )
    .expect("decode System lease creation");
    assert_eq!(system_created["ok"], true);
    let system_eval_request = json!({
        "lease_id": system_created["lease_id"],
        "sid": system_created["sid"],
        "generation": system_created["generation"],
        "code": "local result = vulcan.host.call('concurrency.probe', { origin = 'system' })\nreturn result.ok and 'ready' or 'rejected'",
    })
    .to_string();
    let system_eval_request_after_reload = system_eval_request.clone();
    let roots_for_reload = roots.clone();
    let handle = LuaSkillsRuntimeHandle::from_shared_engine(
        Arc::new(RwLock::new(engine)),
        runtime_root,
        roots,
        0,
        Vec::new(),
        true,
        true,
        8,
        2,
    );
    let runlua_source = temp.path().join("concurrent-runlua.lua");
    fs::write(
        &runlua_source,
        "local result = vulcan.host.call('concurrency.probe', { origin = 'runlua' })\nreturn result.ok and 'ready' or 'rejected'\n",
    )
    .expect("write isolated runLua entry");
    let runlua_request = json!({
        "task": "concurrency probe",
        "file": crate::runtime_assets::native_host_path(&runlua_source),
        "args": {},
        "timeout_ms": 20_000,
        "caller_tool_name": "runlua",
    })
    .to_string();

    // release only gates the test callback; it does not replace the real LuaSkills call path.
    // release 仅控制测试回调，不替代真实 LuaSkills 调用路径。
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (entered_sender, entered_receiver) = mpsc::channel::<String>();
    let callback_release = Arc::clone(&release);
    set_host_tool_callback(Some(Arc::new(move |request| match request.action {
        RuntimeHostToolAction::List => Ok(json!([])),
        RuntimeHostToolAction::Has => Ok(json!(
            request.tool_name.as_deref() == Some("concurrency.probe")
        )),
        RuntimeHostToolAction::Call => {
            if request.tool_name.as_deref() != Some("concurrency.probe") {
                return Err(String::from("unexpected host tool"));
            }
            let origin = request.args["origin"]
                .as_str()
                .ok_or_else(|| String::from("missing concurrency probe origin"))?;
            entered_sender
                .send(origin.to_string())
                .map_err(|error| error.to_string())?;
            let (lock, wake) = &*callback_release;
            let held = lock.lock().map_err(|error| error.to_string())?;
            let (released, wait) = wake
                .wait_timeout_while(held, OBSERVATION_TIMEOUT, |released| !*released)
                .map_err(|error| error.to_string())?;
            if wait.timed_out() && !*released {
                return Err(String::from("probe callback release timed out"));
            }
            Ok(json!({"ok": true, "value": "ready"}))
        }
    })));

    // spawn_skill exercises the public Agent entry rather than calling LuaEngine directly.
    // spawn_skill 经过 Agent 公共入口，而不是直接调用 LuaEngine。
    let spawn_skill = |call_handle: LuaSkillsRuntimeHandle| {
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("create isolated caller runtime");
            runtime.block_on(call_luaskills_tool_entry(
                call_handle,
                String::from("concurrency-test-ping"),
                json!({}),
                None,
            ))
        })
    };

    // System and runLua both use upstream's process-cwd lock, so each is paired with Skill separately.
    // System 与 runLua 都使用上游的进程工作目录锁，因此分别与 Skill 配对验证。
    let system_eval_request_for_cwd = system_eval_request.clone();
    let system_handle = handle.clone();
    let system_call = std::thread::spawn(move || {
        let engine = lock_runtime_engine(&system_handle).expect("acquire System host read guard");
        engine.eval_system_runtime_lease_json(&system_eval_request)
    });
    let first_skill = spawn_skill(handle.clone());
    let first = entered_receiver.recv_timeout(OBSERVATION_TIMEOUT);
    let second = entered_receiver.recv_timeout(OBSERVATION_TIMEOUT);
    let (lock, wake) = &*release;
    *lock.lock().expect("release System pair") = true;
    wake.notify_all();
    let first_skill_result = first_skill.join().expect("join System-paired skill");
    let system_result = system_call.join().expect("join real System lease call");
    let system_pair: Vec<_> = [first, second].into_iter().filter_map(Result::ok).collect();
    assert_eq!(
        system_pair.len(),
        2,
        "System and Skill callbacks must overlap"
    );
    assert!(system_pair.iter().any(|origin| origin == "system"));
    assert!(system_pair.iter().any(|origin| origin == "skill"));
    assert_eq!(
        first_skill_result
            .expect("System-paired skill should succeed")
            .content,
        "ready"
    );
    let system_response: Value =
        serde_json::from_str(&system_result.expect("real System lease call should succeed"))
            .expect("decode System lease evaluation");
    assert_eq!(system_response["ok"], true);
    assert_eq!(system_response["result"], "ready");

    *lock.lock().expect("reset callback release") = false;
    let runlua_request_for_cwd = runlua_request.clone();
    let runlua_handle = handle.clone();
    let runlua_call = std::thread::spawn(move || {
        let engine = lock_runtime_engine(&runlua_handle).expect("acquire runLua host read guard");
        engine.execute_runlua_request_json_inline(&runlua_request)
    });
    let second_skill = spawn_skill(handle.clone());
    let third = entered_receiver.recv_timeout(OBSERVATION_TIMEOUT);
    let fourth = entered_receiver.recv_timeout(OBSERVATION_TIMEOUT);
    *lock.lock().expect("release callback lock") = true;
    wake.notify_all();
    let second_skill_result = second_skill.join().expect("join runLua-paired skill");
    let runlua_result = runlua_call.join().expect("join real runLua call");
    let runlua_pair: Vec<_> = [third, fourth].into_iter().filter_map(Result::ok).collect();
    assert_eq!(
        runlua_pair.len(),
        2,
        "runLua and Skill callbacks must overlap"
    );
    assert!(runlua_pair.iter().any(|origin| origin == "runlua"));
    assert!(runlua_pair.iter().any(|origin| origin == "skill"));
    assert_eq!(
        second_skill_result
            .expect("runLua-paired skill should succeed")
            .content,
        "ready"
    );
    assert!(
        runlua_result
            .expect("real runLua call should succeed")
            .contains("ready")
    );

    // File-backed runLua and System lease both switch process cwd under one upstream lock.
    // 文件型 runLua 与 System 租约都在同一把上游锁下切换进程工作目录。
    *lock.lock().expect("reset callback release for cwd pair") = false;
    let (cwd_ready_sender, cwd_ready_receiver) = mpsc::channel::<()>();
    let system_handle = handle.clone();
    let system_ready = cwd_ready_sender.clone();
    let cwd_system = std::thread::spawn(move || {
        let engine = lock_runtime_engine(&system_handle).expect("acquire System host read guard");
        system_ready
            .send(())
            .expect("report System host guard entry");
        engine.eval_system_runtime_lease_json(&system_eval_request_for_cwd)
    });
    let cwd_runlua_handle = handle.clone();
    let cwd_runlua = std::thread::spawn(move || {
        let engine =
            lock_runtime_engine(&cwd_runlua_handle).expect("acquire runLua host read guard");
        cwd_ready_sender
            .send(())
            .expect("report runLua host guard entry");
        engine.execute_runlua_request_json_inline(&runlua_request_for_cwd)
    });
    for _ in 0..2 {
        cwd_ready_receiver
            .recv_timeout(OBSERVATION_TIMEOUT)
            .expect("both cwd-sensitive calls must clear the host read guard");
    }
    let cwd_first = entered_receiver
        .recv_timeout(OBSERVATION_TIMEOUT)
        .expect("one cwd-sensitive callback must enter");
    assert!(
        entered_receiver
            .recv_timeout(WRITER_EXCLUSION_WINDOW)
            .is_err(),
        "two cwd-sensitive callbacks must not run together under the upstream cwd guard"
    );
    *lock.lock().expect("release cwd pair") = true;
    wake.notify_all();
    let cwd_second = entered_receiver
        .recv_timeout(OBSERVATION_TIMEOUT)
        .expect("the waiting cwd-sensitive callback must enter after release");
    assert_ne!(cwd_first, cwd_second);
    assert!([cwd_first.as_str(), cwd_second.as_str()].contains(&"system"));
    assert!([cwd_first.as_str(), cwd_second.as_str()].contains(&"runlua"));
    assert!(
        cwd_system
            .join()
            .expect("join cwd-sensitive System call")
            .is_ok()
    );
    assert!(
        cwd_runlua
            .join()
            .expect("join cwd-sensitive runLua call")
            .is_ok()
    );

    // Upstream reload replaces ordinary pools but retains the System lease manager.
    // 上游重载会替换普通技能池，却保留 System 租约管理器。
    {
        let mut engine = lock_runtime_engine_mut(&handle).expect("acquire reload write guard");
        engine
            .reload_from_roots(&roots_for_reload)
            .expect("reload ordinary skills while System lease exists");
    }
    let system_after_reload = {
        let engine = lock_runtime_engine(&handle).expect("acquire System read guard after reload");
        engine
            .eval_system_runtime_lease_json(&system_eval_request_after_reload)
            .expect("System lease remains callable after ordinary reload")
    };
    let system_after_reload: Value =
        serde_json::from_str(&system_after_reload).expect("decode System result after reload");
    assert_eq!(system_after_reload["ok"], true);
    assert_eq!(
        entered_receiver
            .recv_timeout(OBSERVATION_TIMEOUT)
            .expect("System callback remains live after reload"),
        "system"
    );

    // Inline requests avoid the upstream cwd guard and expose the independent runLua pool limit.
    // 内联请求不需要上游工作目录锁，可单独验证 runLua 池的容量边界。
    *lock.lock().expect("reset inline runLua gate") = false;
    let inline_request = json!({
        "task": "runLua pool probe",
        "code": "local result = vulcan.host.call('concurrency.probe', { origin = 'runlua-inline' })\nreturn result.ok and 'ready' or 'rejected'",
        "args": {},
        "timeout_ms": 20_000,
        "caller_tool_name": "runlua",
    })
    .to_string();
    let spawn_inline = |runlua_handle: LuaSkillsRuntimeHandle| {
        let request = inline_request.clone();
        std::thread::spawn(move || {
            let engine =
                lock_runtime_engine(&runlua_handle).expect("acquire inline runLua host read guard");
            engine.execute_runlua_request_json_inline(&request)
        })
    };
    let first_inline = spawn_inline(handle.clone());
    let second_inline = spawn_inline(handle.clone());
    for _ in 0..2 {
        assert_eq!(
            entered_receiver
                .recv_timeout(OBSERVATION_TIMEOUT)
                .expect("two runLua VMs must execute"),
            "runlua-inline"
        );
    }
    let third_inline = spawn_inline(handle.clone());
    assert!(
        entered_receiver
            .recv_timeout(WRITER_EXCLUSION_WINDOW)
            .is_err(),
        "the third inline runLua request must wait for a runLua VM"
    );
    *lock.lock().expect("release inline runLua calls") = true;
    wake.notify_all();
    assert_eq!(
        entered_receiver
            .recv_timeout(OBSERVATION_TIMEOUT)
            .expect("third runLua must enter after VM return"),
        "runlua-inline"
    );
    for caller in [first_inline, second_inline, third_inline] {
        assert!(
            caller
                .join()
                .expect("join inline runLua caller")
                .expect("inline runLua must succeed")
                .contains("ready")
        );
    }

    #[cfg(windows)]
    {
        // Harness callers remain OS threads, while the real Agent entry admits at most eight workers.
        // 测试调用方仍是系统线程，而真实 Agent 入口最多只准入八个工作线程。
        let baseline_threads = process_thread_count();
        for caller_count in [8usize, 16, 64] {
            *lock.lock().expect("reset skill pressure gate") = false;
            let callers: Vec<_> = (0..caller_count)
                .map(|_| spawn_skill(handle.clone()))
                .collect();
            for _ in 0..8 {
                assert_eq!(
                    entered_receiver
                        .recv_timeout(OBSERVATION_TIMEOUT)
                        .expect("eight main-pool VM callbacks must enter"),
                    "skill"
                );
            }
            if caller_count > 8 {
                assert!(
                    entered_receiver
                        .recv_timeout(WRITER_EXCLUSION_WINDOW)
                        .is_err(),
                    "the ninth skill must wait for a main-pool VM"
                );
            }
            std::thread::sleep(Duration::from_millis(250));
            let peak_threads = process_thread_count();
            *lock.lock().expect("release skill pressure gate") = true;
            wake.notify_all();
            for _ in 8..caller_count {
                assert_eq!(
                    entered_receiver
                        .recv_timeout(OBSERVATION_TIMEOUT)
                        .expect("waiting skill must enter after VM return"),
                    "skill"
                );
            }
            for caller in callers {
                assert_eq!(
                    caller
                        .join()
                        .expect("join pressure caller")
                        .expect("pressure skill must succeed")
                        .content,
                    "ready"
                );
            }
            let settled_at = Instant::now();
            let settled_threads = loop {
                let observed = process_thread_count();
                if observed <= baseline_threads + 8
                    || settled_at.elapsed() >= Duration::from_secs(5)
                {
                    break observed;
                }
                std::thread::sleep(Duration::from_millis(100));
            };
            eprintln!(
                "[luaskills-runtime][host-thread-pressure-test] harness_callers={caller_count} admitted_workers=8 baseline={baseline_threads} peak={peak_threads} settled={settled_threads} queued_async_waiters={} ",
                caller_count.saturating_sub(8)
            );
            assert!(peak_threads >= baseline_threads + caller_count);
            assert!(
                peak_threads <= baseline_threads + caller_count + 12,
                "production workers must remain bounded independently of harness callers"
            );
            assert!(settled_threads <= baseline_threads + 8);
        }
    }
    set_host_tool_callback(None);
}

/// Measure isolated Windows process memory and engine startup for each main-pool default profile.
/// 分别测量主池两档默认配置对应的隔离 Windows 进程内存和引擎启动耗时。
///
/// Run in separate test processes with `VULCAN_VM_PROBE_PROFILE=old` or `new`; the result is printed.
/// 分别以 `VULCAN_VM_PROBE_PROFILE=old` 或 `new` 的独立测试进程运行，并打印测量结果。
#[cfg(windows)]
#[test]
#[ignore = "isolated startup/RSS probe; run once per profile in separate test processes"]
fn vm_pool_startup_resource_probe() {
    let profile = std::env::var("VULCAN_VM_PROBE_PROFILE").expect("select old or new profile");
    let (min_size, max_size) = match profile.as_str() {
        "old" => (1, 2),
        "new" => (4, 8),
        _ => panic!("unknown VM probe profile: {profile}"),
    };
    let temp = tempfile::tempdir().expect("create isolated probe root");
    let runtime_root = temp.path().join("runtime");
    let skill_root = runtime_root.join("root_skills");
    // Both profiles load the same eight real skill descriptors and Lua entry files.
    // 两档配置都加载相同的八份真实技能描述与 Lua 入口文件。
    for index in 0..8 {
        let skill_id = format!("resource-probe-{index}");
        let skill_dir = skill_root.join(&skill_id);
        fs::create_dir_all(skill_dir.join("runtime")).expect("create probe skill directory");
        fs::write(
            skill_dir.join("skill.yaml"),
            format!(
                "name: {skill_id}\nversion: 0.1.0\nenable: true\ndebug: false\nentries:\n  - name: ping\n    description: Resource probe.\n    lua_entry: runtime/ping.lua\n    lua_module: {skill_id}.ping\n"
            ),
        )
        .expect("write probe skill descriptor");
        fs::write(
            skill_dir.join("runtime").join("ping.lua"),
            "return function(args) return 'ready' end\n",
        )
        .expect("write probe skill entry");
    }
    let started_at = Instant::now();
    let mut engine = LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size,
            max_size,
            idle_ttl_secs: 30,
        },
        LuaRuntimeHostOptions::with_runtime_root(crate::runtime_assets::native_host_path(
            &runtime_root
                .canonicalize()
                .expect("canonicalize probe runtime root"),
        )),
    ))
    .expect("construct isolated probe engine");
    engine
        .load_from_roots(&[RuntimeSkillRoot {
            name: String::from("ROOT"),
            skills_dir: skill_root,
        }])
        .expect("load isolated ROOT layer");
    let startup_ms = started_at.elapsed().as_millis();
    // Allow the test process working set to settle before sampling the comparable RSS proxy.
    // 在采样可比较的 RSS 代理值前，让测试进程工作集短暂稳定。
    std::thread::sleep(Duration::from_millis(250));
    let process_id = std::process::id();
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-Process -Id {process_id}).WorkingSet64"),
        ])
        .output()
        .expect("query exact test-process working set");
    assert!(output.status.success(), "working-set query must succeed");
    let rss_bytes: u64 = String::from_utf8(output.stdout)
        .expect("decode working-set output")
        .trim()
        .parse()
        .expect("parse working-set bytes");
    eprintln!(
        "[luaskills-vm-resource-probe] profile={profile} min={min_size} max={max_size} startup_ms={startup_ms} rss_bytes={rss_bytes}"
    );
    drop(engine);
}
