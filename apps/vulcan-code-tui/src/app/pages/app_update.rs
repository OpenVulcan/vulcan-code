// Application update page, mirroring the WebUI update settings area: the running version, check,
// download, restart, repair, the rollback notice, the AppArmor profile the Linux desktop app needs
// and the update settings. Every action answers with
// a fresh updater snapshot; progress arrives through `app.update.changed` events.
// 应用更新页，镜像 WebUI 更新设置区：正在运行的版本、检查、下载、重启、修复、回退提示、Linux 桌面应用
// 所需的 AppArmor 配置与更新设置。
// 每个操作都以新的更新器快照应答；进度经 `app.update.changed` 事件到达。

use super::permission_judge::format_bytes;
use super::settings::utc_time;
use super::*;
use crate::service_api::app_update as api_update;

/// Label of the reason updating is off.
/// 更新关闭原因的标签。
fn unavailable_label(reason: &str) -> String {
    match reason {
        "developmentBuild" => tr!("settings.appUpdate.unavailable.developmentBuild"),
        "notInstalled" => tr!("settings.appUpdate.unavailable.notInstalled"),
        "installRecordMissing" => tr!("settings.appUpdate.unavailable.installRecordMissing"),
        "unsupportedPlatform" => tr!("settings.appUpdate.unavailable.unsupportedPlatform"),
        other => other.to_string(),
    }
}

/// Outcome line of a failed operation.
/// 失败操作的结果说明。
fn failure_label(operation: &str) -> String {
    match operation {
        "check" => tr!("settings.appUpdate.failed.check"),
        "download" => tr!("settings.appUpdate.failed.download"),
        "repair" => tr!("settings.appUpdate.failed.repair"),
        "restart" => tr!("settings.appUpdate.failed.restart"),
        other => other.to_string(),
    }
}

/// Release notes in the interface language, then English, then any language.
/// 依次选择界面语言、英语、任一语言的发布说明。
fn release_notes(notes: &Value) -> String {
    let Some(notes) = notes.as_object() else {
        return String::new();
    };
    [crate::i18n::active_locale(), "en-US"]
        .iter()
        .find_map(|locale| notes.get(*locale))
        .or_else(|| notes.values().next())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Builds the offer rows: version, size, release notes and the download or download-page action.
/// 构建更新提议行：版本、大小、发布说明，以及下载或下载页操作。
fn offer_rows(rows: &mut Vec<Row>, snapshot: &Value, offer: &Value, busy: bool) {
    let version = s(offer, "/version");
    let size = format_bytes(n(offer, "/downloadBytes") as f64);
    let detail = if s(offer, "/kind") == "incremental" {
        tr!("settings.appUpdate.incremental", size = size)
    } else if b(offer, "/installable") {
        tr!("settings.appUpdate.full", size = size)
    } else if b(offer, "/setupManaged") {
        tr!("settings.appUpdate.fullSetup")
    } else {
        tr!("settings.appUpdate.fullManual")
    };
    // A setup-managed installation is updated by the setup program, so it gets no action here.
    // 由安装器管理的安装由安装器更新，因此此处不提供操作。
    let actions = if b(offer, "/installable") {
        vec![RowAction {
            label: tr!("settings.appUpdate.download"),
            action: (!busy).then(|| call(api_update::download_app_update())),
            tone: Tone::Accent,
            switch: None,
        }]
    } else if b(offer, "/setupManaged") {
        Vec::new()
    } else {
        vec![action(
            tr!("settings.appUpdate.openDownloadPage"),
            call(api::project_actions::open_external_url(&s(
                snapshot,
                "/downloadPageUrl",
            ))),
            Tone::Normal,
        )]
    };
    let mut heading = item(
        "app-update-offer",
        tr!("settings.appUpdate.available", version = version),
        detail,
    )
    .actions(actions);
    if s(offer, "/channel") == "preview" {
        heading = heading.badge(tr!("settings.appUpdate.preview"), Tone::Warn);
    }
    rows.push(heading);
    if b(offer, "/previouslyFailed") {
        rows.push(note(tr!("settings.appUpdate.previouslyFailed"), Tone::Bad));
    }
    // Skipped releases follow the offered one, newest first; once there are any, each version's notes get
    // a title so the versions stay apart.
    // 被跳过的版本按从新到旧排在所提供版本之后；一旦存在，每个版本的说明都带标题以便区分。
    let skipped = v(offer, "/skippedReleases")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let titled = !skipped.is_empty();
    release_notes_rows(rows, titled.then_some(version.as_str()), v(offer, "/notes"));
    for release in &skipped {
        release_notes_rows(rows, Some(&s(release, "/version")), v(release, "/notes"));
    }
    if !b(offer, "/skippedReleasesComplete") {
        rows.push(note(
            tr!(
                "appUpdate.dialog.earlierNotesMissing",
                current = s(snapshot, "/currentVersion")
            ),
            Tone::Muted,
        ));
    }
}

/// Appends one release's notes, under a title naming its version when `version` is given.
/// 追加一个发布的说明；给出 `version` 时在其上加注明版本的标题。
fn release_notes_rows(rows: &mut Vec<Row>, version: Option<&str>, notes: &Value) {
    let notes = release_notes(notes);
    if notes.is_empty() {
        return;
    }
    if let Some(version) = version {
        rows.push(note(
            tr!("appUpdate.dialog.notesTitle", version = version),
            Tone::Normal,
        ));
    }
    rows.push(Row::new(RowKind::Markdown { content: notes }, ""));
}

/// Builds the rows of the AppArmor profile the Linux desktop app still needs: the explanation,
/// the install action through the system password prompt and the terminal command.
/// 构建 Linux 桌面应用仍需要的 AppArmor 配置的行：说明、经系统密码框安装的操作以及终端命令。
fn desktop_profile_rows(rows: &mut Vec<Row>, profile: &Value) {
    rows.push(note(
        if s(profile, "/status") == "outdated" {
            tr!("settings.appUpdate.desktopProfile.outdatedTitle")
        } else {
            tr!("settings.appUpdate.desktopProfile.title")
        },
        Tone::Warn,
    ));
    rows.push(note(
        tr!("settings.appUpdate.desktopProfile.body"),
        Tone::Muted,
    ));
    rows.push(if b(profile, "/installing") {
        note(
            tr!("settings.appUpdate.desktopProfile.installing"),
            Tone::Muted,
        )
    } else {
        button(
            tr!("settings.appUpdate.desktopProfile.install"),
            call(api_update::install_desktop_profile()),
            Tone::Accent,
        )
    });
    rows.push(info(
        tr!("settings.appUpdate.desktopProfile.manual"),
        s(profile, "/command"),
        Tone::Normal,
    ));
    let error = s(profile, "/error");
    if !error.is_empty() {
        rows.push(info(
            tr!("settings.appUpdate.desktopProfile.failed"),
            error,
            Tone::Bad,
        ));
    }
}

/// Builds the settings and repair rows of an installation that can update itself.
/// 构建可自行更新的安装的设置与修复行。
fn settings_rows(rows: &mut Vec<Row>, snapshot: &Value, busy: bool) {
    let settings = v(snapshot, "/settings");
    rows.push(
        choice(
            tr!("settings.appUpdate.checkInterval"),
            vec![
                (
                    "oneHour".into(),
                    tr!("settings.appUpdate.checkInterval.oneHour"),
                ),
                (
                    "sixHours".into(),
                    tr!("settings.appUpdate.checkInterval.sixHours"),
                ),
                (
                    "twelveHours".into(),
                    tr!("settings.appUpdate.checkInterval.twelveHours"),
                ),
                (
                    "oneDay".into(),
                    tr!("settings.appUpdate.checkInterval.oneDay"),
                ),
            ],
            s(settings, "/checkInterval"),
            dto(
                settings.clone(),
                "/checkInterval",
                api_update::save_app_update_settings,
            ),
        )
        .detail(tr!("settings.appUpdate.checkIntervalHint")),
    );
    rows.push(
        toggle(
            tr!("settings.appUpdate.previewChannel"),
            b(settings, "/preview"),
            dto(
                settings.clone(),
                "/preview",
                api_update::save_app_update_settings,
            ),
        )
        .detail(tr!("settings.appUpdate.previewChannelHint")),
    );
    rows.push(
        choice(
            tr!("settings.appUpdate.route"),
            vec![
                ("auto".into(), tr!("settings.appUpdate.route.auto")),
                (
                    "international".into(),
                    tr!("settings.appUpdate.route.international"),
                ),
                ("china".into(), tr!("settings.appUpdate.route.china")),
            ],
            s(settings, "/route"),
            dto(
                settings.clone(),
                "/route",
                api_update::save_app_update_settings,
            ),
        )
        .detail(tr!("settings.appUpdate.routeHint")),
    );
    rows.push(section(tr!("settings.appUpdate.repair")));
    let repaired = v(snapshot, "/repairedFiles");
    rows.push(note(
        if repaired.is_null() {
            tr!("settings.appUpdate.repairHint")
        } else {
            tr!(
                "settings.appUpdate.repaired",
                count = n(snapshot, "/repairedFiles")
            )
        },
        Tone::Muted,
    ));
    if !busy {
        rows.push(button(
            tr!("settings.appUpdate.repairAction"),
            call(api_update::repair_app_install()),
            Tone::Normal,
        ));
    }
}

/// Builds the application update page.
/// 构建应用更新页。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let snapshot = app.slots.value(Slot::AppUpdate);
    if snapshot.is_null() {
        return vec![note(tr!("settings.appUpdate.loading"), Tone::Muted)];
    }
    let activity = s(snapshot, "/activity");
    let busy = activity != "idle";
    let unavailable = s(snapshot, "/unavailableReason");
    let mut rows = Vec::new();

    let status = if unavailable.is_empty() {
        let checked = v(snapshot, "/lastCheckedAt");
        let time = checked
            .as_i64()
            .map(utc_time)
            .unwrap_or_else(|| tr!("settings.appUpdate.neverChecked"));
        tr!("settings.appUpdate.lastChecked", time = time)
    } else {
        unavailable_label(&unavailable)
    };
    rows.push(info(
        tr!("settings.appUpdate.currentVersion"),
        s(snapshot, "/currentVersion"),
        Tone::Normal,
    ));
    rows.push(note(status, Tone::Muted));
    if unavailable.is_empty() {
        rows.push(if busy {
            note(
                if activity == "checking" {
                    tr!("settings.appUpdate.checking")
                } else {
                    tr!("settings.appUpdate.check")
                },
                Tone::Muted,
            )
        } else {
            button(
                tr!("settings.appUpdate.check"),
                call(api_update::check_app_update()),
                Tone::Accent,
            )
        });
    }

    let rollback = v(snapshot, "/rollback");
    if !rollback.is_null() {
        rows.push(note(
            tr!(
                "settings.appUpdate.rollback.title",
                version = s(rollback, "/failed")
            ),
            Tone::Warn,
        ));
        rows.push(note(
            tr!(
                "settings.appUpdate.rollback.body",
                restored = s(rollback, "/restored")
            ),
            Tone::Muted,
        ));
        rows.push(button(
            tr!("settings.appUpdate.rollback.dismiss"),
            call(api_update::dismiss_app_update_notice()),
            Tone::Normal,
        ));
    }

    let desktop_profile = v(snapshot, "/desktopProfile");
    if !desktop_profile.is_null() {
        desktop_profile_rows(&mut rows, desktop_profile);
    }

    let error = v(snapshot, "/error");
    if !error.is_null() {
        rows.push(info(
            failure_label(&s(error, "/operation")),
            s(error, "/message"),
            Tone::Bad,
        ));
    }

    let ready = s(snapshot, "/readyVersion");
    if !ready.is_empty() {
        rows.push(
            item(
                "app-update-ready",
                tr!("settings.appUpdate.ready", version = ready),
                tr!("settings.appUpdate.readyHint"),
            )
            .actions(vec![RowAction {
                label: if activity == "restarting" {
                    tr!("settings.appUpdate.restarting")
                } else {
                    tr!("settings.appUpdate.restart")
                },
                // Running background commands stop with the restart, so they are counted first.
                // 运行中的后台命令会随重启停止，因此先统计其数量。
                action: (!busy).then_some(UiAction::RestartToUpdate),
                tone: Tone::Accent,
                switch: None,
            }]),
        );
    }

    let offer = v(snapshot, "/offer");
    if !offer.is_null() {
        offer_rows(&mut rows, snapshot, offer, busy);
    }

    let progress = v(snapshot, "/progress");
    if !progress.is_null() {
        let received = n(progress, "/receivedBytes");
        let total = n(progress, "/totalBytes");
        rows.push(meter(
            if activity == "repairing" {
                tr!("settings.appUpdate.repairing")
            } else {
                tr!("settings.appUpdate.downloading")
            },
            received as f64 / total.max(1) as f64,
            format!(
                "{} / {}",
                format_bytes(received as f64),
                format_bytes(total as f64)
            ),
            Tone::Info,
        ));
        rows.push(button(
            tr!("settings.appUpdate.cancel"),
            call(api_update::cancel_app_update()),
            Tone::Normal,
        ));
    }

    if unavailable.is_empty() {
        settings_rows(&mut rows, snapshot, busy);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BackendHandle;
    use crate::ui::theme::Theme;
    use std::sync::mpsc;

    /// Builds an idle installed snapshot with the given overrides.
    /// 以给定覆盖值构建一个空闲的已安装快照。
    fn snapshot(overrides: Value) -> Value {
        let mut base = json!({
            "currentVersion": "0.1.4", "unavailableReason": null,
            "settings": { "preview": false, "route": "auto", "checkInterval": "sixHours" }, "activity": "idle",
            "progress": null, "lastCheckedAt": null, "offer": null, "readyVersion": null,
            "repairedFiles": null, "error": null, "rollback": null, "desktopProfile": null,
            "downloadPageUrl": "https://openvulcan.com/download"
        });
        for (key, value) in overrides.as_object().unwrap() {
            base[key] = value.clone();
        }
        base
    }

    /// Methods of every button and row action on the page.
    /// 页面上每个按钮与行操作的方法。
    fn methods(rows: &[Row]) -> Vec<String> {
        let mut found = Vec::new();
        let mut push = |action: &UiAction| {
            if let UiAction::Send(Command::Call(request)) = action {
                found.push(request.method.to_string());
            }
        };
        for row in rows {
            match &row.kind {
                RowKind::Button { action, .. } => push(action),
                RowKind::Item { actions, .. } => {
                    for row_action in actions {
                        if let Some(action) = &row_action.action {
                            push(action);
                        }
                    }
                }
                _ => {}
            }
        }
        found
    }

    /// Renders the page rows of one snapshot.
    /// 渲染一个快照的页面行。
    fn page(value: Value) -> Vec<Row> {
        let (commands, _receiver) = mpsc::channel();
        let (_, events) = mpsc::channel();
        let mut app = App::new(
            BackendHandle {
                commands,
                events,
                label: "test".into(),
                is_mock: true,
            },
            Theme::dark(),
            true,
        );
        app.slots
            .set(Slot::AppUpdate, api_update::load_app_update_status(), value);
        rows(&app)
    }

    // A copy that cannot update itself offers no update actions.
    // 无法自行更新的副本不提供任何更新操作。
    #[test]
    fn unavailable_copies_offer_no_actions() {
        let rows = page(snapshot(json!({ "unavailableReason": "notInstalled" })));
        assert!(methods(&rows).is_empty());
    }

    // An installable offer downloads; a manual full update opens the download page instead.
    // 可安装的提议执行下载；需手动的完整更新改为打开下载页。
    #[test]
    fn offers_download_or_point_to_the_download_page() {
        let offer = |kind: &str, installable: bool| {
            json!({ "offer": { "version": "0.1.5", "channel": "stable", "kind": kind,
                "downloadBytes": 1024, "releasedAt": "2026-11-04T00:00:00Z", "notes": {},
                "installable": installable, "previouslyFailed": false } })
        };
        let incremental = methods(&page(snapshot(offer("incremental", true))));
        assert!(incremental.contains(&"service_download_app_update".to_string()));
        let manual = methods(&page(snapshot(offer("full", false))));
        assert!(!manual.contains(&"service_download_app_update".to_string()));
        assert!(manual.contains(&"service_open_external_url".to_string()));
        let mut managed = offer("full", false);
        managed["offer"]["setupManaged"] = json!(true);
        let managed = methods(&page(snapshot(managed)));
        assert!(!managed.contains(&"service_download_app_update".to_string()));
        assert!(!managed.contains(&"service_open_external_url".to_string()));
    }

    // Skipped releases follow the offered version's notes, newest first, each under its own title.
    // 被跳过的版本按从新到旧排在所提供版本的说明之后，各带标题。
    #[test]
    fn skipped_releases_follow_the_offered_notes_in_order() {
        let rows = page(snapshot(json!({ "offer": {
            "version": "0.1.7", "channel": "stable", "kind": "incremental", "downloadBytes": 1024,
            "releasedAt": "2026-11-04T00:00:00Z", "notes": { "en-US": "notes-017" },
            "skippedReleases": [
                { "version": "0.1.6", "date": null, "notes": { "en-US": "notes-016" } },
                { "version": "0.1.5", "date": null, "notes": { "en-US": "notes-015" } }
            ],
            "skippedReleasesComplete": true, "installable": true, "previouslyFailed": false
        } })));
        let markdown: Vec<&str> = rows
            .iter()
            .filter_map(|row| match &row.kind {
                RowKind::Markdown { content } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(markdown, ["notes-017", "notes-016", "notes-015"]);
    }

    // A missing desktop profile offers its install until the password prompt is open.
    // 缺少桌面配置时提供安装，直到密码框打开。
    #[test]
    fn a_missing_desktop_profile_offers_its_install_until_the_prompt_opens() {
        let profile = |installing: bool| {
            json!({ "desktopProfile": { "status": "missing", "installing": installing,
                "command": "sudo install -m 0644 a b", "error": null } })
        };
        let install = "service_install_desktop_profile".to_string();
        assert!(!methods(&page(snapshot(json!({})))).contains(&install));
        assert!(methods(&page(snapshot(profile(false)))).contains(&install));
        assert!(!methods(&page(snapshot(profile(true)))).contains(&install));
    }

    // While an operation runs only cancel stays available.
    // 操作进行中只保留取消。
    #[test]
    fn busy_updater_only_offers_cancel() {
        let rows = page(snapshot(json!({
            "activity": "downloading",
            "progress": { "receivedBytes": 512, "totalBytes": 1024 }
        })));
        let methods = methods(&rows);
        assert!(methods.contains(&"service_cancel_app_update".to_string()));
        for idle_only in ["service_check_app_update", "service_repair_app_install"] {
            assert!(!methods.contains(&idle_only.to_string()), "{idle_only}");
        }
    }
}
