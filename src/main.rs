//! `bgm` — the command-line entry point and composition root.
//!
//! This binary is the **Facade**: it wires the independent crates together and
//! hides that assembly behind a single command. The flow is the one documented
//! in `updates/00-overview.md`:
//!
//! 1. CLI flags or a YAML file → [`Scenario`] (domain model).
//! 2. [`HttpProtocol`] (Strategy) + [`EngineBuilder`] → a running engine.
//! 3. The engine streams `IterReport`s; a collector consumes them — the live
//!    [`bgm_tui`] dashboard, or a silent aggregator for text/JSON output.
//!
//! No business logic lives here; it only translates user intent into the
//! vocabulary the library crates already speak.

use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use bgm_core::IterReport;
use bgm_engine::EngineBuilder;
use bgm_metrics::{Aggregate, BenchReport};
use bgm_protocols::{HttpClientConfig, HttpProtocol, WsConfig, WsProtocol};
use bgm_report::{Comparison, CsvReporter, HtmlReporter, JsonReporter, Reporter, TextReporter};
use bgm_scenario::{Header, HttpRequestSpec, Profile, ProtocolKind, Scenario, ScenarioBuilder};
use bgm_tui::RunMeta;
use clap::{Parser, ValueEnum};
use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;

/// How the final report is rendered when not using the live dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Output {
    /// Human-readable summary.
    Text,
    /// Machine-readable JSON.
    Json,
    /// One-row CSV summary.
    Csv,
    /// Self-contained HTML page with charts.
    Html,
}

impl Output {
    /// The reporter strategy for this format.
    fn reporter(self) -> Box<dyn Reporter> {
        match self {
            Output::Text => Box::new(TextReporter),
            Output::Json => Box::new(JsonReporter),
            Output::Csv => Box::new(CsvReporter),
            Output::Html => Box::new(HtmlReporter),
        }
    }
}

/// BGM — a powerful async load-testing tool with a real-time TUI.
#[derive(Debug, Parser)]
#[command(name = "bgm", version, about)]
struct Cli {
    /// Target URL (omit when using --file).
    url: Option<String>,

    /// Run a YAML scenario file instead of building one from flags.
    #[arg(short, long, value_name = "PATH")]
    file: Option<PathBuf>,

    /// HTTP method (ignored in --file mode).
    #[arg(short, long)]
    method: Option<String>,

    /// Request header `Name: Value` (repeatable; ignored in --file mode).
    #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
    header: Vec<String>,

    /// Request body (ignored in --file mode).
    #[arg(short, long)]
    body: Option<String>,

    /// Template variable `key=value` (repeatable; merged over the scenario).
    #[arg(short = 'V', long = "var", value_name = "KEY=VALUE")]
    var: Vec<String>,

    /// Number of concurrent workers.
    #[arg(short, long)]
    concurrency: Option<usize>,

    /// Stop after this much wall-clock time (e.g. `30s`, `2m`).
    #[arg(short, long, value_parser = parse_duration)]
    duration: Option<Duration>,

    /// Stop after this many total iterations.
    #[arg(short = 'n', long)]
    iterations: Option<u64>,

    /// Global rate cap in requests per second.
    #[arg(short, long)]
    rate: Option<u32>,

    /// Ramp concurrency up over this window (switches to the ramp-up profile).
    #[arg(long, value_parser = parse_duration)]
    ramp: Option<Duration>,

    /// Per-request timeout.
    #[arg(long, value_parser = parse_duration, default_value = "30s")]
    timeout: Duration,

    /// Follow 3xx redirects (off by default; HTTP only).
    #[arg(long)]
    follow_redirects: bool,

    /// (WebSocket) fire-and-forget: send without waiting for a reply.
    #[arg(long)]
    no_reply: bool,

    /// Output format for the final report.
    #[arg(short, long, value_enum, default_value_t = Output::Text)]
    output: Output,

    /// Write the rendered report to a file instead of stdout.
    #[arg(long, value_name = "PATH")]
    out_file: Option<PathBuf>,

    /// Save this run as a baseline (JSON) for future comparisons.
    #[arg(long, value_name = "PATH")]
    save: Option<PathBuf>,

    /// Compare this run against a saved baseline; exit non-zero on regression.
    #[arg(long, value_name = "PATH")]
    baseline: Option<PathBuf>,

    /// Disable the live dashboard even on a terminal (print a summary instead).
    #[arg(long)]
    no_tui: bool,
}

impl Cli {
    /// Build (and validate) the [`Scenario`] this invocation describes.
    fn build_scenario(&self) -> Result<Scenario> {
        let mut scenario = if let Some(path) = &self.file {
            let text = fs::read_to_string(path)
                .with_context(|| format!("reading scenario {}", path.display()))?;
            Scenario::from_yaml(&text).context("parsing scenario YAML")?
        } else {
            let url = self
                .url
                .clone()
                .context("a target URL is required (or pass --file <scenario.yml>)")?;
            let mut request = HttpRequestSpec::get(url);
            if let Some(method) = &self.method {
                request.method = method.to_uppercase();
            }
            request.headers = parse_headers(&self.header)?;
            request.body = self.body.clone();
            ScenarioBuilder::new("cli").request(request).build()?
        };

        // Flag overrides apply to either source.
        if let Some(c) = self.concurrency {
            scenario.load.concurrency = c;
        }
        if let Some(d) = self.duration {
            scenario.load.duration = Some(d);
        }
        if let Some(n) = self.iterations {
            scenario.load.iterations = Some(n);
        }
        if let Some(r) = self.rate {
            scenario.load.rate = Some(r);
        }
        if let Some(ramp) = self.ramp {
            scenario.load.profile = Profile::RampUp;
            scenario.load.ramp = Some(ramp);
        }
        for kv in &self.var {
            let (key, value) = split_once(kv, '=', "--var expects key=value")?;
            scenario.vars.insert(key, value);
        }

        // For flag-built scenarios, infer the transport from the URL scheme
        // (a YAML scenario keeps whatever `protocol:` it declared).
        if self.file.is_none() {
            let url = &scenario.request.url;
            if url.starts_with("ws://") || url.starts_with("wss://") {
                scenario.protocol = ProtocolKind::Ws;
            }
        }

        scenario.validate().context("invalid scenario")?;
        Ok(scenario)
    }

    /// Use the dashboard only on an interactive terminal, for text output, and
    /// when not explicitly disabled.
    fn use_tui(&self) -> bool {
        !self.no_tui && self.output == Output::Text && std::io::stdout().is_terminal()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let scenario = cli.build_scenario()?;

    // One cancellation token drives graceful shutdown for the whole run.
    let cancel = CancellationToken::new();
    spawn_signal_handler(cancel.clone());

    // Pick the protocol strategy. Each arm builds a (differently-typed) engine
    // but both yield the same `Receiver<IterReport>`, so the run path converges.
    let reports = match scenario.protocol {
        ProtocolKind::Http => {
            let config = HttpClientConfig {
                timeout: cli.timeout,
                follow_redirects: cli.follow_redirects,
            };
            let protocol = HttpProtocol::from_scenario_with(&scenario, config)
                .context("configuring HTTP protocol")?;
            EngineBuilder::new(protocol, &scenario.load)
                .build()
                .run(cancel.clone())
        }
        ProtocolKind::Ws => {
            let config = WsConfig {
                timeout: cli.timeout,
                expect_reply: !cli.no_reply,
            };
            let protocol = WsProtocol::from_scenario_with(&scenario, config)
                .context("configuring WebSocket protocol")?;
            EngineBuilder::new(protocol, &scenario.load)
                .build()
                .run(cancel.clone())
        }
    };
    let report = if cli.use_tui() {
        let meta = RunMeta {
            name: scenario.name.clone(),
            target_iterations: scenario.load.iterations,
            target_duration: scenario.load.duration,
        };
        bgm_tui::run(meta, reports, cancel)
            .await
            .context("dashboard")?
    } else {
        collect_silently(reports).await
    };

    emit_report(&cli, &report)
}

/// Render the report in the chosen format, write it (stdout or file), then run
/// the optional save/baseline side-tasks. Meta messages go to stderr so stdout
/// stays clean for piping.
fn emit_report(cli: &Cli, report: &BenchReport) -> Result<()> {
    let rendered = cli.output.reporter().render(report);
    if let Some(path) = &cli.out_file {
        fs::write(path, &rendered).with_context(|| format!("writing {}", path.display()))?;
        eprintln!("report written to {}", path.display());
    } else {
        print!("{rendered}");
        if !rendered.ends_with('\n') {
            println!();
        }
    }

    if let Some(path) = &cli.save {
        bgm_report::save(report, path)
            .with_context(|| format!("saving baseline {}", path.display()))?;
        eprintln!("baseline saved to {}", path.display());
    }

    if let Some(path) = &cli.baseline {
        let base = bgm_report::load(path)
            .with_context(|| format!("loading baseline {}", path.display()))?;
        let comparison = Comparison::new(report, &base);
        eprintln!("{comparison}");
        if comparison.regressed {
            std::process::exit(1);
        }
    }
    Ok(())
}

/// Drain the report stream into an aggregate without any UI (for piped/JSON use).
async fn collect_silently(mut reports: Receiver<IterReport>) -> BenchReport {
    let start = Instant::now();
    let mut aggregate = Aggregate::new();
    while let Some(report) = reports.recv().await {
        aggregate.record(&report);
    }
    aggregate.snapshot(start.elapsed())
}

/// Cancel the run on the first Ctrl-C (used in silent mode; the dashboard maps
/// the key itself while in raw mode).
fn spawn_signal_handler(cancel: CancellationToken) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            cancel.cancel();
        }
    });
}

/// Parse `Name: Value` header flags into the scenario's header list.
fn parse_headers(raw: &[String]) -> Result<Vec<Header>> {
    raw.iter()
        .map(|h| {
            let (name, value) = split_once(h, ':', "header expects 'Name: Value'")?;
            Ok(Header::new(name, value.trim_start()))
        })
        .collect()
}

/// Split `s` on the first `sep`, trimming the key; error with `msg` if absent.
fn split_once(s: &str, sep: char, msg: &str) -> Result<(String, String)> {
    match s.split_once(sep) {
        Some((k, v)) => Ok((k.trim().to_owned(), v.to_owned())),
        None => bail!("{msg} (got {s:?})"),
    }
}

/// clap value parser for humantime durations like `30s` / `2m`.
fn parse_duration(s: &str) -> std::result::Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| format!("invalid duration {s:?}: {e}"))
}
