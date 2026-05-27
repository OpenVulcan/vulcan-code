//! Release-mode timing helper for tokenizer startup and counting hot paths.
//! 用于测量 tokenizer 启动与统计热路径的 release 模式辅助程序。

use std::{
    env, fs,
    hint::black_box,
    path::PathBuf,
    time::{Duration, Instant},
};

use vulcan_token_counter::{OpenAiEncoding, TokenCounter, TokenizerBackend};

/// Number of hot-loop iterations used for large-text throughput measurements.
/// 大文本吞吐测量使用的热循环次数。
const LARGE_TEXT_ITERATIONS: usize = 20;

/// Runs the timing helper and prints human-readable timing results.
/// 运行测量辅助程序并打印可读耗时结果。
///
/// # Returns
/// 返回值
/// `Ok(())` when all measured token counting paths succeed.
/// 当所有被测 Token 统计路径均成功时返回 `Ok(())`。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input_path = input_path();
    let file_text = fs::read_to_string(&input_path)?;
    let large_text = repeat_to_min_chars(&file_text, 200_000);

    println!("input file: {}", input_path.display());
    println!(
        "input size: {} bytes, expanded size: {} bytes",
        file_text.len(),
        large_text.len()
    );

    let wordchipper = measure_backend(TokenizerBackend::Wordchipper, &file_text, &large_text)?;
    let tokie = measure_backend(TokenizerBackend::Tokie, &file_text, &large_text)?;

    print_token_delta(
        "same-file gpt-5-codex tokens",
        wordchipper.o200k_tokens,
        tokie.o200k_tokens,
    );
    print_token_delta(
        "same-file claude proxy tokens",
        wordchipper.cl100k_proxy_tokens,
        tokie.cl100k_proxy_tokens,
    );

    measure_exact_routes(&file_text)?;

    Ok(())
}

/// Measures one backend on the same input file and expanded throughput buffer.
/// 在同一个输入文件与扩展吞吐缓冲区上测量单个后端。
///
/// # Parameters
/// 参数
/// `backend` selects the tokenizer backend to measure.
/// `backend` 选择需要测量的 tokenizer 后端。
/// `file_text` is the exact file content shared by both backends.
/// `file_text` 是两个后端共享的精确文件内容。
/// `large_text` is a repeated buffer derived from the same file content.
/// `large_text` 是从同一文件内容扩展出的重复缓冲区。
///
/// # Returns
/// 返回值
/// Timing summary values needed for cross-backend comparison.
/// 跨后端对比所需的计时汇总值。
fn measure_backend(
    backend: TokenizerBackend,
    file_text: &str,
    large_text: &str,
) -> Result<BackendMeasurement, Box<dyn std::error::Error>> {
    let construction_started = Instant::now();
    let counter = TokenCounter::new_with_backend(backend)?;
    let construction_elapsed = construction_started.elapsed();

    println!();
    println!("backend: {}", backend);
    println!("construction: {}", format_duration(construction_elapsed));

    let first_o200k = time_result(|| counter.estimate("gpt-5-codex", file_text))?;
    println!(
        "same-file o200k estimate(load+count): {} tokens={} precision={:?}",
        format_duration(first_o200k.elapsed),
        first_o200k.value.tokens,
        first_o200k.value.precision
    );

    let second_o200k = time_result(|| counter.estimate("gpt-5-codex", file_text))?;
    println!(
        "same-file o200k estimate(hot): {} tokens={}",
        format_duration(second_o200k.elapsed),
        second_o200k.value.tokens
    );

    let first_cl100k = time_result(|| counter.estimate("claude-sonnet-4.5", file_text))?;
    println!(
        "same-file cl100k proxy(load+count): {} tokens={} precision={:?}",
        format_duration(first_cl100k.elapsed),
        first_cl100k.value.tokens,
        first_cl100k.value.precision
    );

    let second_cl100k = time_result(|| counter.estimate("claude-sonnet-4.5", file_text))?;
    println!(
        "same-file cl100k proxy(hot): {} tokens={}",
        format_duration(second_cl100k.elapsed),
        second_cl100k.value.tokens
    );

    let warm_counter = TokenCounter::new_with_backend(backend)?;
    let warm_up = time_result(|| warm_counter.warm_up())?;
    println!(
        "warm_up both encodings: {}",
        format_duration(warm_up.elapsed)
    );

    let large_openai = time_repeated(LARGE_TEXT_ITERATIONS, || {
        counter.estimate_tokens("gpt-5-codex", black_box(large_text))
    })?;
    print_throughput("large o200k hot", large_text, large_openai);

    let large_doubao = time_repeated(LARGE_TEXT_ITERATIONS, || {
        counter.estimate_tokens("doubao-seed-2.0-pro", black_box(large_text))
    })?;
    print_throughput("large eastern proxy hot", large_text, large_doubao);

    println!(
        "loaded flags: o200k={} cl100k={}",
        counter.is_encoding_loaded(OpenAiEncoding::O200kBase),
        counter.is_encoding_loaded(OpenAiEncoding::Cl100kBase)
    );

    Ok(BackendMeasurement {
        o200k_tokens: second_o200k.value.tokens,
        cl100k_proxy_tokens: second_cl100k.value.tokens,
    })
}

/// Returns the benchmark input path from CLI or this crate's `src/lib.rs`.
/// 从命令行或本 crate 的 `src/lib.rs` 返回基准输入路径。
///
/// # Returns
/// 返回值
/// The file path used by both backend measurements.
/// 两个后端测量共同使用的文件路径。
fn input_path() -> PathBuf {
    env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("lib.rs")
    })
}

/// Minimal token results retained for cross-backend comparison.
/// 为跨后端对比保留的最小 Token 结果。
struct BackendMeasurement {
    /// Exact o200k route count for the shared file content.
    /// 共享文件内容在精确 o200k 路由下的数量。
    o200k_tokens: usize,
    /// Claude proxy count for the shared file content.
    /// 共享文件内容在 Claude 代理路由下的数量。
    cl100k_proxy_tokens: usize,
}

/// Measures exact `.tkz` model-family routes on the same input file.
/// 在同一个输入文件上测量精确 `.tkz` 模型族路由。
///
/// # Parameters
/// 参数
/// `file_text` is the exact file content shared by all route measurements.
/// `file_text` 是所有路由测量共享的精确文件内容。
///
/// # Returns
/// 返回值
/// `Ok(())` when all measured routes succeed.
/// 所有被测路由成功时返回 `Ok(())`。
fn measure_exact_routes(file_text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let counter = TokenCounter::new()?;
    let models = [
        "Qwen/Qwen3.6-35B-A3B",
        "Qwen/Qwen3-Coder-Next",
        "deepseek-ai/DeepSeek-V4-Pro",
        "MiniMaxAI/MiniMax-M2.7",
        "zai-org/GLM-5.1",
        "tencent/Hy3-preview",
        "baidu/ERNIE-4.5-300B-A47B-PT",
        "meta-llama/Llama-4-Scout-17B-16E",
        "mistralai/Mistral-Medium-3.5-128B",
        "microsoft/Phi-4",
        "nvidia/NVIDIA-Nemotron-3-Super-120B-A12B-FP8",
        "google/gemini-3-pro",
        "moonshotai/Kimi-K2.6",
        "xai-org/grok-2",
    ];

    println!();
    println!("exact .tkz route checks:");
    for model in models {
        let first = time_result(|| counter.estimate(model, file_text))?;
        let second = time_result(|| counter.estimate(model, file_text))?;
        println!(
            "{}: first={} hot={} tokens={} precision={:?} tier={:?}",
            model,
            format_duration(first.elapsed),
            format_duration(second.elapsed),
            second.value.tokens,
            second.value.precision,
            second.value.routed_tier
        );
    }

    Ok(())
}

/// Result value with its measured elapsed duration.
/// 带有测量耗时的结果值。
struct Timed<T> {
    /// Value returned by the measured operation.
    /// 被测操作返回的值。
    value: T,
    /// Elapsed duration of the measured operation.
    /// 被测操作消耗的时间。
    elapsed: Duration,
}

/// Measures one fallible operation once.
/// 对单个可失败操作进行一次测量。
///
/// # Parameters
/// 参数
/// `operation` is the closure to execute and time.
/// `operation` 是需要执行并计时的闭包。
///
/// # Returns
/// 返回值
/// The operation value plus elapsed duration, or the original operation error.
/// 操作返回值与耗时，或原始操作错误。
fn time_result<T, E>(operation: impl FnOnce() -> Result<T, E>) -> Result<Timed<T>, E> {
    let started = Instant::now();
    let value = operation()?;
    Ok(Timed {
        value,
        elapsed: started.elapsed(),
    })
}

/// Measures one fallible operation repeatedly and returns the total elapsed duration.
/// 重复测量单个可失败操作并返回总耗时。
///
/// # Parameters
/// 参数
/// `iterations` is the number of repetitions.
/// `iterations` 是重复次数。
/// `operation` is the closure to execute repeatedly.
/// `operation` 是需要重复执行的闭包。
///
/// # Returns
/// 返回值
/// Total elapsed duration for all iterations, or the original operation error.
/// 全部迭代的总耗时，或原始操作错误。
fn time_repeated<T, E>(
    iterations: usize,
    operation: impl Fn() -> Result<T, E>,
) -> Result<Duration, E> {
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(operation()?);
    }
    Ok(started.elapsed())
}

/// Builds a deterministic mixed-language text buffer with at least `target_chars` characters.
/// 构建至少包含 `target_chars` 个字符的确定性中英代码混合文本。
///
/// # Parameters
/// 参数
/// `source` is the file content used as the repeated unit.
/// `source` 是作为重复单元的文件内容。
/// `target_chars` is the approximate minimum character count to generate.
/// `target_chars` 是需要生成的大致最小字符数量。
///
/// # Returns
/// 返回值
/// A mixed-language text buffer suitable for tokenizer throughput measurement.
/// 一个适合 tokenizer 吞吐测量的中英混合文本缓冲区。
fn repeat_to_min_chars(source: &str, target_chars: usize) -> String {
    let mut text = String::with_capacity(target_chars + source.len());
    while text.chars().count() < target_chars {
        text.push_str(source);
        if !source.ends_with('\n') {
            text.push('\n');
        }
    }
    text
}

/// Prints throughput statistics for one large-text timing run.
/// 打印单次大文本计时运行的吞吐统计。
///
/// # Parameters
/// 参数
/// `label` names the measured route.
/// `label` 命名被测路由。
/// `text` is the measured input text.
/// `text` 是被测输入文本。
/// `elapsed` is the total elapsed duration across repeated iterations.
/// `elapsed` 是重复迭代的总耗时。
fn print_throughput(label: &str, text: &str, elapsed: Duration) {
    let bytes = text.len() * LARGE_TEXT_ITERATIONS;
    let average = elapsed / LARGE_TEXT_ITERATIONS as u32;
    let mib_per_second = (bytes as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();

    println!(
        "{}: total={} avg={} throughput={:.2} MiB/s",
        label,
        format_duration(elapsed),
        format_duration(average),
        mib_per_second
    );
}

/// Prints absolute and percentage token deltas between two backends.
/// 打印两个后端之间的绝对与百分比 Token 差异。
///
/// # Parameters
/// 参数
/// `label` names the compared route.
/// `label` 命名被对比的路由。
/// `wordchipper_tokens` is the result from the default backend.
/// `wordchipper_tokens` 是默认后端返回的结果。
/// `tokie_tokens` is the result from the `.tkz` backend.
/// `tokie_tokens` 是 `.tkz` 后端返回的结果。
fn print_token_delta(label: &str, wordchipper_tokens: usize, tokie_tokens: usize) {
    let signed_delta = tokie_tokens as isize - wordchipper_tokens as isize;
    let percentage = if wordchipper_tokens == 0 {
        0.0
    } else {
        signed_delta as f64 / wordchipper_tokens as f64 * 100.0
    };

    println!();
    println!(
        "{}: wordchipper={} tokie={} delta={} ({:+.2}%)",
        label, wordchipper_tokens, tokie_tokens, signed_delta, percentage
    );
}

/// Formats a duration using milliseconds with microsecond precision.
/// 使用毫秒与微秒精度格式化持续时间。
///
/// # Parameters
/// 参数
/// `duration` is the duration to format.
/// `duration` 是需要格式化的持续时间。
///
/// # Returns
/// 返回值
/// A compact duration string.
/// 一个紧凑的持续时间字符串。
fn format_duration(duration: Duration) -> String {
    format!("{:.3} ms", duration.as_secs_f64() * 1000.0)
}
