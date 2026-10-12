//! Credential revision, unauthorized recovery, and candidate isolation regressions.
//! 凭据修订版、未授权恢复与候选隔离回归测试。

use super::super::test_support::TestStaticSecretStore;
use super::*;
use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;

/// Persists a native rotation fence before failure and proves restart or deletion cannot replay it.
/// 在失败前持久化原生轮换标记，并证明重启或删除后不能重放。
#[tokio::test]
async fn openvulcan_rotation_fence_survives_failure_and_new_leases() {
    // initial is already expired, so production refresh must fail without contacting any service.
    // initial 已过期，因此生产刷新必须失败且不访问任何服务。
    let initial = CredentialSecretEnvelope::new(crate::CredentialSecretMaterial::OpenVulcanAuth {
        access_token: "a".repeat(43),
        authorization_id: uuid::Uuid::new_v4().to_string(),
        region: crate::OpenVulcanRegion::China,
        expires_at_unix_ms: 1,
        rotation_pending: false,
    })
    .expect("native fixture");
    // store retains the durable image across independent lease generations.
    // store 跨独立租约代际保留持久镜像。
    let store = Arc::new(IsolatedCredentialSecretStore {
        delegate: Arc::new(UnavailableCredentialSecretResolver),
        secret_reference: String::from("native-secret"),
        secret: StdMutex::new(Some(initial)),
    });
    // lease uses the same production CAS fence and refresh path as a real provider request.
    // lease 使用与实际供应商请求相同的生产 CAS 标记及刷新路径。
    let lease = ProviderExecutionCredentialLease::new(
        String::from("native-account"),
        String::from("native-secret"),
        Arc::new(OAuthAuthorizationRuntime::new(reqwest::Client::new())),
        Arc::new(StdMutex::new(BTreeMap::new())),
        store.clone(),
    );
    assert!(lease.resolve_for_request().await.is_err());
    let fenced = store
        .read_secret("native-secret")
        .await
        .expect("fence persisted");
    assert_eq!(fenced.revision, 2);
    assert!(matches!(
        fenced.material,
        crate::CredentialSecretMaterial::OpenVulcanAuth {
            rotation_pending: true,
            ..
        }
    ));
    // restarted gets a fresh lock and runtime, proving the persistent flag owns replay prevention.
    // restarted 获得新的锁和运行时，证明持久标记持有防重放权威。
    let restarted = ProviderExecutionCredentialLease::new(
        String::from("native-account"),
        String::from("native-secret"),
        Arc::new(OAuthAuthorizationRuntime::new(reqwest::Client::new())),
        Arc::new(StdMutex::new(BTreeMap::new())),
        store.clone(),
    );
    assert!(restarted.resolve_for_request().await.is_err());
    assert_eq!(
        store.read_secret("native-secret").await.unwrap().revision,
        fenced.revision
    );
    store.delete_secret("native-secret").await.unwrap();
    assert!(lease.resolve_for_request().await.is_err());
    assert!(
        store
            .read_optional_secret("native-secret")
            .await
            .unwrap()
            .is_none()
    );
}

/// Builds one valid Kimi OAuth envelope for live credential-lease tests.
/// 为实时凭据租约测试构建一份有效 Kimi OAuth 封装。
///
/// # Parameters
/// 参数
///
/// `revision` is the secure-store revision and `access_token` is the observable test value.
/// `revision` 是安全仓库修订号，`access_token` 是可观测测试值。
///
/// # Returns
/// 返回值
///
/// A non-expiring renewable credential whose refresh endpoint is never contacted by these tests.
/// 返回一份不会过期的可续期凭据；这些测试不会访问其刷新端点。
fn test_kimi_oauth_secret(revision: u64, access_token: &str) -> CredentialSecretEnvelope {
    // now anchors a valid issue and expiration interval for the credential invariant.
    // now 为凭据不变量提供有效的签发与过期时间区间。
    let now = current_unix_millis().expect("test clock should be available");
    CredentialSecretEnvelope {
        version: crate::CREDENTIAL_SECRET_FORMAT_VERSION,
        revision,
        material: crate::CredentialSecretMaterial::KimiCodingPlanOAuth {
            tokens: crate::OAuthTokenSet {
                access_token: access_token.to_string(),
                refresh_token: String::from("refresh-token"),
                issued_at_unix_ms: now,
                expires_at_unix_ms: now.saturating_add(60 * 60 * 1_000),
                scopes: BTreeSet::new(),
                account_subject: Some(String::from("account-subject")),
                email: Some(String::from("account@example.com")),
            },
            device_id: String::from("00000000-0000-4000-8000-000000000001"),
        },
    }
}

/// Verifies the next physical request observes a concurrently refreshed OAuth revision.
/// 验证下一次真实请求会观察到并发刷新的 OAuth 修订版。
#[tokio::test]
async fn execution_credential_lease_observes_latest_revision_after_refresh() {
    // initial is the exact token revision used by the rejected physical request.
    // initial 是被拒绝真实请求使用的精确 Token 修订版。
    let initial = test_kimi_oauth_secret(1, "token-a");
    // store provides a mutable isolated secure-store boundary without durable writes.
    // store 提供不产生持久写入的可变隔离安全仓库边界。
    let store = Arc::new(IsolatedCredentialSecretStore {
        delegate: Arc::new(UnavailableCredentialSecretResolver),
        secret_reference: String::from("oauth-secret"),
        secret: StdMutex::new(Some(initial.clone())),
    });
    // secrets erases the concrete fixture behind the production credential-store contract.
    // secrets 通过生产凭据仓库契约抹除具体夹具类型。
    let secrets: Arc<dyn CredentialSecretStore> = store.clone();
    // lease freezes the account reference while resolving its token revision per request.
    // lease 冻结账号引用，同时按请求解析其 Token 修订版。
    let lease = ProviderExecutionCredentialLease::new(
        String::from("oauth-connection"),
        String::from("oauth-secret"),
        Arc::new(OAuthAuthorizationRuntime::new(reqwest::Client::new())),
        Arc::new(StdMutex::new(BTreeMap::new())),
        secrets,
    );
    // rejected is the first physical request's immutable credential snapshot.
    // rejected 是第一次真实请求的不可变凭据快照。
    let rejected = lease
        .resolve_for_request()
        .await
        .expect("initial credential should resolve");
    assert_eq!(rejected.revision(), 1);
    assert_eq!(rejected.expose_value(), "token-a");

    // refreshed simulates another request completing OAuth refresh while this execution remains active.
    // refreshed 模拟当前执行仍活动时另一个请求完成 OAuth 刷新。
    let refreshed = test_kimi_oauth_secret(2, "token-b");
    store
        .replace_secret("oauth-secret", 1, &refreshed)
        .await
        .expect("concurrent refresh should replace revision one");
    assert!(
        lease
            .refresh_after_unauthorized(rejected.revision())
            .await
            .expect("newer revision should satisfy unauthorized recovery")
    );
    // current is the credential that the immediately following physical request will send.
    // current 是紧接着的真实请求将要发送的凭据。
    let current = lease
        .resolve_for_request()
        .await
        .expect("latest credential should resolve");
    assert_eq!(current.revision(), 2);
    assert_eq!(current.expose_value(), "token-b");
    assert!(
        !lease
            .refresh_after_unauthorized(rejected.revision())
            .await
            .expect("same physical request must not refresh twice")
    );
    lease.mark_request_succeeded();
    // later simulates a subsequent physical request observing another concurrent OAuth refresh.
    // later 模拟后续真实请求观察到另一次并发 OAuth 刷新。
    let later = test_kimi_oauth_secret(3, "token-c");
    store
        .replace_secret("oauth-secret", current.revision(), &later)
        .await
        .expect("later refresh should replace revision two");
    assert!(
        lease
            .refresh_after_unauthorized(current.revision())
            .await
            .expect("successful request should restore one recovery opportunity")
    );
    // latest is the token resolved for the physical request after the second recovery.
    // latest 是第二次恢复后真实请求解析到的 Token。
    let latest = lease
        .resolve_for_request()
        .await
        .expect("latest credential should resolve after guard reset");
    assert_eq!(latest.revision(), 3);
    assert_eq!(latest.expose_value(), "token-c");
}

/// Verifies static API keys remain terminal when an upstream request returns 401.
/// 验证上游请求返回 401 时静态 API Key 仍保持终态。
#[tokio::test]
async fn execution_credential_lease_does_not_refresh_static_api_key() {
    // store owns one immutable user-managed API key.
    // store 持有一份不可变的用户管理 API Key。
    let store: Arc<dyn CredentialSecretStore> = Arc::new(TestStaticSecretStore::api_key());
    // lease uses the same production boundary as renewable credentials.
    // lease 使用与可续期凭据相同的生产边界。
    let lease = ProviderExecutionCredentialLease::new(
        String::from("api-key-connection"),
        String::from("api-key-secret"),
        Arc::new(OAuthAuthorizationRuntime::new(reqwest::Client::new())),
        Arc::new(StdMutex::new(BTreeMap::new())),
        store,
    );
    // secret proves the static key remains readable for ordinary dispatch.
    // secret 证明静态 Key 仍可供普通分派读取。
    let secret = lease
        .resolve_for_request()
        .await
        .expect("static API key should resolve");
    assert!(
        !lease
            .refresh_after_unauthorized(secret.revision())
            .await
            .expect("static API key recovery should be a controlled no-op")
    );
}

/// Counts every call that reaches the durable-store side of an isolated credential overlay.
/// 统计每次触达隔离凭据覆盖层持久仓库一侧的调用。
#[derive(Default)]
struct TestCountingSecretStore {
    /// Number of delegated reads.
    /// 被委托读取次数。
    reads: AtomicUsize,
    /// Number of delegated creates.
    /// 被委托创建次数。
    creates: AtomicUsize,
    /// Number of delegated replacements.
    /// 被委托替换次数。
    replaces: AtomicUsize,
    /// Number of delegated deletes.
    /// 被委托删除次数。
    deletes: AtomicUsize,
    /// Number of delegated restorations.
    /// 被委托恢复次数。
    restores: AtomicUsize,
}

#[async_trait]
impl CredentialSecretResolver for TestCountingSecretStore {
    /// Counts and rejects every durable read.
    /// 统计并拒绝每次持久读取。
    async fn read_optional_secret(
        &self,
        _secret_reference: &str,
    ) -> Result<Option<CredentialSecretEnvelope>, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Err(String::from("The counting store rejects durable reads."))
    }
}

#[async_trait]
impl CredentialSecretStore for TestCountingSecretStore {
    /// Counts and rejects every durable create.
    /// 统计并拒绝每次持久创建。
    async fn create_secret(
        &self,
        _secret_reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Err(String::from("The counting store rejects durable creates."))
    }

    /// Counts and rejects every durable replacement.
    /// 统计并拒绝每次持久替换。
    async fn replace_secret(
        &self,
        _secret_reference: &str,
        _expected_revision: u64,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        self.replaces.fetch_add(1, Ordering::SeqCst);
        Err(String::from(
            "The counting store rejects durable replacements.",
        ))
    }

    /// Counts and rejects every durable deletion.
    /// 统计并拒绝每次持久删除。
    async fn delete_secret(&self, _secret_reference: &str) -> Result<(), String> {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        Err(String::from("The counting store rejects durable deletes."))
    }

    /// Counts and rejects every durable restoration.
    /// 统计并拒绝每次持久恢复。
    async fn restore_secret(
        &self,
        _secret_reference: &str,
        _secret: &CredentialSecretEnvelope,
    ) -> Result<(), String> {
        self.restores.fetch_add(1, Ordering::SeqCst);
        Err(String::from(
            "The counting store rejects durable restorations.",
        ))
    }
}

/// Verifies an isolated candidate can read and mutate its secret without durable-store calls.
/// 验证隔离候选可以读取和修改自身秘密且不会调用持久仓库。
#[tokio::test]
async fn isolated_candidate_secret_never_reaches_durable_store() {
    // durableStore records any accidental boundary crossing.
    // durableStore 记录任何意外的边界穿透。
    let durable_store = Arc::new(TestCountingSecretStore::default());
    // initialSecret is the exact revision-one candidate supplied by a create request.
    // initialSecret 是创建请求提供的精确第一修订版候选。
    let initial_secret = CredentialSecretEnvelope::new_api_key(String::from("candidate-key"))
        .expect("candidate secret should validate");
    // isolatedStore shadows only the candidate reference.
    // isolatedStore 仅遮蔽候选引用。
    let isolated_store = IsolatedCredentialSecretStore::new(
        durable_store.clone(),
        String::from("candidate-secret"),
        initial_secret.clone(),
    );

    // observedSecret proves the overlay returns the exact in-memory envelope.
    // observedSecret 证明覆盖层返回精确的内存封装。
    let observed_secret = isolated_store
        .read_secret("candidate-secret")
        .await
        .expect("isolated read should succeed");
    assert!(observed_secret == initial_secret);
    // replacement advances only the in-memory revision.
    // replacement 仅推进内存修订版。
    let replacement = initial_secret
        .replace_api_key(String::from("replacement-key"))
        .expect("replacement secret should validate");
    isolated_store
        .replace_secret("candidate-secret", initial_secret.revision, &replacement)
        .await
        .expect("isolated replacement should succeed");
    isolated_store
        .delete_secret("candidate-secret")
        .await
        .expect("isolated deletion should succeed");
    isolated_store
        .restore_secret("candidate-secret", &replacement)
        .await
        .expect("isolated restoration should succeed");

    assert_eq!(durable_store.reads.load(Ordering::SeqCst), 0);
    assert_eq!(durable_store.creates.load(Ordering::SeqCst), 0);
    assert_eq!(durable_store.replaces.load(Ordering::SeqCst), 0);
    assert_eq!(durable_store.deletes.load(Ordering::SeqCst), 0);
    assert_eq!(durable_store.restores.load(Ordering::SeqCst), 0);
    assert!(
        isolated_store
            .read_optional_secret("unrelated-secret")
            .await
            .is_err(),
        "an unrelated reference must still delegate"
    );
    assert_eq!(durable_store.reads.load(Ordering::SeqCst), 1);
}
