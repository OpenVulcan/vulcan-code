//! Isolated worker process that runs the local permission-judge model.
//! 运行本机权限判定模型的隔离 worker 进程。
//!
//! The service starts one worker per loaded model and speaks the line protocol of
//! `vulcan_permission_judge::protocol` over stdin and stdout. A native crash or GPU fault therefore
//! ends only this process, and the service treats it as a failed judgement.
//! 服务为每个已加载模型启动一个 worker，并通过 stdin 与 stdout 使用
//! `vulcan_permission_judge::protocol` 的行协议。原生崩溃或 GPU 故障因此只会结束本进程，服务将其
//! 视为判定失败。

mod engine;
mod gpu;
mod sequence;
mod status;
mod worker;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut worker = worker::Worker::default();
    match worker::serve(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        &mut worker,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
