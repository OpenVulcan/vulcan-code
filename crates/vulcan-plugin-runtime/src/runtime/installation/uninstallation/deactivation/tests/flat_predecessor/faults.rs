//! Real dormant-source faults retain the original physical owners and never adopt replacement bytes.
//! 真实休眠来源故障保留原物理所有者，绝不接纳替代字节。

use super::*;
use std::cell::Cell;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

/// Derive observation metadata from the actual retained pair and the sole production tombstone rule.
/// 从实际保留配对及唯一生产隐藏规则派生观测元数据。
/// manager/state supply the real canonical root and request; returns a path without allocating cleanup authority.
/// manager／state 提供真实规范根及请求；返回路径，不分配清理权威。
fn hidden_path(
    manager: &VulcanPluginRuntimeManager,
    state: &VulcanPluginDeactivationStateReservation,
) -> PathBuf {
    // Canonicalize the actual manager root, matching the original capture rather than inventing another boundary.
    // 规范化实际管理器根，匹配原捕获，而非虚构另一边界。
    let user_root = manager
        .user_plugin_root()
        .canonicalize()
        .expect("actual original canonical user root");
    uninstallation_tombstone(&user_root, state.plugin_id(), state.request_id())
}

/// Inventory real fixture files before deletion so a Windows refusal proves partial physical removal.
/// 删除前清点真实夹具文件，使 Windows 拒绝能够证明部分物理移除。
/// root is the actual hidden original; returns relative file paths without assuming directory enumeration order.
/// root 为实际隐藏原来源；返回相对文件路径，不假定目录枚举顺序。
#[cfg(windows)]
fn actual_file_paths(root: &Path) -> Vec<PathBuf> {
    // Retain concrete paths instead of interpreting an attempted deletion or a checkpoint count as physical progress.
    // 保留具体路径，不把删除尝试或检查点计数解释为物理进度。
    let mut files = Vec::new();
    // This worklist visits only the original fixture tree already validated by production capture.
    // 此工作列表仅访问已由生产捕获验证的原夹具目录树。
    let mut directories = vec![root.to_path_buf()];
    // Each directory is an actual descendant of this single hidden original.
    // 每个目录均为此单个隐藏原来源的实际下级。
    while let Some(directory) = directories.pop() {
        // Each entry comes from the actual filesystem, never from an inferred manifest layout.
        // 每个条目均来自实际文件系统，绝非推定清单布局。
        for entry in fs::read_dir(directory).expect("read actual hidden fixture directory") {
            // The real entry supplies the unique file or directory path.
            // 真实条目提供唯一文件或目录路径。
            let path = entry.expect("actual hidden fixture entry").path();
            if path.is_dir() {
                directories.push(path);
            } else {
                assert!(
                    path.is_file(),
                    "actual fixture descendant must be a plain file"
                );
                files.push(
                    path.strip_prefix(root)
                        .expect("actual file belongs to the same hidden original")
                        .to_path_buf(),
                );
            }
        }
    }
    files
}

/// A real Windows rename refusal retains exact source bytes, rejects later changes and recovers only the same restored original.
/// 真实 Windows 重命名拒绝保留精确来源字节，拒绝后续改变，并仅恢复相同已还原原来源。
/// No parameters or return value; the actual directory handle excludes DELETE sharing throughout the first rename attempt.
/// 无参数及返回值；实际目录句柄在首次重命名尝试期间排除 DELETE 共享。
#[cfg(windows)]
#[test]
fn dormant_flat_rename_refusal_preserves_changed_bytes_and_recovers_original() {
    // Temporary storage outlives the same original pair and the real Windows directory blocker.
    // 临时存储比相同原配对及真实 Windows 目录阻挡句柄存活更久。
    let temp = tempfile::tempdir().expect("actual dormant rename refusal fixture");
    // Production installation and activation establish independent managed and dormant packages.
    // 生产安装及激活建立独立受管及休眠包。
    let (manager, store, selected) = fixture(temp.path(), false);
    // The same non-cloneable pair survives the failed rename, byte conflict and final recovery.
    // 相同不可克隆配对跨失败重命名、字节冲突及最终恢复存续。
    let (mut original, mut state) = reserve(&manager, &store, &selected);
    // Use the inspected original's actual path, independent of current immutable managed routing.
    // 使用已检查原来源实际路径，独立于当前不可变受管路由。
    let flat_root = state
        .original_flat_predecessor()
        .expect("actual captured dormant source")
        .package_root
        .clone();
    // The hidden observation path derives solely from this actual retained pair.
    // 隐藏观测路径仅由此实际保留配对派生。
    let hidden = hidden_path(&manager, &state);
    // Retain the actual original manifest so recovery restores exact bytes rather than recapturing a descriptor.
    // 保留实际原清单，使恢复还原精确字节，而非重新捕获描述符。
    let manifest = flat_root.join("plugin.yaml");
    // These exact bytes are restored only by the test, never by a production fallback.
    // 此精确字节仅由测试还原，绝非生产兜底。
    let original_bytes = fs::read(&manifest).expect("actual original dormant manifest bytes");
    collect_local(&manager, &store, &mut state, &mut original);
    // Backup semantics opens the actual directory and read/write sharing deliberately excludes DELETE sharing.
    // 备份语义打开实际目录，读写共享刻意排除 DELETE 共享。
    let blocker = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x0200_0000)
        .share_mode(0x1 | 0x2)
        .open(&flat_root)
        .expect("actual Windows dormant directory rename blocker");
    // This error is produced by the real operating-system rename after all original admission checks pass.
    // 此错误由全部原准入检查通过后的真实操作系统重命名产生。
    let rename_error = manager
        .collect_deactivation_flat_predecessor(&mut state, &original)
        .expect_err("actual no-DELETE directory handle must refuse rename");
    assert!(rename_error.to_string().contains("os error"));
    assert!(flat_root.is_dir() && !hidden.exists());
    assert_eq!(
        fs::read(&manifest).expect("original survived rename refusal"),
        original_bytes
    );
    assert!(
        manager
            .confirm_deactivation_flat_predecessor_collected(&state, &original)
            .is_err()
    );
    assert_busy(&manager);
    drop(blocker);
    // A valid YAML comment changes the real package fingerprint without inventing a different manifest contract.
    // 有效 YAML 注释改变真实包指纹，不虚构不同清单契约。
    let mut changed_bytes = original_bytes.clone();
    changed_bytes.extend_from_slice(
        "\n# Foreign bytes after actual rename refusal.\n# 实际重命名拒绝后的外来字节。\n"
            .as_bytes(),
    );
    fs::write(&manifest, &changed_bytes)
        .expect("actual changed original source after rename refusal");
    assert!(
        manager
            .collect_deactivation_flat_predecessor(&mut state, &original)
            .expect_err("changed original bytes cannot be admitted on retry")
            .to_string()
            .contains("package bytes changed")
    );
    assert_eq!(
        fs::read(&manifest).expect("changed bytes remain intact"),
        changed_bytes
    );
    assert!(flat_root.is_dir() && !hidden.exists());
    assert_busy(&manager);
    fs::write(&manifest, &original_bytes).expect("test restores the same exact original manifest");
    manager
        .collect_deactivation_flat_predecessor(&mut state, &original)
        .expect("same admitted rename recovers after exact original restoration");
    assert!(!flat_root.exists() && !hidden.exists());
    manager
        .confirm_deactivation_flat_predecessor_collected(&state, &original)
        .expect("same original reaches actual terminal collection");
    assert!(state.package_root.is_dir());
    assert_busy(&manager);
}

/// Actual successful hiding may lose acknowledgement by Err or panic without repeating rename or dropping either original File.
/// 实际成功隐藏可通过 Err 或 panic 丢失确认，而不重复重命名或释放任一原文件。
/// No parameters or return value; the exact existing observer follows the real rename and recovery uses the retained pair.
/// 无参数及返回值；精确既有观察者跟随真实重命名，恢复使用保留配对。
#[test]
fn dormant_flat_hidden_acknowledgement_fault_recovers_same_pair() {
    // Each fault mode owns separate real fixture files and one observer scope.
    // 每种故障模式拥有独立真实夹具文件及单个观察者作用域。
    for panic in [false, true] {
        // Keep actual storage alive across real worker unwinding.
        // 跨真实工作者展开保持实际存储存活。
        let temp = tempfile::tempdir().expect("actual dormant hidden acknowledgement fixture");
        // Both selected and dormant package bytes come from the existing real producer.
        // 已选及休眠包字节均来自既有真实生产者。
        let (manager, store, selected) = fixture(temp.path(), true);
        // The outer pair, rather than the fault wrapper, retains production authority.
        // 外部配对保留生产权威，而非故障包装对象。
        let (mut original, mut state) = reserve(&manager, &store, &selected);
        // The captured descriptor supplies the actual source child.
        // 捕获描述符提供实际来源子级。
        let flat_root = state
            .original_flat_predecessor()
            .expect("actual dormant descriptor")
            .package_root
            .clone();
        // Full-byte fingerprint evidence is retained before the successful physical hide.
        // 完整字节指纹证据在成功物理隐藏之前保留。
        let fingerprint = state
            .original_flat_predecessor()
            .expect("actual dormant fingerprint")
            .content_fingerprint
            .clone();
        // The sole production path rule supplies metadata for filesystem observation.
        // 唯一生产路径规则提供文件系统观测元数据。
        let hidden = hidden_path(&manager, &state);
        collect_local(&manager, &store, &mut state, &mut original);
        // Arm after local collection so its different terminal checkpoint cannot consume this dormant fault.
        // 在本地回收之后启用，使其不同终态检查点无法消费此休眠故障。
        let scope = observation::arm_exact(
            &state.package_root,
            state.plugin_id(),
            state.request_id(),
            observation::Checkpoint::Hidden,
            panic,
        );
        // Catch only the injected post-rename unwind, keeping the real original pair outside the worker closure.
        // 仅捕获注入的重命名后展开，使真实原配对留在工作者闭包之外。
        let result = catch_unwind(AssertUnwindSafe(|| {
            manager.collect_deactivation_flat_predecessor(&mut state, &original)
        }));
        if panic {
            assert!(result.is_err());
        } else {
            assert!(result.expect("post-hide Err must not panic").is_err());
        }
        assert_eq!(manager.state.session_registration_gate.is_poisoned(), panic);
        assert!(!flat_root.exists() && hidden.is_dir());
        assert_eq!(
            crate::catalog::compute_package_fingerprint(&hidden)
                .expect("real hidden bytes remain exact"),
            fingerprint
        );
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (1, 0)
        );
        assert!(
            manager
                .confirm_deactivation_flat_predecessor_collected(&state, &original)
                .is_err()
        );
        assert_busy(&manager);
        manager
            .collect_deactivation_flat_predecessor(&mut state, &original)
            .expect("recover the same actual hidden source");
        assert!(!manager.state.session_registration_gate.is_poisoned());
        assert!(!flat_root.exists() && !hidden.exists());
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (1, 1)
        );
        manager
            .confirm_deactivation_flat_predecessor_collected(&state, &original)
            .expect("confirm actual same-original dormant absence");
        manager
            .collect_deactivation_flat_predecessor(&mut state, &original)
            .expect("terminal retry is read-only");
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (1, 1)
        );
        assert!(state.package_root.is_dir());
        assert_busy(&manager);
        drop(scope);
    }
}

/// Real Windows partial hidden deletion refuses a new visible source and resumes only its own retained hidden original.
/// 真实 Windows 隐藏目录部分删除拒绝新可见来源，并仅恢复自身保留隐藏原来源。
/// No parameters or return value; both post-hide fault modes precede an actual no-DELETE file handle refusal.
/// 无参数及返回值；两种隐藏后故障模式均先于实际无 DELETE 文件句柄拒绝。
#[cfg(windows)]
#[test]
fn dormant_flat_partial_hidden_delete_preserves_new_source_before_recovery() {
    // Each case keeps one exact observer armed through hide, partial delete, conflict and successful GC.
    // 每个用例跨隐藏、部分删除、冲突及成功回收保持单个精确观察者启用。
    for panic in [false, true] {
        // Every actual source and blocker belongs to an independent disposable root.
        // 每个实际来源及阻挡句柄属于独立一次性根。
        let temp = tempfile::tempdir().expect("actual dormant partial hidden deletion fixture");
        // Existing production installation establishes the actual runtime bundle path.
        // 既有生产安装建立实际运行时包路径。
        let (manager, store, selected) = fixture(temp.path(), false);
        // Retain the original selection File and its same local/environment owner throughout all faults.
        // 跨全部故障保留原选择文件及其相同本地／环境所有者。
        let (mut original, mut state) = reserve(&manager, &store, &selected);
        // Use the actual captured source instead of constructing a replacement package location.
        // 使用实际捕获来源，而非构造替代包位置。
        let flat_root = state
            .original_flat_predecessor()
            .expect("actual dormant source")
            .package_root
            .clone();
        // Derive only observation metadata for this actual original hidden child.
        // 仅派生此实际原隐藏子级的观测元数据。
        let hidden = hidden_path(&manager, &state);
        collect_local(&manager, &store, &mut state, &mut original);
        // The existing observer faults only after the actual dormant rename succeeds.
        // 既有观察者仅在实际休眠重命名成功之后故障。
        let scope = observation::arm_exact(
            &state.package_root,
            state.plugin_id(),
            state.request_id(),
            observation::Checkpoint::Hidden,
            panic,
        );
        // The real pair survives either acknowledgement Err or actual panic.
        // 真实配对跨确认 Err 或实际 panic 存续。
        let result = catch_unwind(AssertUnwindSafe(|| {
            manager.collect_deactivation_flat_predecessor(&mut state, &original)
        }));
        if panic {
            assert!(result.is_err());
        } else {
            assert!(
                result
                    .expect("hidden acknowledgement Err must not panic")
                    .is_err()
            );
        }
        assert!(!flat_root.exists() && hidden.is_dir());
        // Concrete actual pre-delete files establish physical progress without relying on an attempted-operation count.
        // 具体实际删除前文件建立物理进度，不依赖操作尝试计数。
        let original_files = actual_file_paths(&hidden);
        assert!(original_files.len() > 1);
        // The actual bundle is produced by write_user_plugin_package and survives the real DELETE-sharing refusal.
        // 实际包由 write_user_plugin_package 生产，并跨真实 DELETE 共享拒绝存续。
        let bundle = hidden.join("runtime/plugin.bundle.lua");
        // Retain original bytes to prove both failed GC and the visible-source conflict preserve this blocked file.
        // 保留原字节，以证明失败回收及可见来源冲突均保留此被阻挡文件。
        let bundle_bytes = fs::read(&bundle).expect("actual hidden original runtime bundle");
        // Share normal readers and writers while physically refusing this file's deletion.
        // 共享普通读者及写者，同时物理拒绝此文件删除。
        let blocker = fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&bundle)
            .expect("actual hidden file no-DELETE blocker");
        assert!(
            manager
                .collect_deactivation_flat_predecessor(&mut state, &original)
                .is_err()
        );
        assert!(!manager.state.session_registration_gate.is_poisoned());
        assert!(hidden.is_dir() && !flat_root.exists());
        assert!(
            original_files
                .iter()
                .any(|relative| !hidden.join(relative).exists()),
            "actual hidden removal must delete at least one original file before the blocked bundle"
        );
        assert_eq!(
            fs::read(&bundle).expect("blocked original bundle remains"),
            bundle_bytes
        );
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (1, 0)
        );
        assert_busy(&manager);
        fs::create_dir(&flat_root)
            .expect("test creates a foreign visible source after partial deletion");
        fs::write(
            flat_root.join("foreign.txt"),
            b"foreign visible source bytes",
        )
        .expect("test-owned foreign visible bytes");
        assert!(
            manager
                .collect_deactivation_flat_predecessor(&mut state, &original)
                .is_err()
        );
        assert_eq!(
            fs::read(flat_root.join("foreign.txt")).expect("foreign visible source is preserved"),
            b"foreign visible source bytes"
        );
        assert_eq!(
            fs::read(&bundle).expect("hidden original is not joined to foreign source"),
            bundle_bytes
        );
        assert!(
            manager
                .confirm_deactivation_flat_predecessor_collected(&state, &original)
                .is_err()
        );
        assert_busy(&manager);
        drop(blocker);
        assert!(
            manager
                .collect_deactivation_flat_predecessor(&mut state, &original)
                .is_err()
        );
        assert_eq!(
            fs::read(flat_root.join("foreign.txt"))
                .expect("foreign source blocks even after real file release"),
            b"foreign visible source bytes"
        );
        // Remove only this test's known foreign file and empty source directory; production never adopts or deletes them.
        // 仅移除此测试已知外来文件及空来源目录；生产绝不接纳或删除它们。
        fs::remove_file(flat_root.join("foreign.txt"))
            .expect("remove only test-owned foreign visible bytes");
        fs::remove_dir(&flat_root).expect("remove only test-owned empty foreign visible source");
        manager
            .collect_deactivation_flat_predecessor(&mut state, &original)
            .expect("same hidden original recovers after blocker and foreign source release");
        assert!(!flat_root.exists() && !hidden.exists());
        manager
            .confirm_deactivation_flat_predecessor_collected(&state, &original)
            .expect("same physical hidden GC becomes terminal");
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (1, 1)
        );
        assert!(state.package_root.is_dir());
        assert_busy(&manager);
        drop(scope);
    }
}

/// Actual terminal collection is retained before an acknowledgement Err or panic and repeated calls never delete resurrected hidden bytes.
/// 实际终态回收先于确认 Err 或 panic 保留，重复调用绝不删除复活隐藏字节。
/// No parameters or return value; checkpoint counts prove that confirmation and GC cannot replay the actual completed producers.
/// 无参数及返回值；检查点计数证明确认及回收无法重放实际已完成生产者。
#[test]
fn dormant_flat_terminal_acknowledgement_fault_never_replays_collection() {
    // Each failure mode has its own original pair and one same-thread scope.
    // 每种失败模式拥有自身原配对及单个同线程作用域。
    for panic in [false, true] {
        // Storage remains alive when the actual manager sequencing guard unwinds.
        // 实际管理器排序守卫展开时，存储保持存活。
        let temp = tempfile::tempdir().expect("actual dormant terminal acknowledgement fixture");
        // Real versioned activation supplies the selected authority without fabricated records.
        // 真实版本化激活提供已选权威，不虚构记录。
        let (manager, store, selected) = fixture(temp.path(), true);
        // These same actual original allocations outlive every fault and terminal retry.
        // 此相同实际原分配比每个故障及终态重试存活更久。
        let (mut original, mut state) = reserve(&manager, &store, &selected);
        // Original descriptor evidence fixes the actual formerly visible source.
        // 原描述符证据固定实际先前可见来源。
        let flat_root = state
            .original_flat_predecessor()
            .expect("actual dormant terminal source")
            .package_root
            .clone();
        // This is observation metadata derived from the same real manager and original request.
        // 此为由相同真实管理器及原请求派生的观测元数据。
        let hidden = hidden_path(&manager, &state);
        collect_local(&manager, &store, &mut state, &mut original);
        // Arm only after local-state terminal confirmation so this fault follows actual dormant GC.
        // 仅在本地状态终态确认之后启用，使此故障跟随实际休眠回收。
        let scope = observation::arm_exact(
            &state.package_root,
            state.plugin_id(),
            state.request_id(),
            observation::Checkpoint::Collected,
            panic,
        );
        // Catch the actual post-GC panic while retaining production originals outside the worker closure.
        // 捕获实际回收后 panic，同时在工作者闭包之外保留生产原实例。
        let result = catch_unwind(AssertUnwindSafe(|| {
            manager.collect_deactivation_flat_predecessor(&mut state, &original)
        }));
        if panic {
            assert!(result.is_err());
        } else {
            assert!(result.expect("post-GC Err must not panic").is_err());
        }
        assert_eq!(manager.state.session_registration_gate.is_poisoned(), panic);
        assert!(!flat_root.exists() && !hidden.exists());
        // A real terminal confirm after the fault proves the saved flag precedes the fallible acknowledgement.
        // 故障后的真实终态确认，证明已保存标志先于可失败确认。
        state
            .flat_predecessor
            .confirm(manager.user_plugin_root())
            .expect("actual terminal evidence survived post-GC fault");
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (1, 1)
        );
        assert_busy(&manager);
        // Multiple actual readbacks cannot replay either the original rename or terminal checkpoint.
        // 多次实际回读无法重放原重命名或终态检查点。
        for _attempt in 0..3 {
            manager
                .confirm_deactivation_flat_predecessor_collected(&state, &original)
                .expect("repeat actual terminal confirm");
            manager
                .collect_deactivation_flat_predecessor(&mut state, &original)
                .expect("repeat terminal GC is read-only");
            assert_eq!(
                observation::counts_exact(
                    &state.package_root,
                    state.plugin_id(),
                    state.request_id()
                ),
                (1, 1)
            );
            assert_busy(&manager);
        }
        assert!(!manager.state.session_registration_gate.is_poisoned());
        fs::create_dir(&hidden)
            .expect("test creates foreign hidden resurrection after actual terminal GC");
        fs::write(hidden.join("foreign.txt"), b"foreign hidden terminal bytes")
            .expect("actual test-owned resurrected hidden bytes");
        assert!(
            manager
                .collect_deactivation_flat_predecessor(&mut state, &original)
                .is_err()
        );
        assert!(
            manager
                .confirm_deactivation_flat_predecessor_collected(&state, &original)
                .is_err()
        );
        assert_eq!(
            fs::read(hidden.join("foreign.txt"))
                .expect("terminal hidden resurrection is preserved"),
            b"foreign hidden terminal bytes"
        );
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (1, 1)
        );
        assert!(state.package_root.is_dir());
        assert_busy(&manager);
        // Remove only this test's resurrection, then prove the original terminal evidence was never cleared.
        // 仅移除此测试复活内容，然后证明原终态证据从未被清除。
        fs::remove_file(hidden.join("foreign.txt"))
            .expect("remove only test-owned terminal hidden bytes");
        fs::remove_dir(&hidden).expect("remove only test-owned empty terminal hidden directory");
        manager
            .confirm_deactivation_flat_predecessor_collected(&state, &original)
            .expect("same original still retains terminal evidence");
        assert_eq!(
            observation::counts_exact(&state.package_root, state.plugin_id(), state.request_id()),
            (1, 1)
        );
        drop(scope);
    }
}

/// A private collection's real final validator Err or panic cannot erase already completed deletion or authorize hidden resurrection GC.
/// 私有回收真实最终校验器 Err 或 panic 无法清除已完成删除，或授权隐藏复活回收。
/// No parameters or return value; the Cell records a fault only after both actual original paths are physically absent.
/// 无参数及返回值；Cell 仅在两个实际原路径均物理缺失之后记录故障。
#[test]
fn dormant_flat_post_delete_validation_fault_retains_true_terminal_evidence() {
    // Exercise Err and real unwinding using separate actual production originals.
    // 使用独立实际生产原实例覆盖 Err 及真实展开。
    for panic in [false, true] {
        // Actual package and selection bytes remain available outside the fallible worker closure.
        // 实际包及选择字节在可失败工作者闭包之外保持可用。
        let temp = tempfile::tempdir().expect("actual dormant post-delete validation fixture");
        // The fixture constructs the actual manager, store and full selection through production writers.
        // 夹具通过生产写入器构造实际管理器、存储及完整选择。
        let (manager, store, selected) = fixture(temp.path(), false);
        // Strong original ownership is retained independently of every injected validator outcome.
        // 强原所有权独立于每个注入校验器结果保持保留。
        let (mut original, mut state) = reserve(&manager, &store, &selected);
        // The actual captured source is present before the first validation; absence here cannot be fabricated.
        // 实际捕获来源在首次校验之前存在；此处缺失不能被虚构。
        let flat_root = state
            .original_flat_predecessor()
            .expect("actual dormant post-delete source")
            .package_root
            .clone();
        // This fixed hidden child comes from the same original production naming rule.
        // 此固定隐藏子级来自相同原生产命名规则。
        let hidden = hidden_path(&manager, &state);
        collect_local(&manager, &store, &mut state, &mut original);
        assert!(flat_root.is_dir() && !hidden.exists());
        // Record only real filesystem absence; no call counter chooses which validator invocation fails.
        // 仅记录真实文件系统缺失；不使用调用计数选择失败校验器调用。
        let fault_observed = Cell::new(false);
        // Invoke the actual tree producer with its real selection validator before raising the final fault.
        // 在引发最终故障之前，使用真实选择校验器调用实际目录树生产者。
        let result = catch_unwind(AssertUnwindSafe(|| {
            state.flat_predecessor.collect(
                manager.user_plugin_root(),
                || {
                    original
                        .validate_published_for_catalog(&store)
                        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
                    // Hidden absence alone also precedes rename; require visible absence to identify actual completed deletion.
                    // 单独隐藏缺失也先于重命名；同时要求可见缺失，以标识实际已完成删除。
                    if !flat_root.exists() && !hidden.exists() {
                        fault_observed.set(true);
                        if panic {
                            panic!(
                                "original validator unwinds only after actual dormant tree deletion"
                            );
                        }
                        return Err(VulcanPluginRuntimeError::new(
                            "Original validator failed after actual dormant tree deletion.",
                        ));
                    }
                    Ok(())
                },
                || Ok(()),
            )
        }));
        if panic {
            assert!(result.is_err());
        } else {
            assert!(
                result
                    .expect("post-delete validation Err must not panic")
                    .is_err()
            );
        }
        assert!(
            fault_observed.get(),
            "fault must follow actual hidden deletion, not a simulated invocation count"
        );
        assert!(!flat_root.exists() && !hidden.exists());
        state
            .flat_predecessor
            .confirm(manager.user_plugin_root())
            .expect("terminal evidence is retained before final validation Err or panic");
        manager
            .confirm_deactivation_flat_predecessor_collected(&state, &original)
            .expect("same real paired authority confirms completed private deletion");
        manager
            .collect_deactivation_flat_predecessor(&mut state, &original)
            .expect("terminal recovery cannot repeat the original tree deletion");
        assert_busy(&manager);
        fs::create_dir(&hidden)
            .expect("actual foreign hidden resurrection after final validation fault");
        fs::write(
            hidden.join("foreign.txt"),
            b"foreign hidden post-validator bytes",
        )
        .expect("actual foreign hidden post-validator bytes");
        assert!(
            state
                .flat_predecessor
                .collect(
                    manager.user_plugin_root(),
                    || original
                        .validate_published_for_catalog(&store)
                        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string())),
                    || Ok(()),
                )
                .is_err()
        );
        assert!(
            manager
                .collect_deactivation_flat_predecessor(&mut state, &original)
                .is_err()
        );
        assert!(
            state
                .flat_predecessor
                .confirm(manager.user_plugin_root())
                .is_err()
        );
        assert_eq!(
            fs::read(hidden.join("foreign.txt"))
                .expect("foreign hidden bytes cannot be deleted after final validation fault"),
            b"foreign hidden post-validator bytes"
        );
        assert!(state.package_root.is_dir());
        assert_busy(&manager);
        // Only the test removes its own foreign bytes, making absence observable without altering original terminal state.
        // 仅测试移除自身外来字节，使缺失可被观测，不改变原终态状态。
        fs::remove_file(hidden.join("foreign.txt"))
            .expect("remove only test-owned post-validator resurrection bytes");
        fs::remove_dir(&hidden)
            .expect("remove only test-owned empty post-validator resurrection directory");
        state
            .flat_predecessor
            .confirm(manager.user_plugin_root())
            .expect("terminal flag remains retained after rejected hidden resurrection");
        manager
            .confirm_deactivation_flat_predecessor_collected(&state, &original)
            .expect("same actual paired terminal proof survives every validator fault");
        assert_busy(&manager);
    }
}
