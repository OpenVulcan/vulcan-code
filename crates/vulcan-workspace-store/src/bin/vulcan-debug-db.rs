//! Explicit pre-release maintenance for the current user's Debug application database.
//! 显式维护当前用户的预发布 Debug 应用数据库。

#[cfg(debug_assertions)]
mod debug_database;

/// Run the independent maintenance command; returns failure without exposing stored values.
/// 运行独立维护命令；失败时不泄漏已存储的值。
/// Accepts only `--check` or `--apply`; the database path comes from the application's Debug root.
/// 仅接受 `--check` 或 `--apply`；数据库路径由应用 Debug 根目录派生。
fn main() -> std::process::ExitCode {
    #[cfg(debug_assertions)]
    {
        match debug_database::run() {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Debug database maintenance failed: {error}");
                std::process::ExitCode::FAILURE
            }
        }
    }
    #[cfg(not(debug_assertions))]
    {
        eprintln!("Database maintenance is available only in Debug builds.");
        std::process::ExitCode::FAILURE
    }
}
