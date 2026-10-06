//! Same-thread scoped acknowledgement faults follow true final physical absence confirmation.
//! 同线程作用域确认故障发生于真实最终物理缺失确认之后。

use super::*;
use std::cell::RefCell;

/// Full literal binding excludes every unrelated root, plugin and request.
/// 完整字面绑定排除每个无关根、插件及请求。
#[derive(PartialEq, Eq)]
struct Key {
    /// Actual canonical original package root.
    /// 实际规范原包根。
    root: PathBuf,
    /// Actual original plugin identity.
    /// 实际原插件身份。
    plugin_id: String,
    /// Actual host-retained original request UUID.
    /// 实际宿主保留原请求 UUID。
    request: Uuid,
}

/// One bound test records actual terminal confirmations and one optional post-success error or panic.
/// 单个绑定测试记录实际终态确认及单次可选成功后错误或 panic。
struct Fault {
    /// Full original binding, independent of local observer wrappers.
    /// 完整原绑定，独立于局部观察者包装对象。
    key: Key,
    /// False returns an error and true raises a real panic after success.
    /// 假返回错误，真在成功后引发真实 panic。
    panic: Option<bool>,
    /// Counts retained terminal confirmations, not attempted deletes.
    /// 计数保留的终态确认，而非删除尝试。
    collected: usize,
}

thread_local! {
    /// Test-only scoped state never allocates or releases production resource authority.
    /// 仅测试作用域状态绝不分配或释放生产资源权威。
    static FAULT: RefCell<Option<Fault>> = const { RefCell::new(None) };
}

/// Scope removal clears only its exact bound observation; it never owns the original namespace or environment File.
/// 作用域移除仅清除精确绑定观测；其绝不拥有原命名空间或环境文件。
pub(super) struct Scope(Key);

impl Drop for Scope {
    /// Clear this same-thread binding; no return value and no production cleanup occurs.
    /// 清除此同线程绑定；无返回值，不发生生产清理。
    fn drop(&mut self) {
        FAULT.with(|slot| {
            // The observer identity is checked independently of production ownership.
            // 观察者身份独立于生产所有权被检查。
            let mut slot = slot.borrow_mut();
            assert!(slot.as_ref().is_some_and(|fault| fault.key == self.0));
            *slot = None;
        });
    }
}

/// Arm an exact metadata-bound observer after its actual original is already retained; returns no physical ownership.
/// 实际原实例已经保留之后，启用精确元数据绑定观察者；不返回物理所有权。
/// root/plugin_id/request come from the actual original; panic selects the single post-success acknowledgement fault.
/// root／plugin_id／request 来自实际原实例；panic 选择单次成功后确认故障。
pub(super) fn arm_exact(root: &Path, plugin_id: &str, request: Uuid, panic: bool) -> Scope {
    // Test metadata selects observation only and cannot reconstruct any production resource owner.
    // 测试元数据仅选择观测，不能重建任何生产资源所有者。
    let key = || Key {
        root: root.into(),
        plugin_id: plugin_id.into(),
        request,
    };
    FAULT.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Fault {
            key: key(),
            panic: Some(panic),
            collected: 0,
        });
    });
    Scope(key())
}

/// Observe a true terminal confirmation using exact actual metadata; returns a one-shot error/panic without changing physical authority.
/// 使用精确实际元数据观察真实终态确认；返回单次错误／panic，不改变物理权威。
/// root/plugin_id/request are actual retained original metadata and the call follows actual producer success only.
/// root／plugin_id／request 为实际保留原元数据，调用仅在实际生产者成功之后执行。
pub(super) fn checkpoint_exact(
    root: &Path,
    plugin_id: &str,
    request: Uuid,
) -> Result<(), VulcanPluginRuntimeError> {
    // Comparison metadata is test-only; the production caller still retains every actual File.
    // 比较元数据仅用于测试；生产调用方仍保留每个实际文件。
    let binding = Key {
        root: root.into(),
        plugin_id: plugin_id.into(),
        request,
    };
    // Consume the one-shot fault before raising a panic so real unwinding never borrows this observer state.
    // 在引发 panic 之前消费单次故障，使真实展开绝不借用此观察者状态。
    let fault = FAULT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let fault = slot.as_mut()?;
        if fault.key != binding {
            return None;
        }
        fault.collected += 1;
        fault.panic.take()
    });
    match fault {
        Some(true) => panic!("original uninstall acknowledgement lost after actual success"),
        Some(false) => Err(VulcanPluginRuntimeError::new(
            "Original uninstall acknowledgement lost after actual success.",
        )),
        None => Ok(()),
    }
}

/// Read real bound terminal confirmation counts; root/plugin_id/request select the armed actual original and confer no authority.
/// 读取真实绑定终态确认计数；root／plugin_id／request 选择已启用实际原实例，不授予权威。
pub(super) fn counts_exact(root: &Path, plugin_id: &str, request: Uuid) -> usize {
    // The expected observation identity contains no resource owner or recovery state.
    // 期望观测身份不包含资源所有者或恢复状态。
    let binding = Key {
        root: root.into(),
        plugin_id: plugin_id.into(),
        request,
    };
    FAULT.with(|slot| {
        // Absence is a test error, never fabricated zero evidence.
        // 缺失为测试错误，绝不虚构零证据。
        let slot = slot.borrow();
        let fault = slot
            .as_ref()
            .expect("same-thread original observer remains armed");
        assert!(fault.key == binding);
        fault.collected
    })
}
