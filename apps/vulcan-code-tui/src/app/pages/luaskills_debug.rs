// Debug Lua skills page: one-click luaskills-debug install, its paths and the runtime setup command,
// mirroring the WebUI page under LuaSkills. The install is a Core-owned task; the status carries it
// and is re-read while it runs.
// 调试 Lua 技能页：一键安装 luaskills-debug、其路径与运行时设置命令，镜像 LuaSkills 下的 WebUI 页面。
// 安装是 Core 自有任务；状态携带该任务，并在其运行期间重读。

use super::permission_judge::format_bytes;
use super::*;
use crate::service_api::skills_actions as api_skills;

/// Catalog label of one installer phase; unknown phases show their key.
/// 单个安装阶段的目录标签；未知阶段显示其键。
fn phase_label(phase: &str) -> String {
    match phase {
        "queued" => tr!("skills.luaDebug.phase.queued"),
        "resolving" => tr!("skills.luaDebug.phase.resolving"),
        "downloading" => tr!("skills.luaDebug.phase.downloading"),
        "verifying" => tr!("skills.luaDebug.phase.verifying"),
        "extracting" => tr!("skills.luaDebug.phase.extracting"),
        "completed" => tr!("skills.luaDebug.phase.completed"),
        other => other.to_string(),
    }
}

/// The one-time Lua runtime setup command of the installed package.
/// 已安装包的一次性 Lua 运行时设置命令。
fn runtime_setup_command(status: &Value) -> String {
    let root = s(status, "/packageRoot");
    if s(status, "/platform").starts_with("windows") {
        format!(
            "powershell -NoProfile -ExecutionPolicy Bypass -File \"{root}\\setup_runtime.ps1\" -Target lua"
        )
    } else {
        format!("bash \"{root}/setup_runtime.sh\" lua")
    }
}

/// Builds the page rows from the `LuaSkillsDebugStatus` slot.
/// 由 `LuaSkillsDebugStatus` 槽位构建页面行。
pub(super) fn rows(app: &App) -> Vec<Row> {
    let status = app.slots.value(Slot::LuaSkillsDebugStatus);
    if status.is_null() {
        return Vec::new();
    }
    let missing = tr!("skills.luaDebug.notInstalled");
    let supported = !v(status, "/platform").is_null();
    let installed = !v(status, "/version").is_null();
    let compatible = b(status, "/compatible");
    let task = v(status, "/task");
    let running = s(task, "/state") == "running";
    let mut rows = vec![
        note(tr!("skills.luaDebug.toolDescription"), Tone::Muted),
        section("luaskills-debug".into()),
    ];
    let (state, tone) = if !supported {
        (tr!("skills.luaDebug.state.unsupported"), Tone::Bad)
    } else if !installed {
        (tr!("skills.luaDebug.state.missing"), Tone::Muted)
    } else if compatible {
        (tr!("skills.luaDebug.state.ready"), Tone::Good)
    } else {
        (tr!("skills.luaDebug.state.mismatch"), Tone::Warn)
    };
    rows.push(info(tr!("appChrome.status"), state, tone));
    if running {
        let progress = match (
            v(task, "/completedBytes").as_f64(),
            v(task, "/totalBytes").as_f64(),
        ) {
            (Some(done), Some(total)) => {
                format!("{} / {}", format_bytes(done), format_bytes(total))
            }
            (Some(done), None) => format_bytes(done),
            _ => String::new(),
        };
        rows.push(meter(
            phase_label(&s(task, "/phase")),
            n(task, "/overallPercent") as f64 / 100.0,
            progress,
            Tone::Info,
        ));
    } else if supported {
        let label = if installed && compatible {
            tr!("skills.luaDebug.reinstall")
        } else {
            tr!("skills.luaDebug.install")
        };
        rows.push(button(
            label,
            call(api_skills::start_luaskills_debug_install()),
            Tone::Accent,
        ));
    }
    if s(task, "/state") == "failed" {
        rows.push(note(
            format!(
                "{} {}",
                tr!("skills.luaDebug.installFailed"),
                s(task, "/error")
            ),
            Tone::Bad,
        ));
    }
    if !supported {
        rows.push(note(tr!("skills.luaDebug.unsupportedHelp"), Tone::Warn));
    }
    if installed && !compatible {
        rows.push(note(
            tr!(
                "skills.luaDebug.mismatchHelp",
                version = s(status, "/version"),
                series = s(status, "/series"),
            ),
            Tone::Warn,
        ));
    }
    let or_missing = |pointer: &str| {
        let value = s(status, pointer);
        if value.is_empty() {
            missing.clone()
        } else {
            value
        }
    };
    rows.push(info(
        tr!("skills.luaDebug.series"),
        format!("{}.x", s(status, "/series")),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("skills.luaDebug.version"),
        or_missing("/version"),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("skills.luaDebug.platform"),
        if supported {
            s(status, "/platform")
        } else {
            tr!("skills.luaDebug.state.unsupported")
        },
        Tone::Normal,
    ));
    rows.push(info(
        tr!("skills.luaDebug.executable"),
        or_missing("/executable"),
        Tone::Normal,
    ));
    rows.push(info(
        tr!("skills.luaDebug.runtimeRoot"),
        or_missing("/runtimeRoot"),
        Tone::Normal,
    ));
    let runtime_ready = b(status, "/runtimeReady");
    rows.push(info(
        tr!("skills.luaDebug.runtimeReady"),
        if !installed {
            missing.clone()
        } else if runtime_ready {
            tr!("skills.luaDebug.runtimeReady.yes")
        } else {
            tr!("skills.luaDebug.runtimeReady.no")
        },
        if installed && !runtime_ready {
            Tone::Warn
        } else {
            Tone::Normal
        },
    ));
    rows.push(info(
        tr!("skills.luaDebug.installRoot"),
        or_missing("/installRoot"),
        Tone::Normal,
    ));
    if installed && !runtime_ready {
        rows.push(section(tr!("skills.luaDebug.runtimeSetup")));
        rows.push(note(tr!("skills.luaDebug.runtimeSetupHelp"), Tone::Muted));
        rows.push(note(runtime_setup_command(status), Tone::Normal));
    }
    rows.push(section(tr!("skills.luaDebug.aiTitle")));
    rows.push(note(tr!("skills.luaDebug.aiDescription"), Tone::Muted));
    rows.push(note(tr!("skills.luaDebug.aiHint"), Tone::Muted));
    rows
}
