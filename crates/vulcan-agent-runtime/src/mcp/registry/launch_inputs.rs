//! Protected inputs belong to one actual launched transport, never to a mutable server lookup.
//! 受保护输入属于单个实际已启动传输，绝不属于可变服务器查找。

use std::sync::{Arc, Weak};
use vulcan_mcp_runtime::{
    McpOAuthSession, McpResourceRelease, McpTransport, McpTransportLaunchSpec,
    McpTransportResources,
};
use vulcan_plugin_runtime::{
    VulcanPluginPredecessorMcpProfile, VulcanPluginPublishedMcpProfile,
    VulcanPluginTargetMcpProfile,
};

/// Source is fixed by the actual producer before work starts, never registered after a connection exists.
/// 来源由实际生产者在工作开始前固定，绝不在连接存在后登记。
#[derive(Clone)]
pub(crate) enum McpPluginLaunchSource {
    /// Ordinary legacy inputs carry no proof of a full published plugin selection.
    /// 普通旧输入不携带完整已发布插件选择的证明。
    Unproven,
    /// Pure original source values share identity with the actual prepared ordinary owner.
    /// 纯原来源值与实际已准备普通所有者共享身份。
    Published(VulcanPluginPublishedMcpProfile),
    /// Selected target source remains distinct from ordinary publication and is bound before native work.
    /// 已选目标来源保持独立于普通发布，在原生工作之前绑定。
    Target(VulcanPluginTargetMcpProfile),
    /// Old restoration source is bound before dispatch and grants no target or published authority.
    /// 旧恢复来源在派发前绑定，不授予目标或已发布权威。
    Predecessor(VulcanPluginPredecessorMcpProfile),
}

/// Explicit capture rejection contains no endpoint, credential, argument or environment value.
/// 显式捕获拒绝不包含端点、凭据、参数或环境值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum McpLaunchInputCaptureError {
    /// The current server has no connected recorded native transport.
    /// 当前服务器没有已连接且有记录的原生传输。
    Unavailable,
    /// The actual transport differs from the one that consumed these inputs.
    /// 实际传输与消费这些输入的传输不同。
    DifferentTransport,
    /// Original native resources have already confirmed retirement.
    /// 原原生资源已经确认退役。
    Retired,
    /// Native spawn inherits ambient values not represented by the passed specification.
    /// 原生启动继承未在传入规格中表示的环境值。
    InheritedEnvironment,
    /// Native spawn resolves the ambient process directory without a frozen explicit directory.
    /// 原生启动解析进程当前目录，没有冻结的显式目录。
    ImplicitWorkingDirectory,
    /// Native launch creates an OAuth session that this host owner cannot capture.
    /// 原生启动创建此宿主所有者无法捕获的 OAuth 会话。
    UnretainedOAuthSession,
    /// The actual producer did not bind a sealed original plugin source.
    /// 实际生产者未绑定密封原插件来源。
    UnprovenPluginSource,
    /// The requested profile is not the same source consumed by this original actual launch.
    /// 请求画像不是此原实际启动消费的同一来源。
    DifferentPluginSource,
}

/// Exact protected launch projection, retained by the same actual native resource bundle.
/// 精确受保护启动投影，由相同实际原生资源包保留。
/// This type deliberately has no Debug, serialization, public constructor or public value getter.
/// 此类型刻意不提供 Debug、序列化、公开构造器或公开值读取器。
struct McpActualLaunchInputs {
    /// The exact specification consumed once by the actual launch factory.
    /// 实际启动工厂仅消费一次的精确规格。
    specification: McpTransportLaunchSpec,
    /// The identical shared OAuth authority passed to this actual launch, not a token snapshot.
    /// 传入此实际启动的同一共享 OAuth 权威，不是令牌快照。
    auth_session: Option<Arc<McpOAuthSession>>,
    /// The producer's original opaque source belongs to the same Arc as the actual specification.
    /// 生产者原不透明来源与实际规格属于同一 Arc。
    source: McpPluginLaunchSource,
}

/// Successful initialization returns both the actual transport and its sealed input ownership.
/// 成功初始化同时返回实际传输及其密封输入所有权。
pub(crate) struct McpLaunchedTransport {
    /// The original successful transport is consumed by the lifecycle caller.
    /// 原成功传输由生命周期调用者消费。
    transport: Arc<dyn McpTransport>,
    /// Capture evidence observes this same transport and its original resource release.
    /// 捕获证据观察此相同传输及其原资源释放。
    inputs: McpActualLaunchInputLease,
}

/// Protected input lease identifies the original transport through its actual Arc allocation.
/// 受保护输入租约通过实际 Arc 分配标识原传输。
/// A retained input lease does not keep a protocol transport or native process alive.
/// 保留输入租约不会延长协议传输或原生进程寿命。
pub(crate) struct McpActualLaunchInputLease {
    /// Weak ownership prevents capture evidence from obstructing physical retirement.
    /// 弱所有权防止捕获证据阻碍物理退役。
    transport: Weak<dyn McpTransport>,
    /// The same projection also belongs to the actual process, pipes, clients and streams.
    /// 相同投影也属于实际进程、管道、客户端及流。
    inputs: Arc<McpActualLaunchInputs>,
    /// This original independent release receipt follows the exact launch resource bundle.
    /// 此原独立释放回执跟随精确启动资源包。
    release: McpResourceRelease,
}

impl McpLaunchedTransport {
    /// Launches once after binding exact protected inputs to the real native resource bundle.
    /// 将精确受保护输入绑定到真实原生资源包后，仅启动一次。
    /// Specification is consumed; auth_session and resources retain their original authority.
    /// 消费 specification；auth_session 及 resources 保留其原权威。
    /// Returns matching transport/input ownership, or a diagnostic redacted using these exact inputs.
    /// 返回匹配的传输及输入所有权，或使用这些精确输入脱敏的诊断。
    pub(super) async fn launch(
        specification: McpTransportLaunchSpec,
        auth_session: Option<Arc<McpOAuthSession>>,
        resources: McpTransportResources,
        source: McpPluginLaunchSource,
    ) -> Result<Self, String> {
        // The projection precedes native work and is never reconstructed from current configuration.
        // 投影先于原生工作建立，绝不从当前配置重建。
        let inputs = Arc::new(McpActualLaunchInputs {
            specification,
            auth_session,
            source,
        });
        // Native owners keep the same Arc even when initialization fails or its observer is cancelled.
        // 即使初始化失败或观察者取消，原生所有者仍保留相同 Arc。
        let (resources, release) = McpTransportResources::new((resources, inputs.clone()));
        let transport = inputs
            .specification
            .clone()
            .launch_with_resources(inputs.auth_session.clone(), resources)
            .await
            .map_err(|error| inputs.redact_diagnostic(error.to_string()))?;
        Ok(Self {
            inputs: McpActualLaunchInputLease {
                transport: Arc::downgrade(&transport),
                inputs,
                release,
            },
            transport,
        })
    }

    /// Transfers the original transport and its exact input lease together to one lifecycle owner.
    /// 将原传输及其精确输入租约同时移交给单个生命周期所有者。
    /// No inputs; returns the same successfully initialized transport and sealed evidence.
    /// 无输入；返回相同成功初始化传输及密封证据。
    pub(crate) fn into_parts(self) -> (Arc<dyn McpTransport>, McpActualLaunchInputLease) {
        (self.transport, self.inputs)
    }
}

impl McpActualLaunchInputLease {
    /// Borrow actual inputs only when current_transport and expected identify the same actual published launch.
    /// 仅当 current_transport 及 expected 标识同一实际已发布启动时借用实际输入。
    /// read is a trusted synchronous host closure; return its value or a value-free source rejection.
    /// read 为可信同步宿主闭包；返回其值或不含值的来源拒绝。
    /// Original transport, native release and exact input eligibility reuse the sole validation below.
    /// 原传输、原生释放及精确输入资格复用下方唯一校验。
    pub(crate) fn with_published_source<R>(
        &self,
        current_transport: &Arc<dyn McpTransport>,
        expected: &VulcanPluginPublishedMcpProfile,
        read: impl FnOnce(&McpTransportLaunchSpec, Option<&Arc<McpOAuthSession>>) -> R,
    ) -> Result<R, McpLaunchInputCaptureError> {
        self.with_current_transport(current_transport, |specification, auth_session| match &self
            .inputs
            .source
        {
            McpPluginLaunchSource::Unproven => {
                Err(McpLaunchInputCaptureError::UnprovenPluginSource)
            }
            McpPluginLaunchSource::Published(original) if original.same_source(expected) => {
                Ok(read(specification, auth_session))
            }
            McpPluginLaunchSource::Published(_)
            | McpPluginLaunchSource::Target(_)
            | McpPluginLaunchSource::Predecessor(_) => {
                Err(McpLaunchInputCaptureError::DifferentPluginSource)
            }
        })?
    }

    /// Borrow same-transport inputs only when expected is the original target source consumed before launch.
    /// 仅当 expected 为启动前消费的原目标来源时，借用同一传输输入。
    /// current_transport is the locked actual entry; read receives exact inputs without exporting ownership.
    /// current_transport 为加锁实际条目；read 接收精确输入，不导出所有权。
    /// Return read's result or a value-free rejection using the sole native input eligibility predicate.
    /// 使用唯一原生输入资格谓词，返回 read 结果或不含值的拒绝。
    pub(crate) fn with_target_source<R>(
        &self,
        current_transport: &Arc<dyn McpTransport>,
        expected: &VulcanPluginTargetMcpProfile,
        read: impl FnOnce(&McpTransportLaunchSpec, Option<&Arc<McpOAuthSession>>) -> R,
    ) -> Result<R, McpLaunchInputCaptureError> {
        self.with_current_transport(current_transport, |specification, auth_session| match &self
            .inputs
            .source
        {
            McpPluginLaunchSource::Unproven => {
                Err(McpLaunchInputCaptureError::UnprovenPluginSource)
            }
            McpPluginLaunchSource::Target(original) if original.same_source(expected) => {
                Ok(read(specification, auth_session))
            }
            McpPluginLaunchSource::Target(_)
            | McpPluginLaunchSource::Published(_)
            | McpPluginLaunchSource::Predecessor(_) => {
                Err(McpLaunchInputCaptureError::DifferentPluginSource)
            }
        })?
    }

    /// Borrow original old inputs only when expected identifies the same pre-dispatch profile allocation.
    /// 仅当 expected 标识同一派发前画像分配时借用原旧输入。
    /// current_transport proves the actual entry; read receives protected inputs and returns its result.
    /// current_transport 证明实际条目；read 接收受保护输入并返回结果。
    /// Eligibility and release use the sole native predicate; foreign sources return value-free rejection.
    /// 资格及释放使用唯一原生谓词；外来来源返回不含值的拒绝。
    pub(crate) fn with_predecessor_source<R>(
        &self,
        current_transport: &Arc<dyn McpTransport>,
        expected: &VulcanPluginPredecessorMcpProfile,
        read: impl FnOnce(&McpTransportLaunchSpec, Option<&Arc<McpOAuthSession>>) -> R,
    ) -> Result<R, McpLaunchInputCaptureError> {
        self.with_current_transport(current_transport, |specification, auth_session| match &self
            .inputs
            .source
        {
            McpPluginLaunchSource::Unproven => {
                Err(McpLaunchInputCaptureError::UnprovenPluginSource)
            }
            McpPluginLaunchSource::Predecessor(original) if original.same_source(expected) => {
                Ok(read(specification, auth_session))
            }
            McpPluginLaunchSource::Predecessor(_)
            | McpPluginLaunchSource::Target(_)
            | McpPluginLaunchSource::Published(_) => {
                Err(McpLaunchInputCaptureError::DifferentPluginSource)
            }
        })?
    }

    /// Borrows protected inputs only while the host proves the identical current native transport.
    /// 仅在宿主证明同一当前原生传输时，借用受保护输入。
    /// Current_transport comes from the locked live map; read is a trusted synchronous host closure.
    /// current_transport 来自加锁实时映射；read 为可信同步宿主闭包。
    /// Returns the closure result, or a value-free explicit unavailable reason; never reads ambient inputs.
    /// 返回闭包结果，或不含值的显式不可用原因；绝不读取环境输入。
    pub(crate) fn with_current_transport<R>(
        &self,
        current_transport: &Arc<dyn McpTransport>,
        read: impl FnOnce(&McpTransportLaunchSpec, Option<&Arc<McpOAuthSession>>) -> R,
    ) -> Result<R, McpLaunchInputCaptureError> {
        let original = self
            .transport
            .upgrade()
            .ok_or(McpLaunchInputCaptureError::Retired)?;
        if !Arc::ptr_eq(&original, current_transport) {
            return Err(McpLaunchInputCaptureError::DifferentTransport);
        }
        if self.release.is_released() {
            return Err(McpLaunchInputCaptureError::Retired);
        }
        match &self.inputs.specification {
            McpTransportLaunchSpec::Stdio {
                inherit_environment: true,
                ..
            } => return Err(McpLaunchInputCaptureError::InheritedEnvironment),
            McpTransportLaunchSpec::Stdio { cwd: None, .. } => {
                return Err(McpLaunchInputCaptureError::ImplicitWorkingDirectory);
            }
            McpTransportLaunchSpec::Http { oauth: Some(_), .. }
            | McpTransportLaunchSpec::Sse { oauth: Some(_), .. }
                if self.inputs.auth_session.is_none() =>
            {
                return Err(McpLaunchInputCaptureError::UnretainedOAuthSession);
            }
            _ => {}
        }
        Ok(read(
            &self.inputs.specification,
            self.inputs.auth_session.as_ref(),
        ))
    }
}

impl McpActualLaunchInputs {
    /// Redacts failures with the exact passed values, even when ambient environment changed afterward.
    /// 使用精确传入值脱敏失败，即使环境之后已经变化。
    /// Diagnostic is an owned native failure; returns text with all nonempty protected values removed.
    /// diagnostic 为自有原生失败；返回移除全部非空受保护值的文本。
    fn redact_diagnostic(&self, mut diagnostic: String) -> String {
        let mut values: Vec<&str> = match &self.specification {
            McpTransportLaunchSpec::Stdio { args, env, .. } => args
                .iter()
                .chain(env.values())
                .map(String::as_str)
                .collect(),
            McpTransportLaunchSpec::Http {
                uri,
                bearer_token,
                headers,
                oauth,
                ..
            }
            | McpTransportLaunchSpec::Sse {
                uri,
                bearer_token,
                headers,
                oauth,
                ..
            } => {
                let mut values = vec![uri.as_str()];
                values.extend(
                    bearer_token
                        .iter()
                        .chain(headers.values())
                        .map(String::as_str),
                );
                if let Some(oauth) = oauth {
                    values.extend(
                        oauth
                            .client_id
                            .iter()
                            .chain(oauth.client_secret.iter())
                            .chain(oauth.redirect_uri.iter())
                            .map(String::as_str),
                    );
                }
                values
            }
        };
        values.retain(|value| !value.is_empty());
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        for value in values {
            diagnostic = diagnostic.replace(value, "[REDACTED]");
        }
        diagnostic
    }
}

#[cfg(test)]
mod tests;
