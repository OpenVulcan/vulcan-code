// Vendored from openai/codex (codex-rs/code-mode-host/src/main.rs) at c419fd01a70dd1d05356da443ac44328bda2e6f4.
// Modified for Vulcan Code; see NOTICE.md in this crate.
// VULCAN MODIFICATION: upstream also accepted `--otel-trace-listen` and `--otel-trace-exporter`
// and installed a `codex_otel` tracing layer. Vulcan does not export OpenTelemetry traces, so the
// host keeps only `--listen` and the stderr log layer; the startup span and transport are unchanged.
use clap::Parser;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Debug, Parser)]
struct Cli {
    /// Transport endpoint: `stdio`, `stdio://`, `grpc+stdio://`, or `grpc://IP:PORT`.
    #[arg(
        long,
        value_name = "URL",
        default_value = vulcan_code_mode_host::DEFAULT_LISTEN_URL
    )]
    listen: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO),
        )
        .init();
    tracing::info_span!(
        "code_mode_host.startup",
        otel.name = "code_mode_host.startup"
    )
    .in_scope(|| {});

    vulcan_code_mode_host::run_main(&cli.listen).await
}
