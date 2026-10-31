// RTK page: routing switches, the route checklist, and token-saving statistics, mirroring the
// WebUI RTK page under plugin management. Every change saves the complete settings; the next turn
// reads them.
// RTK 页：路由开关、路由清单与 Token 节省统计，镜像插件管理下的 WebUI RTK 页。每次修改都保存完整
// 设置；下一回合即读取。

use super::*;
use crate::service_api::rtk as api_rtk;
use crate::ui::text::{compact_count, compact_duration};

/// The offered route catalog: curated routes, then extended routes.
/// 可选路由目录：先精选路由，再扩展路由。
fn universe(snapshot: &Value) -> Vec<String> {
    a(snapshot, "/curatedRoutes")
        .iter()
        .chain(a(snapshot, "/extendedRoutes"))
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The list the route mode owns: the denylist in `all` mode, else the allowlist.
/// 路由模式拥有的列表：`all` 模式为拒绝列表，否则为允许列表。
fn owned_list(settings: &Value) -> &'static str {
    if s(settings, "/routeMode") == "all" {
        "disabledRoutes"
    } else {
        "enabledRoutes"
    }
}

/// Whether `route` is checked: listed in `selected` mode, not listed in `all` mode.
/// `route` 是否勾选：`selected` 模式下已列出，`all` 模式下未列出。
fn checked(settings: &Value, route: &str) -> bool {
    let listed = a(settings, &format!("/{}", owned_list(settings)))
        .iter()
        .any(|entry| entry.as_str() == Some(route));
    listed != (owned_list(settings) == "disabledRoutes")
}

/// Checks or unchecks one route; `ctx` carries `{ settings, route }`. The owned list stays sorted
/// and keeps its entries outside the offered catalog.
/// 勾选或取消勾选一条路由；`ctx` 携带 `{ settings, route }`。所属列表保持排序，并保留可选目录之外的条目。
fn set_route(ctx: &Value, value: &SettingValue) -> Option<ServiceCall> {
    let mut settings = v(ctx, "/settings").clone();
    let route = v(ctx, "/route").as_str()?;
    let list = owned_list(&settings);
    // The owned list contains the route when checked in `selected` mode or unchecked in `all` mode.
    // 所属列表在 `selected` 模式勾选或 `all` 模式取消勾选时包含该路由。
    let include = value.as_bool() != (list == "disabledRoutes");
    let mut routes: Vec<String> = a(&settings, &format!("/{list}"))
        .iter()
        .filter_map(Value::as_str)
        .filter(|entry| *entry != route)
        .map(str::to_string)
        .collect();
    if include {
        routes.push(route.to_string());
    }
    routes.sort();
    settings[list] = json!(routes);
    Some(api_rtk::save_rtk_settings(settings))
}

/// Builds the settings and route rows.
/// 构建设置与路由行。
fn settings_rows(rows: &mut Vec<Row>, snapshot: &Value) {
    let settings = v(snapshot, "/settings");
    rows.push(section(tr!("rtk.section.settings")));
    for (pointer, label, help) in [
        (
            "/enabled",
            tr!("rtk.enableRouting"),
            tr!("rtk.enableRoutingHelp"),
        ),
        (
            "/gitCommitContextEnabled",
            tr!("rtk.gitContext"),
            tr!("rtk.gitContextHelp"),
        ),
        (
            "/telemetryDisabled",
            tr!("rtk.disableTelemetry"),
            tr!("rtk.disableTelemetryHelp"),
        ),
    ] {
        rows.push(
            toggle(
                label,
                b(settings, pointer),
                dto(settings.clone(), pointer, api_rtk::save_rtk_settings),
            )
            .detail(help),
        );
    }
    rows.push(section(tr!("rtk.routes.title")));
    rows.push(note(tr!("rtk.routes.description"), Tone::Muted));
    rows.push(
        choice(
            tr!("rtk.routeMode.label"),
            vec![
                ("disabled".into(), tr!("rtk.routeMode.disabled")),
                ("selected".into(), tr!("rtk.routeMode.selected")),
                ("all".into(), tr!("rtk.routeMode.all")),
            ],
            s(settings, "/routeMode"),
            dto(settings.clone(), "/routeMode", api_rtk::save_rtk_settings),
        )
        .detail(tr!("rtk.routeMode.help")),
    );
    if s(settings, "/routeMode") == "disabled" {
        return;
    }
    rows.push(note(
        if owned_list(settings) == "disabledRoutes" {
            tr!("rtk.routes.allHelp")
        } else {
            tr!("rtk.routes.selectedHelp")
        },
        Tone::Muted,
    ));
    for (title, pointer) in [
        (tr!("rtk.routes.curated"), "/curatedRoutes"),
        (tr!("rtk.routes.extended"), "/extendedRoutes"),
    ] {
        rows.push(section(title));
        for route in a(snapshot, pointer).iter().filter_map(Value::as_str) {
            rows.push(toggle(
                route.to_string(),
                checked(settings, route),
                Edit::Call(Setter::Build {
                    ctx: json!({ "settings": settings, "route": route }),
                    build: set_route,
                }),
            ));
        }
    }
    let total = universe(snapshot);
    let selected = total
        .iter()
        .filter(|route| checked(settings, route))
        .count();
    rows.push(note(
        tr!(
            "plugins.settings.selectedCount",
            selected = selected,
            total = total.len()
        ),
        Tone::Muted,
    ));
}

/// Formats a signed token count; a negative count means compression grew the output.
/// 格式化有符号 Token 数；负数表示压缩使输出变大。
fn signed_count(value: i64) -> String {
    let magnitude = compact_count(value.unsigned_abs());
    if value < 0 {
        format!("-{magnitude}")
    } else {
        magnitude
    }
}

/// Formats a percentage reported as `0..=100` with one decimal.
/// 将以 `0..=100` 报告的百分比格式化为一位小数。
fn percent(value: &Value) -> String {
    format!("{:.1}%", value.as_f64().unwrap_or(0.0))
}

/// Builds the statistics rows.
/// 构建统计行。
fn statistics_rows(rows: &mut Vec<Row>, statistics: &Value) {
    rows.push(section(tr!("rtk.stats.title")));
    rows.push(note(tr!("rtk.stats.description"), Tone::Muted));
    if statistics.is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
        return;
    }
    if !b(statistics, "/available") {
        rows.push(info(
            tr!("rtk.availability.unavailable"),
            s(statistics, "/error"),
            Tone::Warn,
        ));
        rows.push(note(tr!("rtk.stats.unavailableHelp"), Tone::Muted));
        return;
    }
    let summary = v(statistics, "/summary");
    let by_command = a(statistics, "/byCommand");
    if n(summary, "/totalCommands") == 0 && by_command.is_empty() {
        rows.push(note(tr!("rtk.stats.empty"), Tone::Muted));
    } else {
        let input = n(summary, "/totalInput");
        let output = n(summary, "/totalOutput").max(1);
        rows.push(stats(vec![
            (
                tr!("rtk.stats.savedTokens"),
                signed_count(n(summary, "/totalSaved")),
                Tone::Good,
            ),
            (
                tr!("rtk.stats.averageSavings"),
                percent(v(summary, "/avgSavingsPct")),
                Tone::Normal,
            ),
            (
                tr!("rtk.stats.commands"),
                n(summary, "/totalCommands").to_string(),
                Tone::Normal,
            ),
            (
                tr!("rtk.stats.capacityRatio"),
                format!("{:.1}×", input as f64 / output as f64),
                Tone::Normal,
            ),
            (
                tr!("rtk.stats.excludedOutputs"),
                n(summary, "/oversizedExcluded").to_string(),
                Tone::Muted,
            ),
        ]));
        rows.push(section(tr!("rtk.stats.daily")));
        for period in a(statistics, "/daily") {
            rows.push(info(
                s(period, "/start"),
                format!(
                    "{} · {}",
                    signed_count(n(period, "/savedTokens")),
                    percent(v(period, "/savingsPct"))
                ),
                Tone::Normal,
            ));
        }
        rows.push(section(tr!("rtk.stats.byCommand")));
        if by_command.is_empty() {
            rows.push(note(tr!("rtk.stats.noCommandBreakdown"), Tone::Muted));
        }
        for (index, entry) in by_command.iter().enumerate() {
            let command = s(entry, "/command");
            rows.push(
                item(&format!("{index}:{command}"), command, String::new()).lines(vec![format!(
                    "{} {} · {} {} · {} {} · {} {}",
                    tr!("rtk.stats.count"),
                    n(entry, "/count"),
                    tr!("rtk.stats.saved"),
                    signed_count(n(entry, "/savedTokens")),
                    tr!("rtk.stats.savingsRate"),
                    percent(v(entry, "/savingsPct")),
                    tr!("rtk.stats.time"),
                    compact_duration(v(entry, "/avgTimeMs").as_u64().unwrap_or(0)),
                )]),
            );
        }
    }
    rows.push(section(tr!("rtk.stats.historyTitle")));
    rows.push(
        button(
            tr!("rtk.stats.clearHistory"),
            confirm(
                tr!("rtk.stats.clearHistory"),
                tr!("rtk.stats.clearHistoryHelp"),
                tr!("rtk.stats.clearHistory"),
                call(api_rtk::clear_rtk_statistics()),
            ),
            Tone::Bad,
        )
        .detail(tr!("rtk.stats.clearHistoryHelp")),
    );
}

/// Builds the RTK page.
/// 构建 RTK 页。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let mut rows = Vec::new();
    let snapshot = app.slots.value(Slot::RtkSettings);
    if snapshot.is_null() {
        rows.push(note(tr!("tui.page.loading"), Tone::Muted));
    } else {
        settings_rows(&mut rows, snapshot);
    }
    statistics_rows(&mut rows, app.slots.value(Slot::RtkStatistics));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    // A route edit saves the complete settings and changes only the list the mode owns.
    // 路由编辑保存完整设置，且只修改模式所属的列表。
    #[test]
    fn route_edits_change_only_the_owned_list() {
        let selected = json!({
            "enabled": true, "routeMode": "selected", "enabledRoutes": ["git"],
            "disabledRoutes": ["ls"], "telemetryDisabled": true, "gitCommitContextEnabled": true
        });
        let saved = set_route(
            &json!({ "settings": selected, "route": "docker" }),
            &SettingValue::Bool(true),
        )
        .unwrap();
        assert_eq!(saved.method, "service_save_rtk_settings");
        let request = &saved.params["request"]["settings"];
        assert_eq!(request["enabledRoutes"], json!(["docker", "git"]));
        assert_eq!(request["disabledRoutes"], json!(["ls"]));
        assert_eq!(request["telemetryDisabled"], true);

        let mut all = selected.clone();
        all["routeMode"] = json!("all");
        assert!(checked(&all, "git"));
        assert!(!checked(&all, "ls"));
        let unchecked = set_route(
            &json!({ "settings": all, "route": "git" }),
            &SettingValue::Bool(false),
        )
        .unwrap();
        let request = &unchecked.params["request"]["settings"];
        assert_eq!(request["disabledRoutes"], json!(["git", "ls"]));
        assert_eq!(request["enabledRoutes"], json!(["git"]));
    }
}
