//! Frozen-account credential leases and host-owned secret storage boundaries.
//! 冻结账号凭据租约与宿主持有的秘密存储边界。

use super::{PublicAiRuntime, current_unix_millis};
use crate::{CredentialExecutionSecret, CredentialSecretEnvelope, OAuthAuthorizationRuntime};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Mutex as AsyncMutex;

/// Refreshes renewable tokens this long before their declared expiration.
/// 在可续期 Token 声明过期前这么长时间执行刷新。
const OAUTH_REFRESH_SKEW_MS: u64 = 5 * 60 * 1_000;

/// Resolves the latest secret revision for one frozen account without changing its route.
/// 在不改变冻结账号路由的前提下，为该账号解析最新秘密修订版。
#[derive(Clone)]
pub struct ProviderExecutionCredentialLease {
    /// Exact connection whose refresh operations share one single-flight lock.
    /// 刷新操作共享同一个单飞锁的精确连接。
    connection_id: String,
    /// Exact secure-store reference frozen with the selected account.
    /// 随所选账号冻结的精确安全仓库引用。
    secret_reference: String,
    /// Shared provider OAuth runtime used only for renewable credentials.
    /// 仅为可续期凭据使用的共享供应商 OAuth 运行时。
    oauth: Arc<OAuthAuthorizationRuntime>,
    /// Connection-scoped locks shared with every runtime credential operation.
    /// 与全部运行时凭据操作共享的连接作用域锁。
    refresh_locks: Arc<StdMutex<BTreeMap<String, Arc<AsyncMutex<()>>>>>,
    /// Host-owned secure secret store.
    /// 宿主持有的安全秘密仓库。
    secrets: Arc<dyn CredentialSecretStore>,
    /// One-shot 401 recovery guard reset only after a physical Provider request succeeds.
    /// 仅在一次真实 Provider 请求成功后重置的单次 401 恢复护栏。
    unauthorized_refresh_used: Arc<AtomicBool>,
}

impl ProviderExecutionCredentialLease {
    /// Creates one credential lease for an already selected account.
    /// 为已经选定的账号创建一份凭据租约。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `connection_id` and `secret_reference` freeze account identity; the remaining values
    /// share the host-owned refresh and storage boundaries.
    /// `connection_id` 与 `secret_reference` 冻结账号身份；其余值共享宿主持有的刷新与存储边界。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A reusable lease that re-reads only the exact frozen secret reference.
    /// 返回仅重读精确冻结秘密引用的可复用租约。
    pub(super) fn new(
        connection_id: String,
        secret_reference: String,
        oauth: Arc<OAuthAuthorizationRuntime>,
        refresh_locks: Arc<StdMutex<BTreeMap<String, Arc<AsyncMutex<()>>>>>,
        secrets: Arc<dyn CredentialSecretStore>,
    ) -> Self {
        Self {
            connection_id,
            secret_reference,
            oauth,
            refresh_locks,
            secrets,
            unauthorized_refresh_used: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Resolves the latest valid secret immediately before one physical Provider request.
    /// 在一次真实 Provider 请求之前解析最新有效秘密。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The newest stored revision, refreshing an expiring OAuth credential under one lock.
    /// 返回最新存储修订版，并在单飞锁内刷新即将过期的 OAuth 凭据。
    pub async fn resolve_for_request(&self) -> Result<CredentialExecutionSecret, String> {
        // current is re-read for every physical request so concurrent account refreshes become visible immediately.
        // current 会在每次真实请求前重新读取，使并发账号刷新立即可见。
        let current = self
            .secrets
            .read_secret(self.secret_reference.as_str())
            .await?;
        if !current.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS) {
            return current.execution_secret();
        }
        self.refresh_expiring_secret().await
    }

    /// Forces one renewable credential recovery after an exact upstream 401 response.
    /// 在收到精确上游 401 响应后强制恢复一次可续期凭据。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `rejected_revision` is the exact secret-store revision rejected by the upstream request.
    /// `rejected_revision` 是上游请求拒绝的精确秘密仓库修订号。
    ///
    /// # Returns
    /// 返回值
    ///
    /// `true` after a newer revision is observed or one forced OAuth refresh succeeds; static
    /// credentials and repeated recovery within the same physical request return `false`.
    /// 观察到更新修订版或一次强制 OAuth 刷新成功后返回 `true`；静态凭据以及同一真实请求内
    /// 的重复恢复返回 `false`。
    pub async fn refresh_after_unauthorized(&self, rejected_revision: u64) -> Result<bool, String> {
        // current determines whether the exact frozen credential is renewable before claiming recovery.
        // current 在占用恢复机会前判断精确冻结凭据是否可续期。
        let current = self
            .secrets
            .read_secret(self.secret_reference.as_str())
            .await?;
        if !current.is_renewable_oauth() {
            return Ok(false);
        }
        if self.unauthorized_refresh_used.swap(true, Ordering::AcqRel) {
            return Ok(false);
        }
        // refreshLock serializes forced 401 recovery with expiry-driven refreshes for this account.
        // refreshLock 将强制 401 恢复与该账号的到期刷新串行化。
        let refresh_lock = self.refresh_lock()?;
        let _guard = refresh_lock.lock().await;
        // current is re-read after the lock because another request may already have replaced the rejected revision.
        // current 在获得锁后重新读取，因为另一个请求可能已经替换了被拒绝的修订版。
        let current = self
            .secrets
            .read_secret(self.secret_reference.as_str())
            .await?;
        if current.revision > rejected_revision
            && !current.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS)
        {
            current.execution_secret()?;
            return Ok(true);
        }
        if !current.is_renewable_oauth() {
            return Ok(false);
        }
        self.refresh_locked_secret(&current).await?;
        Ok(true)
    }

    /// Resets the one-shot unauthorized recovery guard after one physical request succeeds.
    /// 在一次真实请求成功后重置单次未授权恢复护栏。
    pub fn mark_request_succeeded(&self) {
        self.unauthorized_refresh_used
            .store(false, Ordering::Release);
    }

    /// Refreshes one expiring credential after re-checking it under the account lock.
    /// 在账号锁内重新检查并刷新一份即将过期的凭据。
    async fn refresh_expiring_secret(&self) -> Result<CredentialExecutionSecret, String> {
        // refreshLock shares expiry refresh ordering with forced 401 recovery.
        // refreshLock 让到期刷新与强制 401 恢复共享顺序。
        let refresh_lock = self.refresh_lock()?;
        let _guard = refresh_lock.lock().await;
        // current is authoritative only after the single-flight lock is held.
        // current 仅在持有单飞锁后才是权威值。
        let current = self
            .secrets
            .read_secret(self.secret_reference.as_str())
            .await?;
        if !current.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS) {
            return current.execution_secret();
        }
        self.refresh_locked_secret(&current).await
    }

    /// Returns the connection-scoped single-flight lock for this frozen account.
    /// 返回当前冻结账号的连接作用域单飞锁。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The shared asynchronous lock, or a controlled poisoned-lock error.
    /// 返回共享异步锁，或受控的锁中毒错误。
    fn refresh_lock(&self) -> Result<Arc<AsyncMutex<()>>, String> {
        // locks is the process-local map that owns every connection refresh lock.
        // locks 是持有每个连接刷新锁的进程本地映射。
        let mut locks = self
            .refresh_locks
            .lock()
            .map_err(|_| String::from("The AI credential refresh lock is poisoned."))?;
        Ok(Arc::clone(
            locks
                .entry(self.connection_id.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        ))
    }

    /// Refreshes and compare-and-replaces one credential while its connection lock is held.
    /// 在持有连接锁时刷新并比较替换一份凭据。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `current` is the exact authoritative revision read under the refresh lock.
    /// `current` 是在刷新锁内读取的精确权威修订版。
    ///
    /// # Returns
    /// 返回值
    ///
    /// The refreshed execution secret or a newer valid concurrent winner.
    /// 返回刷新后的执行秘密，或更新且有效的并发获胜修订版。
    async fn refresh_locked_secret(
        &self,
        current: &CredentialSecretEnvelope,
    ) -> Result<CredentialExecutionSecret, String> {
        // native_checkpoint is durable before consuming the single-use Auth rotation. A crash or
        // unknown network outcome therefore cannot replay the old credential on restart.
        // native_checkpoint 在消费单次 Auth 轮换前持久化；崩溃或未知网络结果因此不能在重启后重放旧凭据。
        let native_checkpoint = if let crate::CredentialSecretMaterial::OpenVulcanAuth {
            rotation_pending,
            ..
        } = &current.material
        {
            if *rotation_pending {
                return Err(String::from(
                    "OpenVulcan rotation was interrupted; sign in again.",
                ));
            }
            let mut fenced_material = current.material.clone();
            if let crate::CredentialSecretMaterial::OpenVulcanAuth {
                rotation_pending, ..
            } = &mut fenced_material
            {
                *rotation_pending = true;
            }
            let fenced = current.advance_with_material(fenced_material)?;
            self.secrets
                .replace_secret(&self.secret_reference, current.revision, &fenced)
                .await?;
            Some(fenced)
        } else {
            None
        };
        // material is the provider-specific refreshed credential returned by the OAuth runtime.
        // material 是 OAuth 运行时返回的供应商专属刷新凭据。
        let material = self.oauth.refresh(&current.material).await?;
        let current = native_checkpoint.as_ref().unwrap_or(current);
        // refreshed advances the secure-store revision exactly once for this material.
        // refreshed 为该凭据材料精确推进一次安全仓库修订号。
        let refreshed = current.advance_with_material(material)?;
        if let Err(replace_error) = self
            .secrets
            .replace_secret(self.secret_reference.as_str(), current.revision, &refreshed)
            .await
        {
            // concurrent is accepted only when another writer published a newer non-expiring revision.
            // concurrent 仅在另一个写入者发布更新且未临近过期的修订版时被接受。
            let concurrent = self
                .secrets
                .read_secret(self.secret_reference.as_str())
                .await?;
            if concurrent.revision > current.revision
                && !concurrent.requires_refresh(current_unix_millis()?, OAUTH_REFRESH_SKEW_MS)
            {
                return concurrent.execution_secret();
            }
            return Err(replace_error);
        }
        refreshed.execution_secret()
    }
}

/// Reads secret material through a host-owned secure storage boundary.
/// 通过宿主所有的安全存储边界读取秘密材料。
#[async_trait]
pub trait CredentialSecretResolver: Send + Sync {
    /// Reads one secret when present without converting absence into a storage failure.
    /// 在秘密存在时读取它，且不把缺失转换为存储失败。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` is the opaque reference saved with the credential connection.
    /// `secret_reference` 是随凭据连接保存的不透明引用。
    ///
    /// # Returns
    /// 返回值
    /// The validated secret when present, `None` when absent, or a controlled storage error.
    /// 存在时返回已验证秘密，缺失时返回 `None`，否则返回受控存储错误。
    async fn read_optional_secret(
        &self,
        secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String>;

    /// Reads the strict versioned secret identified by one exact persisted reference.
    /// 读取由一个精确持久化引用标识的严格版本化秘密。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` is the opaque reference saved with the credential connection.
    /// `secret_reference` 是随凭据连接保存的不透明引用。
    ///
    /// # Returns
    /// 返回值
    /// The validated secret envelope or a controlled storage error that never includes the secret.
    /// 返回已验证秘密封装，或不包含秘密内容的受控存储错误。
    async fn read_secret(
        &self,
        secret_reference: &str,
    ) -> Result<CredentialSecretEnvelope, String> {
        self.read_optional_secret(secret_reference)
            .await?
            .ok_or_else(|| format!("AI credential `{secret_reference}` is not configured."))
    }
}

/// Mutates versioned secrets through a host-owned compare-and-replace boundary.
/// 通过宿主所有的比较替换边界修改版本化秘密。
#[async_trait]
pub trait CredentialSecretStore: CredentialSecretResolver {
    /// Captures `secret_reference` for repair without requiring executable credential material.
    /// 捕获 `secret_reference` 供修复使用，不要求凭据材料可执行。
    /// Returns an optional opaque before-image; host stores override this to retain raw records.
    /// 返回可选不透明前镜像；宿主仓库覆盖此方法以保留原始记录。
    async fn capture_secret(
        &self,
        secret_reference: &str,
    ) -> Result<Option<super::CredentialSecretSnapshot>, String> {
        self.read_optional_secret(secret_reference)
            .await?
            .map(|secret| {
                super::CredentialSecretSnapshot::new(secret_reference, secret.revision, secret)
            })
            .transpose()
    }

    /// Restores `snapshot` at the absent `secret_reference` after an explicit removal fails.
    /// 显式移除失败后，在缺失的 `secret_reference` 恢复 `snapshot`。
    /// Returns the storage result; unsupported records remain unsupported for normal execution.
    /// 返回存储结果；不受支持的记录对正常执行仍然不受支持。
    async fn restore_snapshot(
        &self,
        secret_reference: &str,
        snapshot: &super::CredentialSecretSnapshot,
    ) -> Result<(), String> {
        self.restore_secret(secret_reference, snapshot.record(secret_reference)?)
            .await
    }

    /// Restores `snapshot` only if `expected_revision` still owns `secret_reference`.
    /// 仅当 `expected_revision` 仍持有 `secret_reference` 时恢复 `snapshot`。
    /// Returns a compare-and-replace result with a strictly increasing revision to prevent ABA.
    /// 返回修订号严格递增的比较替换结果，防止 ABA 问题。
    async fn rollback_replacement(
        &self,
        secret_reference: &str,
        expected_revision: u64,
        snapshot: &super::CredentialSecretSnapshot,
    ) -> Result<(), String> {
        // Before-image material is restored only by the owning backend, never projected to a request.
        // 前镜像材料仅由所属后端恢复，绝不投影到请求。
        let mut previous = snapshot
            .record::<CredentialSecretEnvelope>(secret_reference)?
            .clone();
        previous.revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| String::from("AI credential secret revision is exhausted."))?;
        self.replace_secret(secret_reference, expected_revision, &previous)
            .await
    }

    /// Creates one secret only when the opaque reference is absent.
    /// 仅在不透明引用不存在时创建一个秘密。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` is exact and `secret` must contain revision one.
    /// `secret_reference` 为精确引用，`secret` 必须包含第一修订号。
    ///
    /// # Returns
    /// 返回值
    /// Success after an exclusive secure-store create.
    /// 独占安全存储创建完成后返回成功。
    async fn create_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String>;

    /// Replaces one secret only when the stored revision matches.
    /// 仅在已存修订号匹配时替换一个秘密。
    ///
    /// # Parameters
    /// 参数
    /// `expected_revision` protects against stale refresh writes and `secret` is the next value.
    /// `expected_revision` 防止过期刷新写入，`secret` 是下一值。
    ///
    /// # Returns
    /// 返回值
    /// Success after a compare-and-replace mutation.
    /// 比较替换变更完成后返回成功。
    async fn replace_secret(
        &self,
        secret_reference: &str,
        expected_revision: u64,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String>;

    /// Deletes one exact secret.
    /// 删除一个精确秘密。
    ///
    /// # Returns
    /// 返回值
    /// Success when the secret is absent or deleted.
    /// 秘密不存在或已删除时返回成功。
    async fn delete_secret(&self, secret_reference: &str) -> Result<(), String>;

    /// Restores one exact deleted secret for transaction rollback.
    /// 为事务回滚恢复一个精确的已删除秘密。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` must be absent and `secret` is the exact prior revision.
    /// `secret_reference` 必须缺失，`secret` 是精确的先前修订版。
    ///
    /// # Returns
    /// 返回值
    /// Success only after restoring the complete prior secret.
    /// 仅在恢复完整先前秘密后返回成功。
    async fn restore_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String>;
}

/// Overlays one candidate credential in memory while delegating every unrelated reference.
/// 在内存中覆盖一份候选凭据，同时把全部无关引用委托给宿主仓库。
pub(super) struct IsolatedCredentialSecretStore {
    /// Host store used only for references outside the isolated candidate.
    /// 仅用于隔离候选之外引用的宿主仓库。
    delegate: Arc<dyn CredentialSecretStore>,
    /// Exact opaque reference owned by the isolated candidate.
    /// 隔离候选持有的精确不透明引用。
    secret_reference: String,
    /// Mutable in-memory envelope supporting OAuth compare-and-replace without durable writes.
    /// 支持 OAuth 比较替换且不产生持久写入的可变内存封装。
    secret: StdMutex<Option<CredentialSecretEnvelope>>,
}

impl IsolatedCredentialSecretStore {
    /// Creates one exact in-memory credential overlay.
    /// 创建一个精确的内存凭据覆盖层。
    ///
    /// # Parameters
    /// 参数
    /// `delegate` serves unrelated references, while `secret_reference` and `secret` form the
    /// isolated candidate identity.
    /// `delegate` 服务无关引用，`secret_reference` 与 `secret` 共同构成隔离候选身份。
    ///
    /// # Returns
    /// 返回值
    /// A store whose candidate mutations cannot reach the host credential database.
    /// 返回一个候选变更无法触达宿主凭据数据库的仓库。
    pub(super) fn new(
        delegate: Arc<dyn CredentialSecretStore>,
        secret_reference: String,
        secret: CredentialSecretEnvelope,
    ) -> Self {
        Self {
            delegate,
            secret_reference,
            secret: StdMutex::new(Some(secret)),
        }
    }

    /// Reports whether one reference belongs to the isolated candidate.
    /// 报告一个引用是否属于隔离候选。
    ///
    /// # Parameters
    /// 参数
    /// `secret_reference` is the exact opaque lookup key.
    /// `secret_reference` 是精确的不透明查找键。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the candidate-owned reference.
    /// 仅对候选持有的引用返回 `true`。
    fn owns(&self, secret_reference: &str) -> bool {
        self.secret_reference == secret_reference
    }
}

#[async_trait]
impl CredentialSecretResolver for IsolatedCredentialSecretStore {
    /// Reads the candidate from memory or delegates an unrelated reference.
    /// 从内存读取候选，或委托一个无关引用。
    async fn read_optional_secret(
        &self,
        secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        if !self.owns(secret_reference) {
            return self.delegate.read_optional_secret(secret_reference).await;
        }
        self.secret
            .lock()
            .map(|secret| secret.clone())
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))
    }
}

#[async_trait]
impl CredentialSecretStore for IsolatedCredentialSecretStore {
    /// Creates the isolated candidate only when its in-memory slot is absent.
    /// 仅在内存槽位缺失时创建隔离候选。
    async fn create_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        if !self.owns(secret_reference) {
            return self.delegate.create_secret(secret_reference, secret).await;
        }
        secret.validate()?;
        if secret.revision != 1 {
            return Err(String::from(
                "An isolated AI credential must begin at revision one.",
            ));
        }
        let mut current = self
            .secret
            .lock()
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))?;
        if current.is_some() {
            return Err(format!(
                "Isolated AI credential `{secret_reference}` is already configured."
            ));
        }
        *current = Some(secret.clone());
        Ok(())
    }

    /// Replaces the isolated candidate through exact in-memory revision comparison.
    /// 通过精确的内存修订号比较替换隔离候选。
    async fn replace_secret(
        &self,
        secret_reference: &str,
        expected_revision: u64,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        if !self.owns(secret_reference) {
            return self
                .delegate
                .replace_secret(secret_reference, expected_revision, secret)
                .await;
        }
        secret.validate()?;
        let expected_next = expected_revision
            .checked_add(1)
            .ok_or_else(|| String::from("The isolated AI credential revision is exhausted."))?;
        if secret.revision != expected_next {
            return Err(String::from(
                "Replacing an isolated AI credential must advance exactly one revision.",
            ));
        }
        let mut current = self
            .secret
            .lock()
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))?;
        let current_revision = current
            .as_ref()
            .map(|current| current.revision)
            .ok_or_else(|| {
                format!("Isolated AI credential `{secret_reference}` is not configured.")
            })?;
        if current_revision != expected_revision {
            return Err(format!(
                "Isolated AI credential `{secret_reference}` changed concurrently."
            ));
        }
        *current = Some(secret.clone());
        Ok(())
    }

    /// Deletes only the in-memory candidate while delegating unrelated references.
    /// 仅删除内存候选，同时委托无关引用。
    async fn delete_secret(&self, secret_reference: &str) -> Result<(), String> {
        if !self.owns(secret_reference) {
            return self.delegate.delete_secret(secret_reference).await;
        }
        self.secret
            .lock()
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))?
            .take();
        Ok(())
    }

    /// Restores the exact in-memory candidate only when its slot is absent.
    /// 仅在槽位缺失时恢复精确的内存候选。
    async fn restore_secret(
        &self,
        secret_reference: &str,
        secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        if !self.owns(secret_reference) {
            return self.delegate.restore_secret(secret_reference, secret).await;
        }
        secret.validate()?;
        let mut current = self
            .secret
            .lock()
            .map_err(|_| String::from("The isolated AI credential lock is poisoned."))?;
        if current.is_some() {
            return Err(format!(
                "Isolated AI credential `{secret_reference}` is already configured."
            ));
        }
        *current = Some(secret.clone());
        Ok(())
    }
}

/// Rejects credential reads when the host has not installed a secure resolver.
/// 在宿主尚未安装安全解析器时拒绝凭据读取。
#[derive(Debug, Default)]
pub struct UnavailableCredentialSecretResolver;

#[async_trait]
impl CredentialSecretResolver for UnavailableCredentialSecretResolver {
    /// Rejects the optional read because no secure host boundary exists.
    /// 因不存在安全宿主边界而拒绝可选读取。
    async fn read_optional_secret(
        &self,
        _secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        Err(String::from(
            "The host has not installed an AI credential secret resolver.",
        ))
    }
}

#[async_trait]
impl CredentialSecretStore for UnavailableCredentialSecretResolver {
    /// Rejects secret creation because no host store exists.
    /// 因不存在宿主存储而拒绝秘密创建。
    async fn create_secret(
        &self,
        _secret_reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from(
            "The host has not installed an AI credential secret store.",
        ))
    }

    /// Rejects secret replacement because no host store exists.
    /// 因不存在宿主存储而拒绝秘密替换。
    async fn replace_secret(
        &self,
        _secret_reference: &str,
        _expected_revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from(
            "The host has not installed an AI credential secret store.",
        ))
    }

    /// Rejects secret deletion because no host store exists.
    /// 因不存在宿主存储而拒绝秘密删除。
    async fn delete_secret(&self, _secret_reference: &str) -> Result<(), String> {
        Err(String::from(
            "The host has not installed an AI credential secret store.",
        ))
    }

    /// Rejects rollback restoration because no host store exists.
    /// 因不存在宿主存储而拒绝回滚恢复。
    async fn restore_secret(
        &self,
        _secret_reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        Err(String::from(
            "The host has not installed an AI credential secret store.",
        ))
    }
}

#[cfg(test)]
mod tests;

impl PublicAiRuntime {
    /// Resolves a native account's current credential under the shared rotation lock.
    /// 在共享轮换锁下解析原生账户当前凭据。
    /// configuration/connection_id select the persisted account; the returned secret remains backend-only.
    /// configuration/connection_id 选择持久账户；返回秘密仅限后端使用。
    pub async fn resolve_openvulcan_account_credential(
        &self,
        configuration: &crate::PersistedAiConfiguration,
        connection_id: &str,
    ) -> Result<CredentialExecutionSecret, String> {
        let (_, entry, connection) =
            self.resolve_connection_metadata(configuration, connection_id)?;
        if entry.authentication != crate::AuthenticationProjection::OpenVulcanAuth {
            return Err(String::from(
                "Selected connection is not an OpenVulcan account.",
            ));
        }
        self.resolve_fresh_execution_secret(connection_id, &connection.secret_reference)
            .await
    }
    /// Resolves one access secret after coordinating any required OAuth refresh.
    /// 在协调完成任何所需 OAuth 刷新后解析一个访问秘密。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` scopes the process-local single-flight lock and `secret_reference`
    /// identifies the exact operating-system credential entry.
    /// `connection_id` 确定进程内单飞锁作用域，`secret_reference`
    /// 标识精确的操作系统凭据条目。
    ///
    /// # Returns
    /// 返回值
    /// An immutable execution secret from the current or atomically refreshed revision.
    /// 返回来自当前修订版或原子刷新修订版的不可变执行秘密。
    pub(super) async fn resolve_fresh_execution_secret(
        &self,
        connection_id: &str,
        secret_reference: &str,
    ) -> Result<CredentialExecutionSecret, String> {
        self.execution_credential_lease(connection_id, secret_reference)
            .resolve_for_request()
            .await
    }

    /// Creates one live credential lease for an exact frozen connection and secret reference.
    /// 为一个精确冻结连接与秘密引用创建实时凭据租约。
    ///
    /// # Parameters
    /// 参数
    ///
    /// `connection_id` scopes refresh serialization and `secret_reference` fixes account identity.
    /// `connection_id` 限定刷新串行化范围，`secret_reference` 固定账号身份。
    ///
    /// # Returns
    /// 返回值
    ///
    /// A lease that can observe later revisions without changing account or route.
    /// 返回能够观察后续修订版且不会改变账号或路由的租约。
    pub(super) fn execution_credential_lease(
        &self,
        connection_id: &str,
        secret_reference: &str,
    ) -> ProviderExecutionCredentialLease {
        ProviderExecutionCredentialLease::new(
            connection_id.to_string(),
            secret_reference.to_string(),
            Arc::clone(&self.oauth),
            Arc::clone(&self.refresh_locks),
            Arc::clone(&self.secrets),
        )
    }
}
