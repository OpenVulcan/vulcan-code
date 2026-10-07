//! Core-owned lazy rendering listener and revocable, result-scoped viewing capabilities.
//! Core 持有的按需渲染监听器及可撤销、限定结果的查看能力。

use super::{
    HtmlAnswerOrigins,
    grants::{Grant, Grants},
    routes, validate_origin,
};
use axum::Router;
use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::Mutex as AsyncMutex, task::JoinHandle};
use vulcan_service_api::{
    HtmlAnswerSourceIdentity, HtmlAnswerViewError, HtmlAnswerViewLinkRequest,
    HtmlAnswerViewLinkResponse,
};
use vulcan_session_core::v1::html_answers::*;

/// Freshly validated source bytes, borrowed from no persistent secondary store.
/// 刚刚校验的源码字节，不来自任何持久化副本存储。
pub struct HtmlAnswerContent {
    /// Validated plain-text result title.
    /// 已校验的纯文本结果标题。
    pub title: String,
    /// Exact UTF-8 source range reconstructed from the original assistant event.
    /// 从原助手事件重建的精确 UTF-8 源码范围。
    pub source: String,
    /// Compiled draft only; raw version-one answers retain their original source without a second copy.
    /// 仅保存编译后的草稿；版本一原始回答保留源码，不生成第二份副本。
    rendered_html: Option<String>,
}

impl HtmlAnswerContent {
    /// Build content from a freshly authorized `source`, declared `template` and validated `title`.
    /// 根据刚获授权的 `source`、声明的 `template` 及已校验的 `title` 构建内容。
    /// Returns an error for invalid drafts; the kernel calls this within its blocking stored-source read.
    /// 非法草稿返回错误；内核在阻塞式持久源码读取过程中调用此方法。
    pub fn from_source(
        title: String,
        source: String,
        template: Option<templates::HtmlAnswerTemplate>,
    ) -> Result<Self, HtmlAnswerViewError> {
        let rendered_html = template
            .map(|template| templates::render_draft(template, &title, &source))
            .transpose()
            .map_err(|_| HtmlAnswerViewError::InvalidRequest)?;
        Ok(Self {
            title,
            source,
            rendered_html,
        })
    }

    /// Consume the transient result and return its explicitly selected executable representation.
    /// 消费瞬态结果，返回由声明明确选择的可执行表示。
    pub(super) fn into_html(self) -> String {
        match self.rendered_html {
            Some(html) => html,
            None => self.source,
        }
    }
}

/// Async authority callback; the kernel must recheck storage, generation, revision and complete identity.
/// 异步权威回调；内核必须重新校验存储、代次、修订和完整身份。
pub type HtmlAnswerSourceResolver = Arc<
    dyn Fn(
            HtmlAnswerSourceIdentity,
        )
            -> Pin<Box<dyn Future<Output = Result<HtmlAnswerContent, HtmlAnswerViewError>> + Send>>
        + Send
        + Sync,
>;

/// Explicit transport configuration; incoming Host or forwarding headers never select these values.
/// 显式传输配置；传入的 Host 或转发头绝不选择这些值。
pub struct HtmlAnswerRuntimeConfig {
    /// Canonical trusted Core origin, locally discovered or explicitly configured for a proxy.
    /// 规范可信 Core 来源，由本地发现或代理显式配置提供。
    pub viewer_origin: String,
    /// Explicit external renderer origin for a proxy; None uses the actually bound loopback address.
    /// 代理的显式外部渲染来源；为空时使用实际绑定的回环地址。
    pub renderer_origin: Option<String>,
    /// Loopback render port; zero requests an ephemeral local port without exposing a public listener.
    /// 回环渲染端口；零表示临时本地端口，不暴露公开监听器。
    pub renderer_port: u16,
    /// Built WebUI root containing the trusted standalone viewer entry.
    /// 包含可信独立查看器入口的已构建 WebUI 根目录。
    pub static_root: Option<PathBuf>,
}

/// Listener-independent shared authority; no task holds the outer runtime and creates an ownership cycle.
/// 独立于监听器的共享权威；任务不持有外层运行时，避免所有权环。
pub(super) struct RenderState {
    /// Short-held capability table lock, never retained across await.
    /// 短时持有的能力表锁，绝不跨 await 保留。
    pub grants: Mutex<Grants>,
    /// Weak-kernel callback supplied by the owner.
    /// 所有者提供的弱内核引用回调。
    pub resolver: HtmlAnswerSourceResolver,
    /// Actual bound policy, initialized once after the first authorized open.
    /// 首次授权打开后初始化一次的实际绑定策略。
    pub origins: OnceLock<HtmlAnswerOrigins>,
}

/// Unique Core-owned lifetime for the lazy listener and every result capability.
/// 按需监听器及全部结果能力的唯一 Core 生命周期归属。
pub struct HtmlAnswerRuntime {
    /// Immutable trusted transport configuration.
    /// 不可变可信传输配置。
    pub(super) config: HtmlAnswerRuntimeConfig,
    /// Shared routing authority without an outer-runtime back reference.
    /// 不含外层运行时反向引用的共享路由权威。
    pub(super) shared: Arc<RenderState>,
    /// Serialized lazy binding without retaining the synchronous listener lock across await.
    /// 串行的按需绑定，不跨 await 持有同步监听器锁。
    startup: AsyncMutex<()>,
    /// Abortable listener ownership available even from a cancelled parent future's Drop.
    /// 即使父 future 取消后的 Drop 中也可使用的可中止监听器所有权。
    listener: Mutex<Option<JoinHandle<()>>>,
}

impl HtmlAnswerRuntime {
    /// Validate `config` and install `resolver` without opening sockets; return the shared owner.
    /// 校验 `config` 并安装 `resolver`，不打开套接字；返回共享所有者。
    pub fn new(
        config: HtmlAnswerRuntimeConfig,
        resolver: HtmlAnswerSourceResolver,
    ) -> Result<Arc<Self>, &'static str> {
        validate_origin(&config.viewer_origin)?;
        if let Some(renderer) = &config.renderer_origin {
            HtmlAnswerOrigins::new(&config.viewer_origin, renderer)?;
            if config.renderer_port == 0 {
                return Err("Proxy renderer requires an explicit loopback port");
            }
        }
        Ok(Arc::new(Self {
            config,
            shared: Arc::new(RenderState {
                grants: Mutex::new(Grants::default()),
                resolver,
                origins: OnceLock::new(),
            }),
            startup: AsyncMutex::new(()),
            listener: Mutex::new(None),
        }))
    }

    /// Build only trusted viewer and exchange routes; generated content is served on a separate listener.
    /// 仅构造可信查看器及兑换路由；生成内容在独立监听器提供。
    pub fn viewer_router(self: &Arc<Self>) -> Router {
        routes::viewer_router(self.clone())
    }

    /// Validate one authenticated `request`, start isolation lazily, and return a temporary fragment link.
    /// 校验一份已鉴权 `request`，按需启动隔离来源，并返回临时片段链接。
    pub async fn issue(
        &self,
        request: HtmlAnswerViewLinkRequest,
    ) -> Result<HtmlAnswerViewLinkResponse, HtmlAnswerViewError> {
        if !html_answer_identity_is_valid(&request.source.artifact_id) {
            return Err(HtmlAnswerViewError::InvalidRequest);
        }
        if !self
            .config
            .static_root
            .as_ref()
            .is_some_and(|root| root.join("html-answer-viewer.html").is_file())
        {
            return Err(HtmlAnswerViewError::Unavailable);
        }
        self.shared
            .grants
            .lock()
            .map_err(|_| HtmlAnswerViewError::Unavailable)?
            .admit_issue(request.client_id, Instant::now())?;
        (self.shared.resolver)(request.source.clone()).await?;
        self.ensure_listener().await?;
        let grant = Grant {
            id: uuid::Uuid::new_v4(),
            client: request.client_id,
            source: request.source,
            locale: request.locale,
            expires: Instant::now() + Duration::from_secs(HTML_ANSWER_TICKET_SECONDS),
        };
        let id = grant.id;
        let path = format!(
            "{}/html-answers/{}/{}",
            self.config.viewer_origin, grant.source.session_id, grant.source.artifact_id
        );
        let secret = self
            .shared
            .grants
            .lock()
            .map_err(|_| HtmlAnswerViewError::Unavailable)?
            .issue(grant, Instant::now())?;
        Ok(HtmlAnswerViewLinkResponse {
            grant_id: id,
            viewer_url: format!("{path}#ticket={secret}"),
            expires_in_seconds: HTML_ANSWER_TICKET_SECONDS,
        })
    }

    /// Explicitly revoke `client`'s `grant`, including a pending exchange; other clients are unaffected.
    /// 显式撤销 `client` 的 `grant`，包含待完成兑换；不影响其他客户端。
    pub fn revoke(&self, client: uuid::Uuid, grant: uuid::Uuid) -> Result<(), HtmlAnswerViewError> {
        self.shared
            .grants
            .lock()
            .map_err(|_| HtmlAnswerViewError::Unavailable)?
            .revoke(client, grant);
        Ok(())
    }

    /// Stop and join the render task, permanently closing in-flight admission and discarding secrets.
    /// 停止并等待渲染任务，永久关闭进行中的准入并丢弃秘密。
    pub async fn shutdown(&self) {
        let task = self.take_stopped_listener();
        if let Some(task) = task {
            let _ = task.await;
        }
    }

    /// Immediately revoke authority and abort rendering, including when the parent HTTP future is cancelled.
    /// 立即撤销权威并中止渲染，包含父 HTTP future 被取消的情况。
    pub fn stop(&self) {
        self.take_stopped_listener();
    }

    /// Close admission before taking the listener; a concurrent bind must recheck this closure before publication.
    /// 取得监听器前关闭准入；并发绑定必须在发布前重新检查该关闭状态。
    fn take_stopped_listener(&self) -> Option<JoinHandle<()>> {
        if let Ok(mut grants) = self.shared.grants.lock() {
            grants.close();
        }
        let task = self.listener.lock().ok()?.take();
        if let Some(task) = &task {
            task.abort();
        }
        task
    }

    /// Bind the configured loopback port once after source validation; return a stable failure on shutdown.
    /// 源码校验后仅绑定一次配置的回环端口；关闭后返回稳定失败。
    async fn ensure_listener(&self) -> Result<(), HtmlAnswerViewError> {
        let _startup = self.startup.lock().await;
        if self
            .shared
            .grants
            .lock()
            .map_err(|_| HtmlAnswerViewError::Unavailable)?
            .closed
        {
            return Err(HtmlAnswerViewError::Closed);
        }
        if let Some(task) = self
            .listener
            .lock()
            .map_err(|_| HtmlAnswerViewError::Unavailable)?
            .as_ref()
        {
            return if task.is_finished() {
                Err(HtmlAnswerViewError::Unavailable)
            } else {
                Ok(())
            };
        }
        let listener = TcpListener::bind(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            self.config.renderer_port,
        ))
        .await
        .map_err(|_| HtmlAnswerViewError::Unavailable)?;
        let local = listener
            .local_addr()
            .map_err(|_| HtmlAnswerViewError::Unavailable)?;
        let renderer = self
            .config
            .renderer_origin
            .clone()
            .unwrap_or_else(|| format!("http://{local}"));
        let origins = HtmlAnswerOrigins::new(&self.config.viewer_origin, &renderer)
            .map_err(|_| HtmlAnswerViewError::InvalidRequest)?;
        let mut owner = self
            .listener
            .lock()
            .map_err(|_| HtmlAnswerViewError::Unavailable)?;
        if self
            .shared
            .grants
            .lock()
            .map_err(|_| HtmlAnswerViewError::Unavailable)?
            .closed
        {
            return Err(HtmlAnswerViewError::Closed);
        }
        self.shared
            .origins
            .set(origins)
            .map_err(|_| HtmlAnswerViewError::Unavailable)?;
        let router = routes::render_router(self.shared.clone());
        let shared = self.shared.clone();
        *owner = Some(tokio::spawn(async move {
            // Idle capabilities are reclaimed on the shortest policy lifetime, with no second task owner.
            // 按最短策略有效期回收空闲能力，不创建第二个任务所有者。
            let mut cleanup = tokio::time::interval(Duration::from_secs(
                HTML_ANSWER_TICKET_SECONDS.min(HTML_ANSWER_LEASE_SECONDS),
            ));
            cleanup.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let server = axum::serve(listener, router).into_future();
            tokio::pin!(server);
            loop {
                tokio::select! {
                    _ = &mut server => break,
                    _ = cleanup.tick() => {
                        if let Ok(mut grants) = shared.grants.lock() { grants.prune(Instant::now()); }
                    }
                }
            }
        }));
        Ok(())
    }
}

impl Drop for HtmlAnswerRuntime {
    /// Abort the unique listener if its owner is dropped before explicit shutdown.
    /// 如果所有者在显式关闭之前被释放，则中止唯一监听器。
    fn drop(&mut self) {
        self.stop();
    }
}

/// Roll back a pending lease if async source validation fails, panics or is cancelled.
/// 异步源码校验失败、恐慌或取消时回滚待确认租约。
pub(super) struct PendingLease {
    /// Shared bounded table from which this guard owns one pending entry.
    /// 当前守卫拥有其中一条待确认记录的共享有界表。
    pub shared: Arc<RenderState>,
    /// Reserved bearer secret, intentionally absent from Debug.
    /// 预留的持有者秘密，刻意不包含在 Debug 中。
    pub secret: String,
    /// Disarmed only after the exact reservation was promoted to a readable lease.
    /// 仅当精确预留记录晋升为可读租约后解除。
    pub committed: bool,
}

impl Drop for PendingLease {
    /// Remove only this uncommitted reservation; cancellation cannot leak bounded capacity.
    /// 仅移除当前未提交的预留记录；取消不能泄漏有界容量。
    fn drop(&mut self) {
        if !self.committed
            && let Ok(mut grants) = self.shared.grants.lock()
        {
            grants.leases.remove(&self.secret);
        }
    }
}
