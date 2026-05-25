//! All drawing lives here: given a [`Snapshot`], paint one frame. Keeping the
//! renderer a pure function of the view-model (no I/O, no mutation) makes the
//! layout easy to reason about and the loop in `lib.rs` trivial.
//!
//! Layout mirrors the reference dashboard:
//! ```text
//! ┌ BGM · status · duration ───────────────────── BGM ▓▓░ 35% (92k) ┐
//! ├ METRICS (REAL-TIME) ───────────┬ LATENCY DISTRIBUTION (ms) ──────┤
//! │  stats + rps spark + lat spark │  buckets bar chart + percentiles│
//! └ q-quit · c-clear · h-help ─────────────── Iter | Bytes | RPS ────┘
//! ```

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, BorderType, Borders, Gauge, Paragraph};

use crate::state::{Snapshot, fmt_clock, fmt_count, fmt_mib};
use crate::theme;

/// Entry point: paint the whole dashboard for this frame.
pub fn draw(frame: &mut Frame, snap: &Snapshot) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, snap);

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)]).areas(body);
    draw_metrics(frame, left, snap);
    draw_latency(frame, right, snap);

    draw_footer(frame, footer, snap);
}

/// A bordered panel with a muted, bold title.
fn panel(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::MUTED))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme::MUTED)
                .add_modifier(Modifier::BOLD),
        ))
}

/// A bright " BGM " brand chip.
fn brand() -> Span<'static> {
    Span::styled(
        " BGM ",
        Style::default()
            .fg(theme::INK)
            .bg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    )
}

fn draw_header(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let block = panel("");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [left, right] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(40)]).areas(inner);

    let (status_text, status_color) = if snap.finished {
        ("[ DONE ]", theme::OK)
    } else {
        ("[ACTIVE]", theme::ACCENT)
    };

    let line = Line::from(vec![
        brand(),
        Span::styled(
            format!("  {}", snap.meta.name),
            Style::default().fg(theme::MUTED),
        ),
        Span::raw("   STATUS: "),
        Span::styled(
            status_text,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("   DURATION: "),
        Span::styled(
            fmt_clock(snap.elapsed),
            Style::default().fg(theme::ACCENT_ALT),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), left);

    // Right corner: brand chip + progress gauge.
    let [chip, gauge] =
        Layout::horizontal([Constraint::Length(6), Constraint::Min(10)]).areas(right);
    frame.render_widget(
        Paragraph::new(Line::from(brand())).alignment(Alignment::Right),
        chip,
    );
    draw_progress(frame, gauge, snap);
}

fn draw_progress(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let label = match (snap.meta.target_iterations, snap.progress()) {
        (Some(target), Some(p)) => {
            format!(
                "{:.0}% ({}/{})",
                p * 100.0,
                fmt_count(snap.report.total),
                fmt_count(target)
            )
        }
        (None, Some(p)) => format!("{:.0}%", p * 100.0),
        _ => "● running".to_owned(),
    };
    let ratio = snap.progress().unwrap_or(0.0);
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(theme::ACCENT)
                .bg(Color::Rgb(0x26, 0x2b, 0x33)),
        )
        .ratio(ratio)
        .label(Span::styled(
            label,
            Style::default().fg(theme::INK).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(gauge, area);
}

fn draw_metrics(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let block = panel("METRICS (REAL-TIME)");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stats, charts] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(2)]).areas(inner);

    let r = snap.report;
    let live_rps = snap.rps_history.last().copied().unwrap_or(0);
    let (p50, p99) = lat_pair(snap);

    let label = |s: &'static str| Span::styled(s, Style::default().fg(theme::MUTED));
    let lines = vec![
        Line::from(vec![
            label("RPS (Req/s)   "),
            Span::styled(
                fmt_count(live_rps),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   peak {}", fmt_count(snap.peak_rps)),
                Style::default().fg(theme::MUTED),
            ),
        ]),
        Line::from(vec![
            label("T-PUT (MiB/s) "),
            Span::styled(
                format!("{:.2}", r.throughput_bps / 1_048_576.0),
                Style::default()
                    .fg(theme::ACCENT_ALT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            label("LATENCY (ms)  "),
            Span::styled(format!("p50 {p50}"), Style::default().fg(theme::ACCENT_ALT)),
            Span::raw("  "),
            Span::styled(format!("p99 {p99}"), Style::default().fg(theme::WARN)),
        ]),
        Line::from(vec![
            label("OUTCOMES      "),
            Span::styled(
                format!("ok {}", fmt_count(r.success)),
                Style::default().fg(theme::OK),
            ),
            Span::raw(" · "),
            Span::styled(
                format!("4xx {}", r.client_error),
                Style::default().fg(theme::WARN),
            ),
            Span::raw(" · "),
            Span::styled(
                format!("5xx {} err {}", r.server_error, r.error),
                Style::default().fg(theme::BAD),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), stats);

    // RPS as a gradient "equalizer", latency as a smooth Braille line below it.
    let [rps_area, lat_area] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(charts);
    gradient_sparkline(frame, rps_area, "rps", snap.rps_history, snap.peak_rps, theme::cool);
    braille_line(frame, lat_area, "p50 ms", snap.lat_history, snap.peak_lat);
}

/// A smooth latency curve drawn with Braille sub-pixels (2×4 per cell), so it
/// reads as a continuous line rather than blocky bars. Segments are colored by
/// height via the `heat` gradient.
fn braille_line(frame: &mut Frame, area: Rect, title: &str, samples: &[u64], max: u64) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::MUTED))
        .title(Span::styled(format!(" {title} "), Style::default().fg(theme::MUTED)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if samples.len() < 2 {
        return;
    }
    let max_y = max.max(1) as f64;
    let last = (samples.len() - 1) as f64;
    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, last])
        .y_bounds([0.0, max_y])
        .paint(move |ctx| {
            for i in 0..samples.len() - 1 {
                let y1 = samples[i] as f64;
                let y2 = samples[i + 1] as f64;
                let ratio = y1.max(y2) / max_y;
                ctx.draw(&CanvasLine {
                    x1: i as f64,
                    y1,
                    x2: (i + 1) as f64,
                    y2,
                    color: theme::heat(ratio),
                });
            }
        });
    frame.render_widget(canvas, inner);
}

/// A single-row-per-sample bar chart whose bars are colored by their height via
/// `scheme` (so the line glows from calm to hot).
fn gradient_sparkline(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    samples: &[u64],
    max: u64,
    scheme: fn(f64) -> Color,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme::MUTED))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(theme::MUTED),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Distinct "equalizer" bars (width 2, 1-col gaps) read far better than a
    // solid 1-px wall, matching the reference design. Show only as many recent
    // samples as fit so bars never get squeezed together.
    const BAR_W: u16 = 2;
    const GAP: u16 = 1;
    let slot = (BAR_W + GAP) as usize;
    let fits = (inner.width as usize / slot).max(1);
    let start = samples.len().saturating_sub(fits);
    let scale = max.max(1);
    let bars: Vec<Bar> = samples[start..]
        .iter()
        .map(|&v| {
            let ratio = v as f64 / scale as f64;
            Bar::default()
                .value(v)
                .text_value(String::new())
                .style(Style::default().fg(scheme(ratio)))
        })
        .collect();

    let chart = BarChart::default()
        .data(BarGroup::default().bars(&bars))
        .bar_width(BAR_W)
        .bar_gap(GAP)
        .max(scale);
    frame.render_widget(chart, inner);
}

fn draw_latency(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let block = panel("LATENCY DISTRIBUTION (ms)");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [summary, row] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);

    let l = &snap.report.latency;
    let has_data = snap.report.total > 0;
    let cell = |name: &'static str, v: f64| {
        let text = if has_data {
            format!("{v:.1}")
        } else {
            "--".to_owned()
        };
        vec![
            Span::styled(format!("{name} "), Style::default().fg(theme::MUTED)),
            Span::styled(
                text,
                Style::default()
                    .fg(theme::ACCENT_ALT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
        ]
    };
    let mut spans = Vec::new();
    spans.extend(cell("Min", l.min_ms));
    spans.extend(cell("Avg", l.mean_ms));
    spans.extend(cell("p50", l.p50_ms));
    spans.extend(cell("Max", l.max_ms));
    frame.render_widget(Paragraph::new(Line::from(spans)), summary);

    let [chart_area, plist] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(16)]).areas(row);

    // Real latency distribution: one bar per bucket, height = count, colored by
    // how full the bucket is (the busiest buckets glow brightest). The bucket the
    // median falls into is highlighted in teal so the eye lands on "typical".
    let dist = &snap.report.distribution;
    let max_count = dist.iter().map(|b| b.count).max().unwrap_or(0).max(1);
    let p50_idx = if has_data {
        // First bucket whose upper edge covers p50 (the open ∞ bucket covers all).
        dist.iter()
            .position(|b| b.le_ms.is_none_or(|edge| l.p50_ms <= edge))
    } else {
        None
    };
    let labels: Vec<String> = dist
        .iter()
        .map(|b| b.le_ms.map_or_else(|| "∞".to_owned(), fmt_edge))
        .collect();
    let bars: Vec<Bar> = dist
        .iter()
        .zip(&labels)
        .enumerate()
        .map(|(i, (bucket, label))| {
            let color = if Some(i) == p50_idx {
                theme::ACCENT_ALT
            } else {
                let ratio = bucket.count as f64 / max_count as f64;
                theme::cool(ratio)
            };
            Bar::default()
                .value(bucket.count)
                .label(Line::from(label.as_str()))
                .text_value(String::new())
                .style(Style::default().fg(color))
        })
        .collect();
    let chart = BarChart::default()
        .data(BarGroup::default().bars(&bars))
        .bar_width(3)
        .bar_gap(1)
        .max(max_count);
    frame.render_widget(chart, chart_area);

    // Percentile ladder, richer than the summary line.
    let pline = |name: &'static str, v: f64, color: Color| {
        let text = if has_data {
            format!("{v:.1}")
        } else {
            "--".to_owned()
        };
        Line::from(vec![
            Span::styled(format!("{name:<5}"), Style::default().fg(theme::MUTED)),
            Span::styled(text, Style::default().fg(color)),
        ])
    };
    let plines = vec![
        pline("min", l.min_ms, theme::OK),
        pline("mean", l.mean_ms, theme::ACCENT_ALT),
        pline("p50", l.p50_ms, theme::ACCENT_ALT),
        pline("p90", l.p90_ms, theme::ACCENT_ALT),
        pline("p95", l.p95_ms, theme::WARN),
        pline("p99", l.p99_ms, theme::WARN),
        pline("max", l.max_ms, theme::BAD),
    ];
    frame.render_widget(Paragraph::new(plines), plist);
}

fn draw_footer(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let [left, right] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(52)]).areas(area);

    let keys = if snap.show_help {
        " q/Esc quit · c clear stats · h toggle help · Ctrl-C abort "
    } else {
        " q-quit · c-clear · h-help "
    };
    // Inverted chip (dark ink on a muted plate) to visually anchor the bottom edge.
    frame.render_widget(
        Paragraph::new(Span::styled(
            keys,
            Style::default()
                .fg(theme::INK)
                .bg(theme::MUTED)
                .add_modifier(Modifier::BOLD),
        )),
        left,
    );

    let r = snap.report;
    let totals = Line::from(vec![
        Span::styled("Iter ", Style::default().fg(theme::MUTED)),
        Span::styled(fmt_count(r.total), Style::default().fg(theme::ACCENT)),
        Span::styled("  Bytes ", Style::default().fg(theme::MUTED)),
        Span::styled(
            fmt_mib(r.total_bytes as f64),
            Style::default().fg(theme::ACCENT_ALT),
        ),
        Span::styled("  RPS ", Style::default().fg(theme::MUTED)),
        Span::styled(format!("{:.0}", r.rps), Style::default().fg(theme::ACCENT)),
    ]);
    frame.render_widget(Paragraph::new(totals).alignment(Alignment::Right), right);
}

/// `1000.0` → `1k`, `50.0` → `50`.
fn fmt_edge(ms: f64) -> String {
    if ms >= 1_000.0 {
        format!("{:.0}k", ms / 1_000.0)
    } else {
        format!("{ms:.0}")
    }
}

/// p50/p99 as integers, or `--` before any data has arrived.
fn lat_pair(snap: &Snapshot) -> (String, String) {
    if snap.report.total == 0 {
        return ("--".to_owned(), "--".to_owned());
    }
    let l = &snap.report.latency;
    (format!("{:.0}", l.p50_ms), format!("{:.0}", l.p99_ms))
}
