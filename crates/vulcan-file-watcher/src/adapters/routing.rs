use std::path::{Path, PathBuf};

use crate::event::FileWatchEvent;

/// One reusable file-watch route.
/// 一个可复用文件监听路由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWatchRoute {
    /// Consumer or subsystem identifier.
    /// 消费者或子系统标识。
    pub id: String,
    /// Root path matched by this route.
    /// 该路由匹配的根路径。
    pub root: PathBuf,
    /// Whether descendant paths should match.
    /// 是否匹配后代路径。
    pub recursive: bool,
}

impl FileWatchRoute {
    /// Return whether this route matches the provided path.
    /// 返回该路由是否匹配给定路径。
    pub fn matches(&self, path: &Path) -> bool {
        if self.recursive {
            return path.starts_with(&self.root);
        }
        path == self.root
    }
}

/// Routed file-watch event.
/// 已路由的文件监听事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedFileWatchEvent {
    /// Matched route id.
    /// 匹配到的路由标识。
    pub route_id: String,
    /// Original file-watch event.
    /// 原始文件监听事件。
    pub event: FileWatchEvent,
}

/// Reusable path-based event router.
/// 可复用的路径事件路由器。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileWatchRouter {
    /// Registered routes.
    /// 已注册路由。
    pub routes: Vec<FileWatchRoute>,
}

impl FileWatchRouter {
    /// Register one route.
    /// 注册一个路由。
    pub fn add_route(&mut self, route: FileWatchRoute) {
        self.routes.push(route);
    }

    /// Route one event to every matching consumer.
    /// 将一个事件路由给全部匹配消费者。
    pub fn route_event(&self, event: &FileWatchEvent) -> Vec<RoutedFileWatchEvent> {
        self.routes
            .iter()
            .filter(|route| route.matches(&event.path))
            .map(|route| RoutedFileWatchEvent {
                route_id: route.id.clone(),
                event: event.clone(),
            })
            .collect()
    }
}
