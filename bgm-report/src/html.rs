//! HTML reporter — a single self-contained page (inline CSS + inline SVG, no
//! external assets) you can open in a browser or attach to a CI artifact.

use bgm_metrics::BenchReport;

use crate::reporter::Reporter;

/// Renders the report as a standalone, dark-themed HTML document.
#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlReporter;

/// Inline stylesheet kept out of `format!` so its braces don't need escaping.
const STYLE: &str = "\
:root{--bg:#15171c;--card:#1d2026;--fg:#e6e6e6;--muted:#8a8a8a;--accent:#5ad08f;--alt:#46c6c6;--bad:#e06c6c;--warn:#e0bf5a}\
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace}\
.wrap{max-width:900px;margin:32px auto;padding:0 20px}\
.brand{display:inline-block;background:var(--accent);color:#000;font-weight:700;padding:2px 8px;border-radius:4px}\
h1{font-size:20px;font-weight:600;margin:14px 0 4px}.sub{color:var(--muted);margin:0 0 24px}\
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin-bottom:24px}\
.card{background:var(--card);border:1px solid #2a2e36;border-radius:10px;padding:14px}\
.card .k{color:var(--muted);font-size:12px}.card .v{font-size:22px;font-weight:700;color:var(--accent)}\
.card .v.alt{color:var(--alt)}.card .v.bad{color:var(--bad)}\
table{width:100%;border-collapse:collapse;background:var(--card);border-radius:10px;overflow:hidden;margin-bottom:24px}\
th,td{text-align:left;padding:8px 12px;border-bottom:1px solid #2a2e36}th{color:var(--muted);font-weight:600}\
.chart{background:var(--card);border-radius:10px;padding:16px;margin-bottom:24px}\
.chart h2,.tbl-h{color:var(--muted);font-size:13px;font-weight:600;margin:0 0 12px}\
text{fill:var(--muted);font:11px ui-monospace,monospace}";

impl Reporter for HtmlReporter {
    fn render(&self, report: &BenchReport) -> String {
        let l = &report.latency;
        let error_rate = if report.total > 0 {
            (report.total - report.success) as f64 / report.total as f64 * 100.0
        } else {
            0.0
        };

        let mut h = String::with_capacity(4096);
        h.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
        h.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
        h.push_str("<title>BGM report</title><style>");
        h.push_str(STYLE);
        h.push_str("</style></head><body><div class=\"wrap\">");
        h.push_str("<span class=\"brand\">BGM</span><h1>Load-test report</h1>");
        h.push_str(&format!(
            "<p class=\"sub\">{:.1}s · {} iterations</p>",
            report.elapsed_secs, report.total
        ));

        // Summary cards.
        h.push_str("<div class=\"cards\">");
        card(&mut h, "RPS", &format!("{:.0}", report.rps), "");
        card(&mut h, "Throughput", &format!("{:.2} MiB/s", report.throughput_bps / 1_048_576.0), "alt");
        card(&mut h, "p50 latency", &format!("{:.1} ms", l.p50_ms), "alt");
        card(&mut h, "p99 latency", &format!("{:.1} ms", l.p99_ms), "alt");
        card(
            &mut h,
            "Error rate",
            &format!("{error_rate:.2}%"),
            if error_rate > 0.0 { "bad" } else { "" },
        );
        h.push_str("</div>");

        // Distribution chart.
        h.push_str("<div class=\"chart\"><h2>LATENCY DISTRIBUTION (ms)</h2>");
        h.push_str(&distribution_svg(report));
        h.push_str("</div>");

        // Latency table.
        h.push_str("<div class=\"tbl-h\">LATENCY PERCENTILES (ms)</div><table><tr>");
        for head in ["min", "mean", "p50", "p90", "p95", "p99", "max"] {
            h.push_str(&format!("<th>{head}</th>"));
        }
        h.push_str(&format!(
            "</tr><tr><td>{:.1}</td><td>{:.1}</td><td>{:.1}</td><td>{:.1}</td><td>{:.1}</td><td>{:.1}</td><td>{:.1}</td></tr></table>",
            l.min_ms, l.mean_ms, l.p50_ms, l.p90_ms, l.p95_ms, l.p99_ms, l.max_ms
        ));

        // Status codes table.
        if !report.status_codes.is_empty() {
            h.push_str("<div class=\"tbl-h\">STATUS CODES</div><table><tr><th>code</th><th>count</th></tr>");
            for (code, count) in &report.status_codes {
                h.push_str(&format!("<tr><td>{code}</td><td>{count}</td></tr>"));
            }
            h.push_str("</table>");
        }

        h.push_str("</div></body></html>");
        h
    }

    fn extension(&self) -> &'static str {
        "html"
    }
}

/// Append one summary card.
fn card(out: &mut String, key: &str, value: &str, class: &str) {
    out.push_str(&format!(
        "<div class=\"card\"><div class=\"k\">{key}</div><div class=\"v {class}\">{value}</div></div>"
    ));
}

/// Build an inline SVG bar chart of the latency distribution.
fn distribution_svg(report: &BenchReport) -> String {
    let dist = &report.distribution;
    let max = dist.iter().map(|b| b.count).max().unwrap_or(0).max(1) as f64;
    let n = dist.len().max(1) as f64;

    let width = 860.0;
    let baseline = 180.0;
    let bar_zone = 150.0;
    let slot = width / n;

    let mut s = format!("<svg viewBox=\"0 0 {width:.0} 210\" width=\"100%\" height=\"210\">");
    for (i, bucket) in dist.iter().enumerate() {
        let bar_h = (bucket.count as f64 / max) * bar_zone;
        let x = i as f64 * slot + 4.0;
        let y = baseline - bar_h;
        let bw = slot - 8.0;
        s.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{bw:.1}\" height=\"{bar_h:.1}\" rx=\"3\" fill=\"#46c6c6\"/>"
        ));
        let label = bucket.le_ms.map_or_else(|| "\u{221e}".to_owned(), fmt_edge);
        let cx = x + bw / 2.0;
        s.push_str(&format!(
            "<text x=\"{cx:.1}\" y=\"{:.0}\" text-anchor=\"middle\">{label}</text>",
            baseline + 16.0
        ));
        if bucket.count > 0 {
            s.push_str(&format!(
                "<text x=\"{cx:.1}\" y=\"{:.0}\" text-anchor=\"middle\">{}</text>",
                y - 4.0,
                bucket.count
            ));
        }
    }
    s.push_str("</svg>");
    s
}

/// `1000.0` → `1k`, `50.0` → `50`.
fn fmt_edge(ms: f64) -> String {
    if ms >= 1_000.0 {
        format!("{:.0}k", ms / 1_000.0)
    } else {
        format!("{ms:.0}")
    }
}
