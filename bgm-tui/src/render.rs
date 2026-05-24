//! All drawing lives here: given a [`Snapshot`], paint one frame. Keeping the
//! renderer a pure function of the view-model (no I/O, no mutation) makes the
//! layout easy to reason about and the loop in `lib.rs` trivial.
//!
//! Layout mirrors the reference dashboard:
//! ```text
//! ┌ header: BGM · status · duration ───────────────── progress ┐
//! ├ metrics (real-time) ──────────┬ latency distribution ──────┤
//! └ footer: keys ─────────────────────────────── totals ───────┘
//! ```

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
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

fn draw_header(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let block = panel("");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [left, gauge] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(34)]).areas(inner);

    let (status_text, status_color) = if snap.finished {
        ("[ DONE ]", theme::OK)
    } else {
        ("[ACTIVE]", theme::ACCENT)
    };

    let line = Line::from(vec![
        Span::styled(
            " BGM ",
            Style::default()
                .fg(ratatui::style::Color::Black)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
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
        .gauge_style(Style::default().fg(theme::ACCENT))
        .ratio(ratio)
        .label(Span::styled(
            label,
            Style::default().add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(gauge, area);
}

fn draw_metrics(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let block = panel("METRICS (REAL-TIME)");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stats, spark] = Layout::vertical([Constraint::Length(5), Constraint::Min(1)]).areas(inner);

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
                Style::default().fg(theme::ACCENT_ALT),
            ),
        ]),
        Line::from(vec![
            label("LATENCY (ms)  "),
            Span::styled(format!("p50 {p50}"), Style::default().fg(theme::ACCENT_ALT)),
            Span::raw("  "),
            Span::styled(format!("p99 {p99}"), Style::default().fg(theme::ACCENT_ALT)),
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

    draw_sparkline(frame, spark, snap);
}

fn draw_sparkline(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    // Show only as many recent samples as fit the width.
    let width = area.width as usize;
    let hist = snap.rps_history;
    let start = hist.len().saturating_sub(width);
    let bars: Vec<Bar> = hist[start..]
        .iter()
        .map(|&v| Bar::default().value(v).text_value(String::new()))
        .collect();

    let chart = BarChart::default()
        .data(BarGroup::default().bars(&bars))
        .bar_width(1)
        .bar_gap(0)
        .max(snap.peak_rps.max(1))
        .bar_style(Style::default().fg(theme::BAR))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(theme::MUTED))
                .title(Span::styled(" rps ", Style::default().fg(theme::MUTED))),
        );
    frame.render_widget(chart, area);
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
            Span::styled(text, Style::default().fg(theme::ACCENT_ALT)),
            Span::raw("  "),
        ]
    };
    let mut spans = Vec::new();
    spans.extend(cell("Min", l.min_ms));
    spans.extend(cell("Avg", l.mean_ms));
    spans.extend(cell("p50", l.p50_ms));
    spans.extend(cell("Max", l.max_ms));
    frame.render_widget(Paragraph::new(Line::from(spans)), summary);

    // Chart on the left, percentile ladder text on the right (like the ref).
    let [chart_area, plist] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(16)]).areas(row);

    // Real latency distribution: one bar per fixed bucket, height = sample count.
    let dist = &snap.report.distribution;
    let max_count = dist.iter().map(|b| b.count).max().unwrap_or(0).max(1);
    let labels: Vec<String> = dist
        .iter()
        .map(|b| b.le_ms.map_or_else(|| "∞".to_owned(), fmt_edge))
        .collect();
    let bars: Vec<Bar> = dist
        .iter()
        .zip(&labels)
        .map(|(bucket, label)| {
            Bar::default()
                .value(bucket.count)
                .label(Line::from(label.as_str()))
                .text_value(String::new())
                .style(Style::default().fg(theme::ACCENT_ALT))
        })
        .collect();
    let chart = BarChart::default()
        .data(BarGroup::default().bars(&bars))
        .bar_width(3)
        .bar_gap(1)
        .max(max_count);
    frame.render_widget(chart, chart_area);

    // Percentile column.
    let pline = |name: &'static str, v: f64| {
        let text = if has_data {
            format!("{v:.1}")
        } else {
            "--".to_owned()
        };
        Line::from(vec![
            Span::styled(format!("{name:<4}"), Style::default().fg(theme::MUTED)),
            Span::styled(text, Style::default().fg(theme::ACCENT_ALT)),
        ])
    };
    let plines = vec![
        pline("p50", l.p50_ms),
        pline("p90", l.p90_ms),
        pline("p95", l.p95_ms),
        pline("p99", l.p99_ms),
        pline("max", l.max_ms),
    ];
    frame.render_widget(Paragraph::new(plines), plist);
}

/// Format a bucket edge in ms: `1000.0` → `1k`, `50.0` → `50`.
fn fmt_edge(ms: f64) -> String {
    if ms >= 1_000.0 {
        format!("{:.0}k", ms / 1_000.0)
    } else {
        format!("{ms:.0}")
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, snap: &Snapshot) {
    let [left, right] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(50)]).areas(area);

    let keys = if snap.show_help {
        "q/Esc quit · c clear stats · h toggle help · Ctrl-C abort"
    } else {
        "K: q-quit · c-clear · h-help"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(keys, Style::default().fg(theme::MUTED))),
        left,
    );

    let r = snap.report;
    let totals = format!(
        "Iter: {} | Bytes: {} | RPS avg: {:.0}",
        fmt_count(r.total),
        fmt_mib(r.total_bytes as f64),
        r.rps,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(totals, Style::default().fg(theme::MUTED)))
            .alignment(Alignment::Right),
        right,
    );
}

/// p50/p99 as integers, or `--` before any data has arrived.
fn lat_pair(snap: &Snapshot) -> (String, String) {
    if snap.report.total == 0 {
        return ("--".to_owned(), "--".to_owned());
    }
    let l = &snap.report.latency;
    (format!("{:.0}", l.p50_ms), format!("{:.0}", l.p99_ms))
}
