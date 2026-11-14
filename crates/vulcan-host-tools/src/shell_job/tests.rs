use std::path::Path;
use std::time::Duration;

use regex::Regex;
use tokio::process::Command as TokioCommand;

use super::index::{ShellJobIndexLine, append_index_line};
use super::log::{LogLimits, ShellJobLogWriter};
use super::runtime::valid_job_id;
use super::*;
use crate::process_creation::new_background_tokio_command;

/// Writes `text` into a fresh log with `limits` and returns its reader.
/// 以 `limits` 把 `text` 写入新日志并返回其读取器。
fn log_with(dir: &Path, limits: LogLimits, chunks: &[&str]) -> ShellJobLogReader {
    let mut writer = ShellJobLogWriter::create_with_limits(dir, limits).expect("create log");
    for chunk in chunks {
        writer.append(chunk).expect("append log");
    }
    writer.reader()
}

/// Numbered lines `line 1` … `line n`, each ended by a line break.
/// 编号行 `line 1` … `line n`，每行以换行结尾。
fn numbered_lines(count: u64) -> String {
    (1..=count).map(|line| format!("line {line}\n")).collect()
}

/// Output shorter than the budget returns in full and is not marked truncated.
/// 短于预算的输出完整返回，且不标记为截断。
#[test]
fn short_output_tail_is_complete() {
    let temp = tempfile::tempdir().expect("temp dir");
    let reader = log_with(temp.path(), LogLimits::PRODUCT, &["alpha\nbeta\n"]);
    let tail = reader.tail(SHELL_OUTPUT_TAIL_BUDGET_BYTES).expect("tail");
    assert!(!tail.truncated);
    assert_eq!(tail.total_lines, 2);
    assert_eq!(render_plain_rows(&tail.rows), "alpha\nbeta");
    assert_eq!(tail.rows[0].line, 1);
}

/// A long output returns only whole newest lines within the budget, numbered by their real position.
/// 长输出只返回预算内完整的最新行，并按真实位置编号。
#[test]
fn long_output_tail_keeps_newest_whole_lines_within_budget() {
    let temp = tempfile::tempdir().expect("temp dir");
    let reader = log_with(temp.path(), LogLimits::PRODUCT, &[&numbered_lines(10_000)]);
    let tail = reader.tail(1_000).expect("tail");
    assert!(tail.truncated);
    assert_eq!(tail.total_lines, 10_000);
    let rendered = render_plain_rows(&tail.rows);
    assert!(rendered.len() < 1_000, "tail must stay within the budget");
    assert!(rendered.ends_with("line 10000"));
    let first = tail.rows.first().expect("rows");
    assert_eq!(
        rendered.lines().next(),
        Some(format!("line {}", first.line).as_str())
    );
    assert!(
        !first.continuation,
        "the tail must start at a line boundary"
    );
}

/// An over-long line is split into rows no wider than the wrap width; a cut line start is marked.
/// 超长行切分为不超过换行宽度的显示行；被截断的行首会被标记。
#[test]
fn long_lines_are_split_into_rows() {
    let temp = tempfile::tempdir().expect("temp dir");
    let long = "界".repeat(SHELL_OUTPUT_LINE_WRAP_BYTES);
    let reader = log_with(
        temp.path(),
        LogLimits::PRODUCT,
        &[&format!("{long}\nend\n")],
    );
    let range = reader.read_lines(1, 10, usize::MAX).expect("read");
    let first_line_rows: Vec<_> = range.rows.iter().filter(|row| row.line == 1).collect();
    assert!(first_line_rows.len() > 1);
    assert!(
        first_line_rows
            .iter()
            .all(|row| row.text.len() <= SHELL_OUTPUT_LINE_WRAP_BYTES)
    );
    assert!(first_line_rows[1..].iter().all(|row| row.continuation));
    assert_eq!(
        first_line_rows
            .iter()
            .map(|row| row.text.as_str())
            .collect::<String>(),
        long
    );

    let tail = reader.tail(100).expect("tail");
    assert!(tail.truncated);
    assert_eq!(render_plain_rows(&tail.rows).lines().last(), Some("end"));
}

/// Reading a line range returns the requested lines and where to continue.
/// 读取行范围返回所请求的行以及继续读取的位置。
#[test]
fn line_ranges_page_through_the_output() {
    let temp = tempfile::tempdir().expect("temp dir");
    let reader = log_with(
        temp.path(),
        LogLimits::PRODUCT,
        &["a\nb\n", "c\nd", "\ne\n"],
    );
    let first = reader.read_lines(2, 2, usize::MAX).expect("read");
    assert_eq!(render_numbered_rows(&first.rows), "2|b\n3|c");
    assert_eq!(first.next_line, Some(4));
    let rest = reader.read_lines(4, 10, usize::MAX).expect("read");
    assert_eq!(render_numbered_rows(&rest.rows), "4|d\n5|e");
    assert_eq!(rest.next_line, None);
    assert_eq!(rest.total_lines, 5);
}

/// Beyond the log limit the oldest body segments are dropped, the head stays, and a reopened log matches.
/// 超出日志上限后丢弃最早的正文分段，开头保留，重新打开的日志与之一致。
#[test]
fn dropped_body_segments_leave_the_head_and_one_gap() {
    let temp = tempfile::tempdir().expect("temp dir");
    let limits = LogLimits {
        head: 30,
        segment: 40,
        limit: 150,
    };
    let text = numbered_lines(200);
    let reader = log_with(temp.path(), limits, &[&text]);
    let stats = reader.stats();
    assert_eq!(stats.total_lines, 200);
    assert!(stats.dropped_bytes > 0);

    let range = reader.read_lines(1, 1_000, usize::MAX).expect("read");
    assert_eq!(range.rows[0].text, "line 1");
    let gaps: Vec<_> = range
        .rows
        .iter()
        .filter(|row| row.kind == ShellJobRowKind::Gap)
        .collect();
    assert_eq!(gaps.len(), 1, "exactly one gap follows the head");
    assert_eq!(
        range.rows.last().map(|row| row.text.as_str()),
        Some("line 200")
    );
    // Line numbers after the gap stay true to the output.
    // 缺口之后的行号仍与输出一致。
    for row in range
        .rows
        .iter()
        .filter(|row| row.kind == ShellJobRowKind::Text && !row.continuation)
    {
        if let Some(number) = row.text.strip_prefix("line ") {
            assert_eq!(number.parse::<u64>().ok(), Some(row.line));
        }
    }

    let reopened = ShellJobLogReader::open(temp.path()).expect("reopen");
    assert_eq!(reopened.stats(), stats);
    assert_eq!(
        reopened.tail(64).expect("tail"),
        reader.tail(64).expect("tail")
    );
}

/// Redacts both sides of a detected value even when the line read cap splits it.
/// 即使单行读取上限截断了已知值，也遮蔽其在可见范围内的所有片段。
#[test]
fn line_read_cap_keeps_redaction_lookahead() {
    let temporary = tempfile::tempdir().unwrap();
    let prefix = "x".repeat(super::log::LOG_LINE_READ_CAP_BYTES - "synthetic-".len());
    let text = format!("{prefix}synthetic-line-cap-secret\npublic tail\n");
    let reader = log_with(temporary.path(), LogLimits::PRODUCT, &[&text]);
    let redaction = ShellJobLogRedaction::new([(
        "synthetic-line-cap-secret".into(),
        "[redacted:FIXTURE]".into(),
    )]);
    let range = reader
        .read_lines_with_redaction(1, 2, usize::MAX, &redaction)
        .unwrap();
    let output = render_numbered_rows(&range.rows);
    assert!(!output.contains("synthetic-"));
    let first_line: String = range
        .rows
        .iter()
        .filter(|row| row.line == 1)
        .map(|row| row.text.as_str())
        .collect();
    assert!(first_line.contains("[redacted:FIXTURE]"));
    assert!(output.contains("public tail"));
    assert!(output.contains("more bytes of this line"));
    let search = reader
        .search_with_redaction(
            &Regex::new("synthetic-").unwrap(),
            0,
            1,
            usize::MAX,
            &redaction,
        )
        .unwrap();
    assert_eq!(search.match_count, 0);
}

/// Conceals the retained margins where the rolling log discarded the rest of a match.
/// 滚动日志丢弃匹配的其余部分时，遮蔽仍保留的缺口边缘。
#[test]
fn rolling_gap_conceals_retained_secret_fragments() {
    let temporary = tempfile::tempdir().unwrap();
    let limits = LogLimits {
        head: 32,
        segment: 64,
        limit: 96,
    };
    let head_fragment = "gap-prefix";
    let head = format!(
        "{}{head_fragment}",
        "h".repeat(limits.head as usize - head_fragment.len())
    );
    let middle = format!(
        "{}discarded-prefix",
        "x".repeat(limits.segment as usize - "discarded-prefix".len())
    );
    let tail = "retained-fragment\npublic line\n";
    let reader = log_with(temporary.path(), limits, &[&head, &middle, tail]);
    assert_eq!(reader.stats().dropped_bytes, limits.segment);
    let redaction = ShellJobLogRedaction::new([
        ("gap-prefixxxx".into(), "[redacted:A]".into()),
        (
            "discarded-prefixretained-fragment".into(),
            "[redacted:B]".into(),
        ),
    ]);
    let page = reader
        .read_lines_with_redaction(1, 10, usize::MAX, &redaction)
        .unwrap();
    let output = render_numbered_rows(&page.rows);
    assert!(!output.contains(head_fragment));
    assert!(!output.contains("retained-fragment"));
    assert!(page.rows.iter().any(|row| row.kind == ShellJobRowKind::Gap));
    let tail = reader.tail_with_redaction(usize::MAX, &redaction).unwrap();
    let output = render_numbered_rows(&tail.rows);
    assert!(!output.contains("retained-fragment"));
    assert!(output.contains("[redacted]"));
}

/// Search returns matches with context, separating non-adjacent groups.
/// 搜索返回带上下文的匹配，并分隔不相邻的结果组。
#[test]
fn search_returns_matches_with_context() {
    let temp = tempfile::tempdir().expect("temp dir");
    let reader = log_with(
        temp.path(),
        LogLimits::PRODUCT,
        &["ok 1\nok 2\nERROR a\nok 4\nok 5\nok 6\nok 7\nERROR b\n"],
    );
    let pattern = Regex::new("ERROR").expect("regex");
    let found = reader.search(&pattern, 1, 10, usize::MAX).expect("search");
    assert_eq!(found.match_count, 2);
    assert_eq!(
        render_numbered_rows(&found.rows),
        "2|ok 2\n3>|ERROR a\n4|ok 4\n--\n7|ok 7\n8>|ERROR b"
    );
    let limited = reader.search(&pattern, 0, 1, usize::MAX).expect("search");
    assert_eq!(limited.match_count, 1);
    assert!(limited.truncated);
}

/// The index merges events per job, refuses unknown versions and ignores a torn last line.
/// 索引按任务合并事件，拒绝未知版本，并忽略不完整的最后一行。
#[test]
fn index_merges_events_and_refuses_unknown_versions() {
    let temp = tempfile::tempdir().expect("temp dir");
    let started = ShellJobIndexLine::Started {
        version: ShellJobIndexLine::current_version(),
        job_id: String::from("job_000000000001"),
        command: String::from("make"),
        purpose: String::from("build"),
        shell: String::from("sh"),
        cwd: String::from("/w"),
        started_at_ms: 1,
        origin: Some(ShellJobOrigin {
            api_call_id: 3,
            tool_call_id: 4,
        }),
        background: false,
    };
    append_index_line(temp.path(), &started).expect("append");
    append_index_line(
        temp.path(),
        &ShellJobIndexLine::Backgrounded {
            version: ShellJobIndexLine::current_version(),
            job_id: String::from("job_000000000001"),
            at_ms: 2,
        },
    )
    .expect("append");
    append_index_line(
        temp.path(),
        &ShellJobIndexLine::Finished {
            version: ShellJobIndexLine::current_version(),
            job_id: String::from("job_000000000001"),
            outcome: ShellJobFinal {
                status: ShellJobStatus::Completed,
                exit_code: Some(0),
                duration_ms: 5,
                finished_at_ms: 6,
                failure_reason: None,
            },
        },
    )
    .expect("append");
    let index_path = temp.path().join("index.jsonl");
    let committed = std::fs::read_to_string(&index_path).expect("read index");
    std::fs::write(
        &index_path,
        format!("{committed}{{\"event\":\"started\",\"vers"),
    )
    .expect("write torn line");
    let records = read_session_job_records(temp.path()).expect("records");
    assert_eq!(records.len(), 1);
    assert!(records[0].background);
    assert_eq!(
        records[0].outcome.as_ref().map(|outcome| outcome.status),
        Some(ShellJobStatus::Completed)
    );

    let future = committed.replace("\"version\":1", "\"version\":9");
    std::fs::write(&index_path, future).expect("write future version");
    let refused = read_session_job_records(temp.path());
    assert!(
        matches!(
            refused,
            Err(ShellJobIndexError::UnsupportedVersion { version: 9, .. })
        ),
        "{refused:?}"
    );
}

/// Issued identities are recognised; anything that could escape the job directory is not.
/// 能识别已签发的身份；任何可能逃出任务目录的值都不被识别。
#[test]
fn job_ids_are_validated_before_building_paths() {
    assert!(valid_job_id("job_0123456789ab"));
    assert!(!valid_job_id("job_../../etc"));
    assert!(!valid_job_id("0123456789ab"));
}

/// A command for the platform shell.
/// 平台 shell 下的一条命令。
fn shell(script: &str) -> TokioCommand {
    #[cfg(windows)]
    {
        let mut command = new_background_tokio_command("cmd");
        command.args(["/D", "/C", script]);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = new_background_tokio_command("sh");
        command.args(["-c", script]);
        command
    }
}

/// A script that sleeps for about `seconds` seconds.
/// 睡眠约 `seconds` 秒的脚本。
fn sleep_script(seconds: u32) -> String {
    #[cfg(windows)]
    {
        format!("ping -n {} 127.0.0.1 >NUL", seconds + 1)
    }
    #[cfg(not(windows))]
    {
        format!("sleep {seconds}")
    }
}

/// A spawn request in `jobs_dir` for session `session`.
/// 会话 `session` 在 `jobs_dir` 中的启动请求。
fn request(
    jobs_dir: &Path,
    session: &str,
    background: bool,
    timeout: Option<Duration>,
) -> ShellJobSpawn {
    ShellJobSpawn {
        owner_session_id: session.to_string(),
        origin: None,
        command: String::from("test"),
        purpose: String::from("test command"),
        shell_label: String::from("test"),
        cwd: jobs_dir.to_path_buf(),
        jobs_dir: jobs_dir.to_path_buf(),
        stdin: None,
        background,
        timeout,
        capture_streams: !background,
    }
}

/// A foreground job completes, logs its output and is listed with its outcome.
/// 前台任务完成，记录输出，并带结果列出。
#[tokio::test]
async fn foreground_job_completes_and_is_listed() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ShellJobRuntime::new();
    let job = runtime
        .spawn(
            request(temp.path(), "s1", false, None),
            shell("echo first&& echo second"),
        )
        .expect("spawn");
    let foreground = ShellJobForeground::attach(job.clone());
    let outcome = job.wait().await;
    foreground.release();
    assert_eq!(outcome.status, ShellJobStatus::Completed);
    let tail = job
        .log_reader()
        .expect("log")
        .tail(SHELL_OUTPUT_TAIL_BUDGET_BYTES)
        .expect("tail");
    let text = render_plain_rows(&tail.rows);
    assert!(text.contains("first") && text.contains("second"), "{text}");
    let listed = runtime.list_session_jobs("s1", temp.path()).expect("list");
    assert_eq!(listed.len(), 1);
    assert!(!listed[0].running);
    assert!(
        runtime
            .list_session_jobs("s2", temp.path())
            .expect("list")
            .iter()
            .all(|summary| summary.job_id != job.id() || !summary.running)
    );
    assert!(
        runtime.job("s2", job.id()).is_none(),
        "jobs belong to their session"
    );
    assert_eq!(
        runtime.take_notices("s1").len(),
        0,
        "foreground jobs leave no notice"
    );
}

/// Dropping an unreleased foreground attachment stops the job, as a cancelled tool call did before.
/// 未释放的前台附着被丢弃时停止任务，与以往取消工具调用的行为一致。
#[tokio::test]
async fn cancelled_foreground_attachment_stops_the_job() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ShellJobRuntime::new();
    let job = runtime
        .spawn(
            request(temp.path(), "s1", false, None),
            shell(&sleep_script(30)),
        )
        .expect("spawn");
    drop(ShellJobForeground::attach(job.clone()));
    let outcome = tokio::time::timeout(Duration::from_secs(20), job.wait())
        .await
        .expect("stopped");
    assert_eq!(outcome.status, ShellJobStatus::Stopped);
}

/// A detached job keeps running past its foreground timeout, then leaves a completion notice.
/// 转后台的任务越过前台超时继续运行，结束后留下完成通知。
#[tokio::test]
async fn detached_job_outlives_the_foreground_timeout_and_leaves_a_notice() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ShellJobRuntime::new();
    let job = runtime
        .spawn(
            request(temp.path(), "s1", false, Some(Duration::from_millis(500))),
            shell(&format!("{} && echo done", sleep_script(2))),
        )
        .expect("spawn");
    let foreground = ShellJobForeground::attach(job.clone());
    assert!(job.detach(false));
    foreground.release();
    let outcome = tokio::time::timeout(Duration::from_secs(20), job.wait())
        .await
        .expect("finished");
    assert_eq!(outcome.status, ShellJobStatus::Completed);
    let notices = runtime.take_notices("s1");
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].job_id, job.id());
    let records = read_session_job_records(temp.path()).expect("records");
    assert!(records[0].background);
}

/// Rejects an unrepresentable deadline before creating a job directory.
/// 拒绝不可表示的截止时间，且不创建任务目录。
#[tokio::test]
async fn invalid_deadline_leaves_job_directory_uncreated() {
    // Use an absent directory so the assertion observes the logging side effect directly.
    // 使用不存在的目录，直接观察写入日志的副作用。
    let temp = tempfile::tempdir().expect("temp dir");
    let jobs = temp.path().join("jobs");
    let runtime = ShellJobRuntime::new();
    assert!(matches!(
        runtime.spawn(
            request(&jobs, "s1", false, Some(Duration::MAX)),
            shell("exit 0"),
        ),
        Err(ShellJobSpawnError::TimeoutOutOfRange)
    ));
    assert!(!jobs.exists());
}

/// A job started in the background keeps its timeout; stop ends another job.
/// 以后台方式启动的任务保留其超时；停止请求结束另一个任务。
#[tokio::test]
async fn background_timeout_and_stop_end_jobs() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ShellJobRuntime::new();
    let timed = runtime
        .spawn(
            request(temp.path(), "s1", true, Some(Duration::from_millis(300))),
            shell(&sleep_script(30)),
        )
        .expect("spawn");
    let stopped = runtime
        .spawn(
            request(temp.path(), "s1", true, None),
            shell(&sleep_script(30)),
        )
        .expect("spawn");
    assert_eq!(runtime.running_background_count("s1"), 2);
    assert_eq!(runtime.stop_session_jobs("s2", "other session"), 0);
    stopped.request_stop("Stopped by test.");
    let timed_outcome = tokio::time::timeout(Duration::from_secs(20), timed.wait())
        .await
        .expect("timed out");
    let stopped_outcome = tokio::time::timeout(Duration::from_secs(20), stopped.wait())
        .await
        .expect("stopped");
    assert_eq!(timed_outcome.status, ShellJobStatus::TimedOut);
    assert_eq!(stopped_outcome.status, ShellJobStatus::Stopped);
    assert_eq!(runtime.take_notices("s1").len(), 2);
}

/// An interjection releases a wait while the job keeps running; the budget ends a wait too.
/// 插队会释放等待而任务继续运行；等待预算到期同样结束等待。
#[tokio::test]
async fn interjection_releases_a_wait() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ShellJobRuntime::new();
    let job = runtime
        .spawn(
            request(temp.path(), "s1", true, None),
            shell(&sleep_script(30)),
        )
        .expect("spawn");
    runtime.notify_interjection("s1");
    let mut interjection = runtime.subscribe_interjection("s1");
    let budget = job
        .wait_interruptible(Some(Duration::from_millis(100)), &mut interjection)
        .await;
    assert_eq!(
        budget,
        ShellJobWaitOutcome::StillRunning,
        "earlier interjections are not replayed"
    );
    let waiter = {
        let job = job.clone();
        let mut interjection = runtime.subscribe_interjection("s1");
        tokio::spawn(async move { job.wait_interruptible(None, &mut interjection).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    runtime.notify_interjection("s1");
    let released = tokio::time::timeout(Duration::from_secs(5), waiter)
        .await
        .expect("released")
        .expect("join");
    assert_eq!(released, ShellJobWaitOutcome::Interjected);
    assert!(job.outcome().is_none());
    job.request_stop("cleanup");
    job.wait().await;
}

/// The background limit refuses one more background job but never a foreground one.
/// 后台上限拒绝再多一个后台任务，但从不拒绝前台任务。
#[tokio::test]
async fn background_limit_applies_only_to_background_requests() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ShellJobRuntime::new();
    let mut jobs = Vec::new();
    for _ in 0..MAX_RUNNING_BACKGROUND_SHELL_JOBS {
        jobs.push(
            runtime
                .spawn(
                    request(temp.path(), "s1", true, None),
                    shell(&sleep_script(30)),
                )
                .expect("spawn"),
        );
    }
    assert!(matches!(
        runtime.spawn(
            request(temp.path(), "s1", true, None),
            shell(&sleep_script(30))
        ),
        Err(ShellJobSpawnError::BackgroundLimit { .. })
    ));
    let foreground = runtime
        .spawn(request(temp.path(), "s1", false, None), shell("echo ok"))
        .expect("foreground spawn");
    jobs.push(foreground);
    runtime.stop_session_jobs("s1", "cleanup");
    for job in jobs {
        job.wait().await;
    }
}

/// A detach from elsewhere releases the foreground wait, and the foreground keeps separate raw streams.
/// 来自其他地方的转后台会释放前台等待，前台调用分别保留原始流。
#[tokio::test]
async fn detach_releases_the_foreground_wait_and_streams_stay_separate() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = ShellJobRuntime::new();
    let job = runtime
        .spawn(
            request(temp.path(), "s1", false, None),
            shell(&format!("echo out&& echo err 1>&2&& {}", sleep_script(30))),
        )
        .expect("spawn");
    let waiter = {
        let job = job.clone();
        let mut interjection = runtime.subscribe_interjection("s1");
        tokio::spawn(async move { job.wait_interruptible(None, &mut interjection).await })
    };
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(job.detach(false));
    let released = tokio::time::timeout(Duration::from_secs(5), waiter)
        .await
        .expect("released")
        .expect("join");
    assert_eq!(released, ShellJobWaitOutcome::Detached);
    let streams = job.captured_streams().expect("captured");
    assert!(
        streams.stdout.contains("out") && !streams.stdout.contains("err"),
        "{streams:?}"
    );
    assert!(streams.stderr.contains("err"), "{streams:?}");
    job.request_stop("cleanup");
    job.wait().await;
}
