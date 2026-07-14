//! Manifest-driven bundled System Plugin catalog and managed runtime host.
//! 清单驱动的内建系统插件目录与受管运行时宿主。

mod atomic_file;
mod catalog;
mod host_services;
mod manifest;
mod runtime;

pub use catalog::*;
pub use host_services::*;
pub use manifest::*;
pub use runtime::*;
