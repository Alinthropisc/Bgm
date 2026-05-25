//! # bgm-tui
//!
//! The real-time dashboard. It is a *collector* in BGM's Observer setup: given
//! the engine's `mpsc::Receiver<IterReport>`, it folds reports into a
//! [`bgm_metrics::Aggregate`] and renders a live view until the run ends or the
//! user quits, then returns the final [`BenchReport`].
//!
//! Draining and drawing are decoupled: a hot collector task empties the channel
//! into the shared aggregate (so the engine is never throttled by the UI's
//! frame rate), while the render loop just snapshots that aggregate ~10×/s.

pub mod input;
pub mod render;
pub mod state;
pub mod theme;

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bgm_core::IterReport;
use bgm_metrics::{Aggregate, BenchReport};
use ratatui::DefaultTerminal;
use tokio::sync::mpsc::Receiver;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use crate::input::Action;
use crate::state::Snapshot;

pub use crate::state::RunMeta;

/// Redraw cadence.
const TICK: Duration = Duration::from_millis(100);
/// Seconds per tick, for converting iteration deltas into instantaneous RPS.
const TICK_SECS: f64 = 0.1;
/// How many samples to retain for each sparkline.
const HISTORY: usize = 512;

/// Push a sample onto a ring buffer, dropping the oldest once at capacity.
fn push_capped(history: &mut VecDeque<u64>, value: u64) {
    if history.len() == HISTORY {
        history.pop_front();
    }
    history.push_back(value);
}

/// Run the dashboard to completion and return the final report.
///
/// Cancels `cancel` when the user asks to quit, so the engine can wind down its
/// workers; returns once the engine's channel closes (run finished) or the user
/// quits.
pub async fn run(
    meta: RunMeta,
    mut rx: Receiver<IterReport>,
    cancel: CancellationToken,
) -> io::Result<BenchReport> {
    let shared = Arc::new(Mutex::new(Aggregate::new()));

    // Hot path: drain the channel into the aggregate as fast as it arrives,
    // batching under a single lock per wake-up so the engine never blocks on us.
    let collector = {
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            while let Some(report) = rx.recv().await {
                let mut agg = shared.lock().expect("aggregate mutex poisoned");
                agg.record(&report);
                while let Ok(report) = rx.try_recv() {
                    agg.record(&report);
                }
            }
        })
    };

    let mut terminal = ratatui::init();
    let result = dashboard_loop(&mut terminal, &meta, &shared, &collector, &cancel).await;
    ratatui::restore();

    let elapsed = result?;
    let report = shared
        .lock()
        .expect("aggregate mutex poisoned")
        .snapshot(elapsed);
    Ok(report)
}

/// The draw loop. Returns the elapsed time at exit so the caller can build a
/// consistent final snapshot.
async fn dashboard_loop(
    terminal: &mut DefaultTerminal,
    meta: &RunMeta,
    shared: &Arc<Mutex<Aggregate>>,
    collector: &tokio::task::JoinHandle<()>,
    cancel: &CancellationToken,
) -> io::Result<Duration> {
    let start = Instant::now();
    let mut rps_hist: VecDeque<u64> = VecDeque::with_capacity(HISTORY);
    let mut lat_hist: VecDeque<u64> = VecDeque::with_capacity(HISTORY);
    let mut last_total = 0u64;
    let mut peak_rps = 0u64;
    let mut peak_lat = 0u64;
    let mut show_help = false;
    let mut ticker = interval(TICK);

    loop {
        ticker.tick().await;

        // Input polling is non-blocking, so calling it inline is fine.
        match input::poll()? {
            Some(Action::Quit) => cancel.cancel(),
            Some(Action::Clear) => {
                *shared.lock().expect("aggregate mutex poisoned") = Aggregate::new();
                rps_hist.clear();
                lat_hist.clear();
                last_total = 0;
                peak_rps = 0;
                peak_lat = 0;
            }
            Some(Action::ToggleHelp) => show_help = !show_help,
            None => {}
        }

        let elapsed = start.elapsed();
        let report = shared
            .lock()
            .expect("aggregate mutex poisoned")
            .snapshot(elapsed);

        let delta = report.total.saturating_sub(last_total);
        last_total = report.total;
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let instant_rps = (delta as f64 / TICK_SECS) as u64;
        push_capped(&mut rps_hist, instant_rps);
        peak_rps = peak_rps.max(instant_rps);

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let p50 = report.latency.p50_ms.round() as u64;
        push_capped(&mut lat_hist, p50);
        peak_lat = peak_lat.max(p50);

        let finished = collector.is_finished();
        let rps_samples: Vec<u64> = rps_hist.iter().copied().collect();
        let lat_samples: Vec<u64> = lat_hist.iter().copied().collect();
        let snap = Snapshot {
            meta,
            report: &report,
            elapsed,
            rps_history: &rps_samples,
            peak_rps,
            lat_history: &lat_samples,
            peak_lat,
            finished,
            show_help,
        };
        terminal.draw(|frame| render::draw(frame, &snap))?;

        if cancel.is_cancelled() || finished {
            return Ok(elapsed);
        }
    }
}
