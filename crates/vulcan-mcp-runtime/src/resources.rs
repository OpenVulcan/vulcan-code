//! Host resources follow actual transport owners independently of launch and cleanup observers.
//! 宿主资源跟随实际传输所有者，独立于启动及清理观察者。

use crate::McpRuntimeError;
use std::{io, sync::Arc};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::watch;

/// One launch's retained host authority, shared only with its actual resource owners.
/// 单次启动保留的宿主权威，仅与实际资源所有者共享。
/// A receipt observes release of this launch's references, not other independent host references.
/// 回执观察此次启动引用的释放，不代表其他独立宿主引用也已释放。
#[derive(Clone)]
pub struct McpTransportResources {
    /// The last owner releases the supplied bundle before publishing completion.
    /// 最后所有者先释放传入资源包，再发布完成。
    inner: Arc<RetainedHostResources>,
}

/// Owns the exact host bundle without requiring transport code to know plugin or credential types.
/// 拥有精确宿主资源包，无需传输代码了解插件或凭据类型。
struct RetainedHostResources {
    /// Optional storage permits explicit release before notifying observers.
    /// 可选存储允许在通知观察者前显式释放。
    bundle: Option<Box<dyn Send + Sync>>,
    /// Only final destruction publishes true; absence of evidence remains an error.
    /// 仅最终析构发布真值；缺少证据仍为错误。
    released: watch::Sender<bool>,
}

/// Observes one launch's actual resource references without retaining or cancelling them.
/// 观察单次启动的实际资源引用，不持有或取消这些引用。
/// This proves release only; protocol delivery and cleanup errors retain their own result channels.
/// 此对象仅证明释放；协议交付及清理错误保留自身结果通道。
#[derive(Clone)]
pub struct McpResourceRelease {
    /// Independent observers cannot consume or alter each other's retained result.
    /// 独立观察者不能消费或修改彼此保留的结果。
    released: watch::Receiver<bool>,
}

impl McpTransportResources {
    /// Creates a launch resource owner and an independent final-release observation.
    /// 创建启动资源所有者及独立最终释放观察。
    /// Bundle owns the exact approved host resources; returns ownership and its original release receipt.
    /// bundle 拥有精确已批准宿主资源；返回所有权及其原释放回执。
    pub fn new(bundle: impl Send + Sync + 'static) -> (Self, McpResourceRelease) {
        // The channel carries evidence only, never a resource-owning reference.
        // 通道仅携带证据，绝不携带拥有资源的引用。
        let (released, receiver) = watch::channel(false);
        (
            Self {
                inner: Arc::new(RetainedHostResources {
                    bundle: Some(Box::new(bundle)),
                    released,
                }),
            },
            McpResourceRelease { released: receiver },
        )
    }

    /// Attaches this launch's exact host authority to an actual native resource.
    /// 将此次启动的精确宿主权威附着到实际原生资源。
    /// Resource is consumed; the wrapper drops it before dropping its host authority.
    /// 消费 resource；包装先释放该资源，再释放宿主权威。
    pub(crate) fn retain<T>(&self, resource: T) -> RetainedResource<T> {
        RetainedResource {
            resource,
            _resources: Self {
                inner: Arc::clone(&self.inner),
            },
        }
    }
}

impl Drop for RetainedHostResources {
    /// Releases the original bundle before any observer may report actual completion.
    /// 任何观察者报告实际完成之前，释放原资源包。
    /// No inputs or return; a panicking bundle destructor cannot publish successful release.
    /// 无输入或返回；资源包析构恐慌不能发布成功释放。
    fn drop(&mut self) {
        drop(self.bundle.take());
        self.released.send_replace(true);
    }
}

impl McpResourceRelease {
    /// Waits for final actual release while leaving cleanup entirely with the original owners.
    /// 等待最终实际释放，清理完全由原所有者负责。
    /// No inputs; cancellation stops only this wait, and a missing final publication is explicit failure.
    /// 无输入；取消仅停止此次等待，缺少最终发布是明确失败。
    pub async fn wait(&self) -> Result<(), McpRuntimeError> {
        // Each observer owns a private cursor but no reference to the retained resource bundle.
        // 每个观察者拥有独立游标，但没有对保留资源包的引用。
        let mut receiver = self.released.clone();
        loop {
            if *receiver.borrow_and_update() {
                return Ok(());
            }
            receiver.changed().await.map_err(|_| {
                McpRuntimeError::transport("MCP resource owner ended without confirming release.")
            })?;
        }
    }
}

/// Binds one actual handle, client or response stream to its original launch authority.
/// 将单个实际句柄、客户端或响应流绑定到其原启动权威。
/// Rust drops struct fields in declaration order; this order is part of the ownership contract.
/// Rust 按声明顺序释放结构体字段；此顺序是所有权契约的一部分。
pub(crate) struct RetainedResource<T> {
    /// Actual wrapped ownership is released first, including during stack unwinding.
    /// 实际包装所有权首先释放，栈展开期间也如此。
    resource: T,
    /// Host leases cannot disappear before the actual resource above.
    /// 宿主租约不能在上方实际资源之前消失。
    _resources: McpTransportResources,
}

impl<T> RetainedResource<T> {
    /// Binds a dependent resource to the same original host authority.
    /// 将依赖资源绑定到相同原宿主权威。
    /// Resource is consumed; the returned wrapper may outlive the client that created it.
    /// 消费 resource；返回包装可以比创建它的客户端存续更久。
    pub(crate) fn retain<U>(&self, resource: U) -> RetainedResource<U> {
        self._resources.retain(resource)
    }
}

impl<T> AsRef<T> for RetainedResource<T> {
    /// Borrows the original resource without transferring or cloning its native ownership.
    /// 借用原资源，不移交或克隆其原生所有权。
    /// No inputs; the borrow cannot outlive this lease-bound wrapper.
    /// 无输入；借用不能比此绑定租约的包装存续更久。
    fn as_ref(&self) -> &T {
        &self.resource
    }
}

impl<T: Clone> Clone for RetainedResource<T> {
    /// Clones the underlying shared resource together with the same host authority.
    /// 将底层共享资源与相同宿主权威一起克隆。
    /// No inputs; returns a new actual owner retaining the original launch scope.
    /// 无输入；返回保留原启动范围的新实际所有者。
    fn clone(&self) -> Self {
        Self {
            resource: self.resource.clone(),
            _resources: self._resources.clone(),
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for RetainedResource<T> {
    /// Preserves the resource's existing debug view while omitting the opaque host authority.
    /// 保留资源既有调试视图，同时省略不透明宿主权威。
    /// Formatter receives only the wrapped resource's existing representation; returns formatting status.
    /// formatter 仅接收包装资源的既有表示；返回格式化状态。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RetainedResource")
            .field("resource", &self.resource)
            .finish_non_exhaustive()
    }
}

impl<T: futures::Stream + Unpin> futures::Stream for RetainedResource<T> {
    /// Items preserve the original response stream's value and error types.
    /// 条目保留原响应流的值与错误类型。
    type Item = T::Item;

    /// Polls the exact response stream while keeping its host authority attached until disposal.
    /// 轮询精确响应流，将其宿主权威保留至销毁。
    /// Context registers the observer; returns the original stream item without owning observer lifetime.
    /// context 注册观察者；返回原流条目，不拥有观察者生命周期。
    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.get_mut().resource).poll_next(context)
    }
}

impl<T: io::Read> io::Read for RetainedResource<T> {
    /// Reads through the exact original resource while its host leases remain owned.
    /// 宿主租约保持持有期间，经精确原资源读取。
    /// Bytes is the caller's destination; returns the unchanged native read result.
    /// bytes 是调用方目标；返回未改变的原生读取结果。
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.resource.read(bytes)
    }
}

impl<T: io::Write> io::Write for RetainedResource<T> {
    /// Writes the supplied bytes without separating native and host resource ownership.
    /// 写入给定字节，不分离原生及宿主资源所有权。
    /// Bytes is borrowed for this operation; returns the original native write result.
    /// 此操作借用 bytes；返回原原生写入结果。
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.resource.write(bytes)
    }

    /// Flushes the original writer while retaining the same host resource authority.
    /// 刷新原写入者，同时保留相同宿主资源权威。
    /// No inputs; returns the original flush result without synthesizing completion.
    /// 无输入；返回原刷新结果，不合成完成。
    fn flush(&mut self) -> io::Result<()> {
        self.resource.flush()
    }
}
