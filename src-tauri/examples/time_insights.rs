//! Performance harness: time every phase of the Insights load against a COPY of the
//! encrypted mirror. Prints ONLY counts and timings (never record values).
//! Usage: cargo run --release --example time_insights -- "<path-to-mirror-copy.db>"
//! Rebuild phase and counter progress is printed from a `progress::Reporter` sink;
//! `commands.rs` additionally logs rebuild/views timings under the `insights_timing` target.

use emanuel_customer_intelligence_lib::{insights, progress, risk, store};
use std::path::Path;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("pass the mirror.db COPY path as arg 1");
    let key = keyring::v1::Entry::new("emanuel-customer-intelligence", "db_key")?.get_password()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "insights_timing=debug".into()),
        )
        .with_target(false)
        .without_time()
        .init();

    let t = Instant::now();
    let mut s = store::open(Path::new(&path), &key)?;
    println!("[open] store::open: {} ms", t.elapsed().as_millis());

    // (a) rebuild (pass --skip-rebuild as arg 2 to re-time views/risk against an already-built mart)
    let skip_rebuild = std::env::args().any(|a| a == "--skip-rebuild");
    if skip_rebuild {
        println!("[rebuild] skipped (--skip-rebuild)");
    } else {
    let t_all = Instant::now();
    let mut last = Instant::now();
    let mut last_step = 0;
    let mut events = 0usize;
    // Print every phase transition plus throttled counter ticks (counts only, never values).
    let mut sink = |ev: &progress::ProgressEvent| {
        events += 1;
        if ev.step != last_step || ev.done.is_none() {
            println!(
                "  [progress] step {}/{} {:<40} elapsed={} ms (+{} ms since previous phase)",
                ev.step,
                ev.steps,
                ev.phase,
                ev.elapsed_ms,
                last.elapsed().as_millis()
            );
            last = Instant::now();
            last_step = ev.step;
        } else {
            println!(
                "  [progress]   tick {}/{} done={:?} total={:?} elapsed={} ms",
                ev.step, ev.steps, ev.done, ev.total, ev.elapsed_ms
            );
        }
    };
    let mut reporter = progress::Reporter::new("rebuild", insights::REBUILD_STEPS, &mut sink);
    let info = insights::rebuild_with(&mut s, &mut reporter)?;
    drop(reporter);
    println!("[rebuild] progress events emitted: {events}");
    println!(
        "[rebuild] insights::rebuild_with TOTAL: {} ms  (households={}, unavailable_cols={})",
        t_all.elapsed().as_millis(),
        info.households,
        info.unavailable.len()
    );
    }

    // (b) views: cold and warm
    let cur = insights::current_fy();
    for label in ["cold", "warm"] {
        let t = Instant::now();
        let v = insights::views(&s, cur)?;
        println!(
            "[views] insights::views ({label}): {} ms  (trend_rows={}, anchor_type_rows={})",
            t.elapsed().as_millis(),
            v.trend.len(),
            v.anchor_type.len()
        );
    }

    // (c) risk mart read
    let t = Instant::now();
    let hh = insights::load(&s)?;
    let t_load = t.elapsed().as_millis();
    let t2 = Instant::now();
    let years = insights::load_household_years(&s)?;
    let t_years = t2.elapsed().as_millis();
    let t3 = Instant::now();
    let caps = insights::source_capabilities(&s)?;
    let t_caps = t3.elapsed().as_millis();
    println!(
        "[risk-read] load={} ms (households={}), load_household_years={} ms (household_years={}), source_capabilities={} ms (caps={}) => TOTAL {} ms",
        t_load,
        hh.len(),
        t_years,
        years.len(),
        t_caps,
        caps.len(),
        t.elapsed().as_millis()
    );

    // (d) risk::analyze
    let t = Instant::now();
    let (model, list) = risk::analyze(&hh, &years, &caps, cur, risk::DEFAULT_ALPHA);
    println!(
        "[risk] risk::analyze TOTAL: {} ms  (watch_list_len={}, watch_list_available={}, unavailable_reason={:?}, model_json_bytes={})",
        t.elapsed().as_millis(),
        list.rows.len(),
        list.available,
        list.unavailable_reason,
        serde_json::to_string(&model).map(|j| j.len()).unwrap_or(0)
    );
    Ok(())
}
