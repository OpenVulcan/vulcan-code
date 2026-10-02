//! Scoped test observation loses one return only after the actual exact OS write succeeds.
//! 作用域测试观测仅在实际精确操作系统写入成功后丢失一次返回。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Non-secret observations retain booleans and counts, never the credential or its OS entry.
/// 无秘密观测保留布尔值及计数，绝不保留凭据或其操作系统条目。
#[derive(Default)]
struct Observation {
    /// The actual post-write read compared equal without retaining either value.
    /// 实际写后读取比较相等，不保留任一值。
    confirmed_write: Cell<bool>,
    /// An independent actual Store contender encountered the still-owned namespace fence.
    /// 独立实际存储竞争者遇到仍被持有的命名空间屏障。
    confirmed_busy: Cell<bool>,
    /// Exact-account adapter entries are counted before any native account access.
    /// 精确账户适配器入口在任何原生账户访问前被计数。
    accesses: Cell<usize>,
}

/// A test arms only one explicitly allocated UUID account on its current executor thread.
/// 测试仅在当前执行器线程对单个明确分配的 UUID 账户布防。
struct ScopeState {
    /// Exact UUID-based plugin identity is neither a wildcard nor a production policy.
    /// 精确 UUID 插件身份既不是通配符，也不是生产策略。
    plugin_id: String,
    /// Only this declared server belongs to the armed account.
    /// 仅此已声明服务器属于已布防账户。
    server_id: String,
    /// Only this declared key belongs to the armed account.
    /// 仅此已声明密钥键属于已布防账户。
    secret_key: String,
    /// Shared observations outlive the one-shot failure without carrying secret bytes.
    /// 共享观测存续至一次性失败之后，不携带秘密字节。
    observation: Rc<Observation>,
    /// Taking the independent contender probe consumes the return-loss fault exactly once.
    /// 取出独立竞争者探针，精确一次消费返回丢失故障。
    probe: Option<Box<dyn FnOnce() -> bool>>,
    /// A return loss and a task panic have distinct ownership lifetimes after the same successful native effect.
    /// 返回丢失及任务 panic 在相同成功原生副作用后具有不同所有权生命周期。
    failure: WriteFailure,
}

/// Select one exact post-write observer failure without altering the native credential backend.
/// 选择单个精确写后观察者故障，不改变原生凭据后端。
#[derive(Clone, Copy)]
enum WriteFailure {
    /// Return a redacted error after actual successful OS write and readback.
    /// 实际成功操作系统写入及回读后，返回脱敏错误。
    ReturnLoss,
    /// Panic the actual management executor after successful native write and readback.
    /// 成功原生写入及回读后，使实际管理执行器 panic。
    Panic,
}

thread_local! {
    /// Current-thread isolation prevents another test or unrelated task from consuming the exact account's fault.
    /// 当前线程隔离防止其他测试或无关任务消费精确账户故障。
    static STATE: RefCell<Option<ScopeState>> = const { RefCell::new(None) };
}

/// Non-Send RAII ownership clears only its own current-thread test slot on every exit.
/// 不可发送的 RAII 所有权在每次退出时仅清除自身当前线程测试槽位。
pub(crate) struct Scope {
    /// Rc intentionally prevents moving this owner to another executor thread.
    /// Rc 故意禁止将此所有者移动到其他执行器线程。
    observation: Rc<Observation>,
}

impl Scope {
    /// Arm one actual successful write's return loss; exact identifiers select the account and probe observes its real fence.
    /// 布防单次实际成功写入的返回丢失；精确标识选择账户，probe 观察其真实屏障。
    /// Returns current-thread RAII ownership without changing any OS credential or global keyring builder.
    /// 返回当前线程 RAII 所有权，不改变任何操作系统凭据或全局密钥库构建器。
    pub(crate) fn lose_next_write(
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
        probe: impl FnOnce() -> bool + 'static,
    ) -> Self {
        Self::arm(
            plugin_id,
            server_id,
            secret_key,
            Some(Box::new(probe)),
            WriteFailure::ReturnLoss,
        )
    }

    /// Panic after one real successful OS write; the exact account and probe prove its physical fence first.
    /// 在单次真实成功操作系统写入后 panic；精确账户及探针先证明其物理屏障。
    /// Returns a scoped observation that survives the management task's unwind without retaining a credential.
    /// 返回作用域观测，跨管理任务展开栈存续，不保留凭据。
    pub(crate) fn panic_next_write(
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
        probe: impl FnOnce() -> bool + 'static,
    ) -> Self {
        Self::arm(
            plugin_id,
            server_id,
            secret_key,
            Some(Box::new(probe)),
            WriteFailure::Panic,
        )
    }

    /// Count only exact-account adapter access during one test scope; no return loss or backend effect is injected.
    /// 在单个测试作用域仅计数精确账户适配器访问；不注入返回丢失或后端副作用。
    pub(crate) fn count_access(plugin_id: &str, server_id: &str, secret_key: &str) -> Self {
        Self::arm(
            plugin_id,
            server_id,
            secret_key,
            None,
            WriteFailure::ReturnLoss,
        )
    }

    /// Install one UUID-scoped slot and return its sole non-Send owner; overlapping scopes are a fixture error.
    /// 安装单个 UUID 作用域槽位并返回其唯一不可发送所有者；重叠作用域是夹具错误。
    fn arm(
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
        probe: Option<Box<dyn FnOnce() -> bool>>,
        failure: WriteFailure,
    ) -> Self {
        // Only disposable Legacy fixture UUID namespaces are accepted by this test-only hook.
        // 此仅测试钩子仅接受可丢弃的旧格式夹具 UUID 命名空间。
        let allocated = plugin_id
            .strip_prefix("legacy-secret-")
            .and_then(|id| uuid::Uuid::parse_str(id).ok());
        assert!(
            allocated.is_some_and(|id| !id.is_nil()),
            "fault requires an allocated fixture UUID"
        );
        // Observations carry no secret and are private to the exact scope identity.
        // 观测不携带秘密，并专属于精确作用域身份。
        let observation = Rc::new(Observation::default());
        STATE.with(|slot| {
            // Keep initialization atomic with overlap rejection on the current thread.
            // 在当前线程将初始化与重叠拒绝保持原子。
            let mut state = slot.borrow_mut();
            assert!(
                state.is_none(),
                "Legacy write observation scopes must not overlap"
            );
            *state = Some(ScopeState {
                plugin_id: plugin_id.into(),
                server_id: server_id.into(),
                secret_key: secret_key.into(),
                observation: Rc::clone(&observation),
                probe,
                failure,
            });
        });
        Self { observation }
    }

    /// Return whether actual OS readback confirmed the candidate value before the injected lost return.
    /// 返回注入丢失返回前，实际操作系统回读是否确认候选值。
    pub(crate) fn confirmed_write(&self) -> bool {
        self.observation.confirmed_write.get()
    }

    /// Return whether the independent namespace contender observed Busy while the original setter owned its guard.
    /// 返回原设置器持有其屏障时，独立命名空间竞争者是否观察到 Busy。
    pub(crate) fn confirmed_busy(&self) -> bool {
        self.observation.confirmed_busy.get()
    }

    /// Return this exact account's adapter access count without revealing any value or native entry.
    /// 返回此精确账户适配器访问计数，不揭示任何值或原生条目。
    pub(crate) fn access_count(&self) -> usize {
        self.observation.accesses.get()
    }
}

impl Drop for Scope {
    /// Clear only this original slot even during a failed test; unrelated executor threads remain untouched.
    /// 即使测试失败也仅清除此原槽位；无关执行器线程保持不变。
    fn drop(&mut self) {
        STATE.with(|slot| {
            // Exact Rc identity prevents this owner from removing any later or unrelated slot.
            // 精确 Rc 身份防止此所有者移除任何后继或无关槽位。
            let mut state = slot.borrow_mut();
            if state
                .as_ref()
                .is_some_and(|state| Rc::ptr_eq(&state.observation, &self.observation))
            {
                state.take();
            }
        });
    }
}

/// Count plugin/server/key only when every exact account identity matches the currently armed UUID scope.
/// 仅在全部精确账户身份匹配当前已布防 UUID 作用域时，计数 plugin／server／key。
pub(super) fn record_access(plugin_id: &str, server_id: &str, secret_key: &str) {
    STATE.with(|slot| {
        if let Some(state) = slot.borrow().as_ref()
            && state.plugin_id == plugin_id
            && state.server_id == server_id
            && state.secret_key == secret_key
        {
            state
                .observation
                .accesses
                .set(state.observation.accesses.get() + 1);
        }
    });
}

/// Observe one completed actual set_password for the exact account, then consume a single static redacted error.
/// 观察精确账户的一次已完成实际 set_password，然后消费单个静态脱敏错误。
/// Expected is borrowed only for immediate equality; no secret is stored, formatted or serialized.
/// expected 仅借用于即时相等比较；不存储、格式化或序列化任何秘密。
pub(super) fn after_successful_write(
    plugin_id: &str,
    server_id: &str,
    secret_key: &str,
    expected: &str,
) -> Result<(), String> {
    // Extract the one-shot probe before any native read or independent lock attempt can reenter the adapter.
    // 在任何原生读取或独立锁尝试可重入适配器之前，取出一次性探针。
    let fault = STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let state = state.as_mut()?;
        if state.plugin_id != plugin_id
            || state.server_id != server_id
            || state.secret_key != secret_key
        {
            return None;
        }
        state
            .probe
            .take()
            .map(|probe| (Rc::clone(&state.observation), probe, state.failure))
    });
    let Some((observation, probe, failure)) = fault else {
        return Ok(());
    };
    // Real readback witnesses the successful native mutation; only its equality boolean survives this expression.
    // 真实回读见证成功原生变更；仅其相等布尔值存续至此表达式之后。
    observation.confirmed_write.set(
        match super::plugin_secret_entry(plugin_id, server_id, secret_key) {
            Ok(entry) => match entry.get_password() {
                Ok(actual) => actual == expected,
                Err(_) => false,
            },
            Err(_) => false,
        },
    );
    observation.confirmed_busy.set(probe());
    if matches!(failure, WriteFailure::Panic) {
        panic!("Injected task panic after exact native credential write succeeded.");
    }
    Err(String::from(
        "Injected loss after the exact native credential write succeeded.",
    ))
}
