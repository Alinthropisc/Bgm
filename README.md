# BGM

**BGM** is a powerful async load-testing tool written from scratch in Rust, with
a real-time terminal dashboard. It's an idea-level analog of tools like
ddosify / oha / goose / k6 — its own design, its own code.

```
╭ BGM  api-smoke   STATUS: [ACTIVE]   DURATION: 00:27 ───── BGM ███████░░░░░ 35% (92k/260k) ╮
│ METRICS (REAL-TIME) ──────────────────┬ LATENCY DISTRIBUTION (ms) ────────────────────── │
│  RPS (Req/s)   104k   peak 110k        │  Min 1.2  Avg 6.5  p50 5.1  Max 64               │
│  T-PUT (MiB/s) 15.8                    │   ▃     █                          min   1.2     │
│  LATENCY (ms)  p50 5  p99 41           │   █  ▅  █  ▇                       mean  6.5     │
│  OUTCOMES      ok 305k · 4xx 12 · 5xx 1│   █  █  █  █  ▃                    p50   5.1     │
│ ─ rps ─────────────────────────────── │   █  █  █  █  █  ▂  ▁              p90  18.0     │
│  ▂▃▅▇█▇▆▅▇█▇▅▃▂▃▅▇█▇▅  (gradient bars) │  ─────────────────────            p95  27.4     │
│ ─ p50 ms ───────────────────────────── │  25 50 [75] 100 250 1k ∞          p99  41.0     │
│  ⢀⡠⠔⠊⠉⠉⠢⢄⡀⢀⠤⠒⠊⠉  (braille line)        │   └ p50 bucket highlighted        max  64.0     │
╰ q-quit · c-clear · h-help ─────────────────────────── Iter 306k  Bytes 46 MiB  RPS 104k ╯
```

Rounded TrueColor frames, gradient "equalizer" bars (calm green → hot lime),
a sub-pixel **Braille** latency curve, and the **median bucket** lit up in teal
so your eye lands on "typical" at a glance.

## Quick start

```sh
# from the repo root (builds on first run)
./bgm.sh https://example.com -c 20 -d 30s

# YAML scenario
./bgm.sh --file examples/smoke.yml

# pipe a JSON report (no dashboard)
./bgm.sh https://example.com -n 1000 -o json > report.json
```

Key flags: `-c` concurrency, `-d` duration (`30s`/`2m`), `-n` iterations,
`-r` rate cap (req/s), `--ramp 5s` ramp-up, `-H "K: V"` headers, `-b` body,
`--var key=value` template vars, `-o text|json|csv|html`, `--no-tui`, `--no-reply` (WS).

**Protocols:** HTTP and WebSocket (`protocol: ws`, or a `ws://`/`wss://` URL).
**Assertions:** a `assert:` block checks `status` / `max_latency` / `body_contains`.
**Multi-step flows:** a `steps:` list runs several requests per iteration and
passes data between them — each step can `extract` a JSON-pointer or header
value into a variable that later steps interpolate (`{{ var }}`). Dynamic vars
`{{ seq }}` / `{{ worker_id }}` / `{{ worker_seq }}` are always available. See
`examples/flow.yml`, `examples/ws.yml`.

## Architecture

A Cargo workspace of small, single-responsibility crates. Dependencies point
*inward* toward the dependency-light core (Dependency Inversion):

| crate | role | patterns |
|-------|------|----------|
| `bgm-macros` | `#[derive(Interpolate)]` proc-macro | Derive |
| `bgm-core` | domain traits (`Protocol`, `LoadProfile`), types, errors | Strategy / DIP |
| `bgm-metrics` | latency histogram, live aggregate, `BenchReport` | Observer (data) |
| `bgm-scenario` | YAML + builder model, `{{ var }}` interpolation | Builder / Adapter |
| `bgm-protocols` | `HttpProtocol` over reqwest | Strategy / Factory / Template Method |
| `bgm-engine` | tokio worker pool, load profiles, rate limit | Mediator / Observer |
| `bgm-tui` | ratatui real-time dashboard | Observer / Collector |
| `bgm` (root `src/main.rs`) | CLI / composition root | Facade |

### Flow

1. CLI flags or YAML → `Scenario` (domain model).
2. `EngineBuilder` assembles the engine: load profile + protocol + rate limit.
3. tokio spawns N workers; each `setup()` once, then loops `execute()`.
4. Workers stream `IterReport`s over an `mpsc` channel → a collector (dashboard
   or silent aggregator).
5. The run stops on its duration / iteration budget or a `CancellationToken`
   (Ctrl-C); the aggregator emits a `BenchReport` (text or JSON).

## Reports & baselines

```sh
# render formats: text (default), json, csv, html
./bgm.sh https://example.com -n 1000 -o html --out-file report.html

# save a baseline, then gate future runs against it (exits non-zero on regress)
./bgm.sh https://example.com -n 1000 --save base.bgm.json
./bgm.sh https://example.com -n 1000 --baseline base.bgm.json
```

## Command hub (no env pollution)

Drive everything through the wrapper so nothing leaks into your shell. A
project-local `.bgm.env` (copy from `.bgm.env.example`) is loaded only for the
spawned process.

```sh
./bgm.sh <url|flags...>     # run (default)
./bgm.sh example smoke      # run examples/smoke.yml
./bgm.sh build|fmt|clippy|test|check|examples|env|clean
```

Windows: `bgm.bat` mirrors the same commands.

## Development

```sh
./bgm.sh check    # fmt --check + clippy + test
```

Edition 2024, `unsafe` forbidden workspace-wide, clippy `pedantic` on. CI
(`.github/workflows/ci.yml`) runs rustfmt, clippy, the test matrix
(Linux/macOS/Windows) and an end-to-end smoke test.
