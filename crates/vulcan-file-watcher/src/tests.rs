use std::path::PathBuf;

use crate::adapters::project_rules::{
    DEFAULT_PROJECT_RULES_FILE_NAME, ProjectRulesWatchTarget, default_project_rules_route,
};
use crate::adapters::routing::{FileWatchRoute, FileWatchRouter};
use crate::event::{FileWatchEvent, FileWatchEventKind};
use crate::fs::polling::PollingFileWatcher;
use crate::traits::{FileWatchTarget, FileWatcher};
use crate::watcher::NoopFileWatcher;

/// Verifies the noop watcher stores targets and drains events.
/// 验证空操作监听器会保存目标并取出事件。
#[test]
fn noop_file_watcher_registers_and_drains() {
    let mut watcher = NoopFileWatcher::default();
    watcher
        .watch(FileWatchTarget {
            path: PathBuf::from("D:/repo/AGENTS.md"),
            recursive: false,
        })
        .expect("watch should succeed");
    assert_eq!(watcher.target_count(), 1);

    watcher.push_event(FileWatchEvent {
        kind: FileWatchEventKind::Modified,
        path: PathBuf::from("D:/repo/AGENTS.md"),
        previous_path: None,
    });

    let events = watcher.drain_events();
    assert_eq!(events.len(), 1);
    assert!(watcher.drain_events().is_empty());
}

/// Verifies the polling watcher reports newly created watched descendants.
/// 验证轮询监听器会报告新创建的被监听后代路径。
#[test]
fn polling_file_watcher_reports_created_file() {
    let root = unique_temp_dir("vulcan-file-watcher-polling-test");
    std::fs::create_dir_all(&root).expect("create root dir");

    let mut watcher = PollingFileWatcher::new();
    watcher
        .watch(FileWatchTarget {
            path: root.clone(),
            recursive: true,
        })
        .expect("watch should succeed");
    assert_eq!(watcher.target_count(), 1);

    let created_file = root.join("AGENTS.md");
    std::fs::write(&created_file, "rules").expect("write watched file");
    watcher.poll().expect("poll should succeed");

    let events = watcher.drain_events();
    assert!(
        events
            .iter()
            .any(|event| event.kind == FileWatchEventKind::Created && event.path == created_file)
    );

    let _ = std::fs::remove_dir_all(root);
}

/// Verifies the generic router matches recursive path routes.
/// 验证通用路由器会匹配递归路径路由。
#[test]
fn file_watch_router_routes_recursive_events() {
    let mut router = FileWatchRouter::default();
    router.add_route(FileWatchRoute {
        id: String::from("project-rules"),
        root: PathBuf::from("D:/repo"),
        recursive: true,
    });
    let routed = router.route_event(&FileWatchEvent {
        kind: FileWatchEventKind::Modified,
        path: PathBuf::from("D:/repo/AGENTS.md"),
        previous_path: None,
    });
    assert_eq!(routed.len(), 1);
    assert_eq!(routed[0].route_id, "project-rules");
}

/// Verifies project-rules watch targets resolve the workspace AGENTS.md path.
/// 验证项目规则监听目标会解析工作区 AGENTS.md 路径。
#[test]
fn project_rules_watch_target_resolves_agents_path() {
    let target = ProjectRulesWatchTarget::workspace_default(PathBuf::from("D:/repo"));

    assert_eq!(
        target.rules_path(),
        PathBuf::from("D:/repo").join(DEFAULT_PROJECT_RULES_FILE_NAME)
    );
    assert!(!target.to_file_watch_target().recursive);
    assert!(!target.to_live_watch_path().recursive);
}

/// Verifies project-rules watch targets match direct and previous rename paths.
/// 验证项目规则监听目标会匹配直接路径与重命名前路径。
#[test]
fn project_rules_watch_target_matches_events() {
    let target = ProjectRulesWatchTarget::workspace_default(PathBuf::from("D:/repo"));
    let rules_path = PathBuf::from("D:/repo").join(DEFAULT_PROJECT_RULES_FILE_NAME);

    assert!(target.matches_event(&FileWatchEvent {
        kind: FileWatchEventKind::Modified,
        path: rules_path.clone(),
        previous_path: None,
    }));
    assert!(target.matches_event(&FileWatchEvent {
        kind: FileWatchEventKind::Renamed,
        path: PathBuf::from("D:/repo/AGENTS.old.md"),
        previous_path: Some(rules_path),
    }));
}

/// Verifies default project-rules routes match the concrete rules file only.
/// 验证默认项目规则路由只匹配具体规则文件。
#[test]
fn default_project_rules_route_matches_only_agents_file() {
    let route = default_project_rules_route(PathBuf::from("D:/repo"));
    let router = FileWatchRouter {
        routes: vec![route],
    };

    let matched = router.route_event(&FileWatchEvent {
        kind: FileWatchEventKind::Modified,
        path: PathBuf::from("D:/repo/AGENTS.md"),
        previous_path: None,
    });
    let unmatched = router.route_event(&FileWatchEvent {
        kind: FileWatchEventKind::Modified,
        path: PathBuf::from("D:/repo/src/main.rs"),
        previous_path: None,
    });

    assert_eq!(matched.len(), 1);
    assert!(matched[0].route_id.starts_with("project-rules:"));
    assert!(unmatched.is_empty());
}

/// Build one unique temporary directory for tests.
/// 为测试构建一个唯一临时目录。
fn unique_temp_dir(prefix: &str) -> PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{suffix}"))
}
