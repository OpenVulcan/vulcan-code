//! Shared official Xiaomi MiMo API and regional Token Plan route validation.
//! 共享小米 MiMo 官方 API 和区域 Token Plan 路由校验。

/// Classifies the services supported by Xiaomi's browser and account APIs.
/// 区分小米浏览器授权与账户接口支持的服务。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MiMoService {
    /// Pay-as-you-go API shared by mainland China and international accounts.
    /// 中国大陆与国际账户共用的按量 API。
    PayAsYouGo,
    /// Personal Token Plan on a platform-selected cluster.
    /// 使用平台所选集群的个人 Token Plan。
    PersonalTokenPlan,
}

impl MiMoService {
    /// Accepts only official OpenAI-compatible base URLs returned by MiMo's authorization page.
    /// 仅接受 MiMo 授权页返回的官方 OpenAI 兼容基础地址。
    /// `base_url` is the exact route from the manifest or authorization payload.
    /// `base_url` 为清单或授权载荷中的精确路由。
    pub(crate) fn for_base_url(base_url: &str) -> Result<Self, String> {
        match base_url {
            "https://api.xiaomimimo.com/v1" => Ok(Self::PayAsYouGo),
            "https://token-plan-cn.xiaomimimo.com/v1"
            | "https://token-plan-sgp.xiaomimimo.com/v1"
            | "https://token-plan-ams.xiaomimimo.com/v1" => Ok(Self::PersonalTokenPlan),
            _ => Err(String::from(
                "Xiaomi MiMo requires an official service URL.",
            )),
        }
    }

    /// Returns the official key prefix for this service's browser and finance APIs.
    /// 返回本服务浏览器授权和财务接口的官方密钥前缀。
    pub(crate) fn key_prefix(self) -> &'static str {
        match self {
            Self::PayAsYouGo => "sk-",
            Self::PersonalTokenPlan => "tp-",
        }
    }
}
