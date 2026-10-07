//! Bounded, process-local capability tables; no secret implements Debug or reaches persistence.
//! 有界进程内能力表；秘密不实现 Debug，也不进入持久存储。

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use uuid::Uuid;
use vulcan_service_api::{HtmlAnswerSourceIdentity, HtmlAnswerViewError};
use vulcan_session_core::v1::html_answers::*;

/// One result grant retained across its ticket-to-lease transition.
/// 在票据到租约转换过程中保留的一份结果授权。
#[derive(Clone)]
pub(super) struct Grant {
    /// Public revocation identity, independent of all bearer secrets.
    /// 公开撤销身份，与所有持有者秘密独立。
    pub id: Uuid,
    /// Authenticated issuing client's lifetime identity.
    /// 已鉴权签发客户端的生命周期身份。
    pub client: Uuid,
    /// Exact source facts revalidated before every content delivery.
    /// 每次内容交付前重新校验的精确源码事实。
    pub source: HtmlAnswerSourceIdentity,
    /// Shared-catalog locale for the trusted viewer only.
    /// 仅用于可信查看器的共享目录语言。
    pub locale: String,
    /// Monotonic expiry, never derived from adjustable wall-clock time.
    /// 单调到期时间，绝不依赖可调整的墙上时间。
    pub expires: Instant,
}

/// A reserved lease counts toward capacity even while source validation is in flight.
/// 源码校验进行中时，预留租约也计入容量。
pub(super) struct Lease {
    /// Immutable grant ownership and its lease deadline.
    /// 不可变授权归属及其租约期限。
    pub grant: Grant,
    /// Only a completed source check makes content readable.
    /// 只有源码检查完成后才允许读取内容。
    pub ready: bool,
}

/// Token bucket with bounded burst and replenishment from one shared Core policy.
/// 突发和补充速率均来自共享 Core 策略的令牌桶。
struct IssueRate {
    /// Fractional remaining issuance allowance.
    /// 剩余签发额度，允许小数补充。
    credit: f64,
    /// Last request instant, also used to reclaim idle client entries.
    /// 最后请求时间，同时用于回收闲置客户端条目。
    updated: Instant,
}

/// Mutable bounded authority; callers hold one short mutex and never await under it.
/// 可变有界权威；调用方只持有短时互斥锁，绝不持锁等待异步操作。
#[derive(Default)]
pub(super) struct Grants {
    /// Single-use tickets indexed by OS-random secret.
    /// 以操作系统随机秘密索引的单次票据。
    pub tickets: HashMap<String, Grant>,
    /// Pending and active leases indexed by independent OS-random secret.
    /// 以独立操作系统随机秘密索引的待确认及活动租约。
    pub leases: HashMap<String, Lease>,
    /// Bounded client buckets; capacity derives from the maximum possible pending issuers.
    /// 有界客户端令牌桶；容量派生自最大待处理签发者数量。
    rates: HashMap<Uuid, IssueRate>,
    /// Shutdown permanently closes admission, including already in-flight issuers.
    /// 关闭后永久停止准入，包含已经进行中的签发。
    pub closed: bool,
}

impl Grants {
    /// Reclaim expired capabilities and fully replenished idle buckets at `now`.
    /// 在 `now` 回收过期能力和已经完全补充的闲置令牌桶。
    pub fn prune(&mut self, now: Instant) {
        self.tickets.retain(|_, grant| grant.expires > now);
        self.leases.retain(|_, lease| lease.grant.expires > now);
        self.rates
            .retain(|_, rate| now.duration_since(rate.updated) < Duration::from_secs(60));
    }

    /// Charge one authenticated client's attempt before expensive validation; return a stable rejection.
    /// 在昂贵校验前计入已鉴权客户端的一次尝试；返回稳定拒绝原因。
    pub fn admit_issue(&mut self, client: Uuid, now: Instant) -> Result<(), HtmlAnswerViewError> {
        self.prune(now);
        if self.closed {
            return Err(HtmlAnswerViewError::Closed);
        }
        if !self.rates.contains_key(&client) && self.rates.len() >= HTML_ANSWER_MAX_TICKETS {
            return Err(HtmlAnswerViewError::Capacity);
        }
        let rate = self.rates.entry(client).or_insert(IssueRate {
            credit: f64::from(HTML_ANSWER_TICKET_BURST),
            updated: now,
        });
        rate.credit = (rate.credit
            + now.duration_since(rate.updated).as_secs_f64()
                * f64::from(HTML_ANSWER_TICKETS_PER_MINUTE)
                / 60.0)
            .min(f64::from(HTML_ANSWER_TICKET_BURST));
        rate.updated = now;
        if rate.credit < 1.0 {
            return Err(HtmlAnswerViewError::RateLimited);
        }
        rate.credit -= 1.0;
        Ok(())
    }

    /// Insert a validated ticket under both global and client limits; return its fresh secret.
    /// 在全局和客户端上限内插入已校验票据；返回新秘密。
    pub fn issue(&mut self, grant: Grant, now: Instant) -> Result<String, HtmlAnswerViewError> {
        self.prune(now);
        if self.closed {
            return Err(HtmlAnswerViewError::Closed);
        }
        if self.tickets.len() >= HTML_ANSWER_MAX_TICKETS
            || self
                .tickets
                .values()
                .filter(|entry| entry.client == grant.client)
                .count()
                >= HTML_ANSWER_MAX_CLIENT_TICKETS
        {
            return Err(HtmlAnswerViewError::Capacity);
        }
        let secret = new_secret()?;
        if self.tickets.contains_key(&secret) {
            return Err(HtmlAnswerViewError::Unavailable);
        }
        self.tickets.insert(secret.clone(), grant);
        Ok(secret)
    }

    /// Atomically consume `ticket` and reserve capacity for one exact result; ready remains false.
    /// 原子消费 `ticket` 并为一个精确结果预留容量；ready 保持为假。
    /// `source` is checked against the viewer route so a swapped fragment cannot select another result.
    /// 对照查看器路由校验 `source`，避免被替换的片段选中另一结果。
    pub fn reserve_lease(
        &mut self,
        ticket: &str,
        session: Uuid,
        artifact: &str,
        now: Instant,
    ) -> Result<(String, Grant), HtmlAnswerViewError> {
        self.prune(now);
        if self.closed {
            return Err(HtmlAnswerViewError::Closed);
        }
        let grant = self
            .tickets
            .get(ticket)
            .ok_or(HtmlAnswerViewError::Expired)?;
        if grant.source.session_id != session || grant.source.artifact_id != artifact {
            return Err(HtmlAnswerViewError::InvalidRequest);
        }
        if self.leases.len() >= HTML_ANSWER_MAX_LEASES
            || self
                .leases
                .values()
                .filter(|entry| entry.grant.client == grant.client)
                .count()
                >= HTML_ANSWER_MAX_CLIENT_LEASES
        {
            return Err(HtmlAnswerViewError::Capacity);
        }
        let secret = new_secret()?;
        if self.leases.contains_key(&secret) {
            return Err(HtmlAnswerViewError::Unavailable);
        }
        let mut grant = self
            .tickets
            .remove(ticket)
            .ok_or(HtmlAnswerViewError::Expired)?;
        grant.expires = now + Duration::from_secs(HTML_ANSWER_LEASE_SECONDS);
        self.leases.insert(
            secret.clone(),
            Lease {
                grant: grant.clone(),
                ready: false,
            },
        );
        Ok((secret, grant))
    }

    /// Revoke only `client`'s selected `id`, including a source check currently in flight.
    /// 仅撤销 `client` 选定的 `id`，包含正在进行源码检查的授权。
    pub fn revoke(&mut self, client: Uuid, id: Uuid) {
        self.tickets
            .retain(|_, grant| grant.client != client || grant.id != id);
        self.leases
            .retain(|_, lease| lease.grant.client != client || lease.grant.id != id);
    }

    /// Permanently close this runtime and remove every retained secret and rate entry.
    /// 永久关闭当前运行时，并移除所有保留的秘密和速率条目。
    pub fn close(&mut self) {
        self.closed = true;
        self.tickets.clear();
        self.leases.clear();
        self.rates.clear();
    }
}

/// Generate a URL-safe capability from the OS CSPRNG; failure never falls back to weaker entropy.
/// 从操作系统安全随机源生成 URL 安全能力；失败时绝不回退到较弱熵源。
fn new_secret() -> Result<String, HtmlAnswerViewError> {
    let mut bytes = [0u8; HTML_ANSWER_SECRET_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| HtmlAnswerViewError::Unavailable)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
