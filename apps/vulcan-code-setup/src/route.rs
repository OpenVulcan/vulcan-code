//! Measure the installer origins once and apply the result to automatic downloads.
//! 在安装器进程内测速一次，并将结果用于自动线路下载。

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use vulcan_install_core::error::FormatResult;
use vulcan_install_core::fetch::{Downloader, Route};
use vulcan_install_core::layout::{CHINA_ORIGIN, PRIMARY_ORIGIN, setup_pointer_key};
use vulcan_install_core::names::Channel;

/// Total time allowed for either latency probe, including connection setup.
/// 任一延迟探测所允许的总时长，包含连接建立时间。
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// The same observed default is used by the first screen, self-update, and product download.
/// 首屏、自更新与产品下载共用同一测速结果。
static RECOMMENDED_ROUTE: OnceLock<Route> = OnceLock::new();

/// Returns the fastest reachable setup origin, or Auto if neither probe answered successfully.
/// 返回响应最快的可达安装源，两边均未成功响应时返回自动线路。
pub(crate) fn recommendation() -> Route {
    *RECOMMENDED_ROUTE.get_or_init(|| {
        let tls = ureq::tls::TlsConfig::builder()
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent = ureq::Agent::config_builder()
            .user_agent(crate::install::USER_AGENT)
            .tls_config(tls)
            .timeout_global(Some(PROBE_TIMEOUT))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .new_agent();
        let key = setup_pointer_key(Channel::Stable);
        std::thread::scope(|scope| {
            let china = scope.spawn(|| measure(&agent, CHINA_ORIGIN, &key));
            let international = measure(&agent, PRIMARY_ORIGIN, &key);
            fastest(international, china.join().unwrap_or_default())
        })
    })
}

/// Measures time to a successful HTTPS response for the stable setup pointer on `origin`.
/// 测量从 `origin` 的稳定安装器指针收到成功 HTTPS 响应的耗时。
/// Returns no timing when the pointer cannot be reached within the probe deadline.
/// 指针未能在探测期限内访问时，不返回测速结果。
fn measure(agent: &ureq::Agent, origin: &str, key: &str) -> Option<Duration> {
    let started = Instant::now();
    let response = agent.head(format!("{origin}/{key}")).call().ok()?;
    (response.status() == 200).then(|| started.elapsed())
}

/// Chooses the shorter successful latency and breaks a tie in favor of the primary origin.
/// 选择耗时较短的成功响应，耗时相同时优先主源。
fn fastest(international: Option<Duration>, china: Option<Duration>) -> Route {
    match (international, china) {
        (Some(international), Some(china)) if china < international => Route::China,
        (Some(_), _) => Route::International,
        (None, Some(_)) => Route::China,
        (None, None) => Route::Auto,
    }
}

/// Builds a downloader for `route`, preferring the measured origin only in Auto mode.
/// 按 `route` 创建下载器，仅在自动模式下优先使用测速选中的来源。
/// Explicit routes retain their single origin, and the normal signed download checks still apply.
/// 显式线路只使用指定来源，正常下载仍执行签名校验。
pub(crate) fn downloader(route: Route) -> FormatResult<Downloader> {
    let mut downloader = Downloader::new(route.origins(), crate::install::USER_AGENT)?;
    if route == Route::Auto {
        match recommendation() {
            Route::China => downloader.prefer(CHINA_ORIGIN),
            Route::International => downloader.prefer(PRIMARY_ORIGIN),
            Route::Auto => (),
        }
    }
    Ok(downloader)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reachable origin wins over failure, and the faster of two successful origins wins.
    /// 可达节点优先于失败节点，两个节点均可达时取较快者。
    #[test]
    fn chooses_a_reachable_fast_default() {
        let quick = Some(Duration::from_millis(25));
        let slow = Some(Duration::from_millis(150));
        assert_eq!(fastest(slow, quick), Route::China);
        assert_eq!(fastest(quick, slow), Route::International);
        assert_eq!(fastest(quick, quick), Route::International);
        assert_eq!(fastest(None, quick), Route::China);
        assert_eq!(fastest(quick, None), Route::International);
        assert_eq!(fastest(None, None), Route::Auto);
    }
}
