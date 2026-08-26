//! Structured progress for the long Insights jobs (mart rebuild, risk analysis).
//!
//! A `Reporter` walks a fixed, ordered phase list and pushes `ProgressEvent`s into a sink.
//! Progress is a pure side effect: it never touches the store or the analytical numbers.
//! Phase transitions and the terminal tick of a phase are always delivered; intermediate
//! ticks are throttled so a tight per-row loop cannot flood the webview.

use serde::Serialize;
use std::time::{Duration, Instant};

/// Payload of the `insights:progress` event. `step` is 1-based into the job's phase list.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ProgressEvent {
    pub job: &'static str,
    pub phase: String,
    pub step: u32,
    pub steps: u32,
    pub done: Option<u64>,
    pub total: Option<u64>,
    pub elapsed_ms: u64,
}

/// Emits `ProgressEvent`s for one job. Ticks are rate-limited to one per `min_interval`
/// except the terminal tick (`Some(done) == total`); `phase` and `finish` always emit.
pub struct Reporter<'a> {
    job: &'static str,
    steps: u32,
    step: u32,
    phase: String,
    started: Instant,
    last_emit: Option<Instant>,
    min_interval: Duration,
    sink: &'a mut dyn FnMut(&ProgressEvent),
}

impl<'a> Reporter<'a> {
    pub fn new(job: &'static str, steps: u32, sink: &'a mut dyn FnMut(&ProgressEvent)) -> Self {
        Reporter {
            job,
            steps,
            step: 0,
            phase: String::new(),
            started: Instant::now(),
            last_emit: None,
            min_interval: Duration::from_millis(100),
            sink,
        }
    }

    /// Override the tick throttle window (tests).
    pub fn with_min_interval(mut self, d: Duration) -> Self {
        self.min_interval = d;
        self
    }

    /// Enter the next phase (step advances by one, capped at `steps`) and emit unconditionally.
    pub fn phase(&mut self, label: &str) {
        self.step = (self.step + 1).min(self.steps);
        self.phase = label.to_string();
        self.emit(None, None);
    }

    /// Report counter progress within the current phase. Emitted when the throttle window
    /// has passed or when this is the terminal tick. Never changes the step.
    pub fn tick(&mut self, done: u64, total: Option<u64>) {
        let terminal = total == Some(done);
        let due = match self.last_emit {
            None => true,
            Some(t) => t.elapsed() >= self.min_interval,
        };
        if terminal || due {
            self.emit(Some(done), total);
        }
    }

    /// Mark the job complete: step becomes `steps` and a final event is emitted unconditionally.
    pub fn finish(&mut self) {
        self.step = self.steps;
        self.emit(None, None);
    }

    fn emit(&mut self, done: Option<u64>, total: Option<u64>) {
        let ev = ProgressEvent {
            job: self.job,
            phase: self.phase.clone(),
            step: self.step,
            steps: self.steps,
            done,
            total,
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        };
        (self.sink)(&ev);
        self.last_emit = Some(Instant::now());
    }
}

/// A sink that discards every event, for callers that do not report progress.
pub fn noop() -> impl FnMut(&ProgressEvent) {
    |_| {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(f: impl FnOnce(&mut Reporter<'_>), min: Duration) -> Vec<ProgressEvent> {
        let mut out = Vec::new();
        let mut sink = |e: &ProgressEvent| out.push(e.clone());
        let mut r = Reporter::new("rebuild", 3, &mut sink).with_min_interval(min);
        f(&mut r);
        out
    }

    #[test]
    fn phase_advances_step_monotonically_and_caps_at_steps() {
        let evs = collect(
            |r| {
                r.phase("a");
                r.phase("b");
                r.phase("c");
                r.phase("d");
            },
            Duration::ZERO,
        );
        let steps: Vec<u32> = evs.iter().map(|e| e.step).collect();
        assert_eq!(steps, vec![1, 2, 3, 3]);
        assert!(evs.iter().all(|e| e.steps == 3 && e.job == "rebuild"));
        assert_eq!(evs[3].phase, "d");
        assert!(evs.iter().all(|e| e.done.is_none() && e.total.is_none()));
    }

    #[test]
    fn ticks_are_throttled_but_the_terminal_tick_always_lands() {
        let evs = collect(
            |r| {
                r.phase("rows");
                for i in 1..=1000 {
                    r.tick(i, Some(1000));
                }
            },
            Duration::from_secs(3600),
        );
        // The phase event, then exactly one tick: the terminal one.
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1].done, Some(1000));
        assert_eq!(evs[1].total, Some(1000));
        assert_eq!(evs[1].step, 1);
        assert_eq!(evs[1].phase, "rows");
    }

    #[test]
    fn zero_interval_emits_every_tick() {
        let evs = collect(
            |r| {
                r.phase("rows");
                for i in 1..=1000 {
                    r.tick(i, Some(1000));
                }
            },
            Duration::ZERO,
        );
        assert_eq!(evs.len(), 1001);
        assert_eq!(evs.last().unwrap().done, Some(1000));
    }

    #[test]
    fn tick_never_changes_step_and_finish_sets_step_to_steps() {
        let evs = collect(
            |r| {
                r.phase("a");
                r.tick(1, None);
                r.tick(2, Some(5));
                r.finish();
            },
            Duration::ZERO,
        );
        assert_eq!(evs.iter().map(|e| e.step).collect::<Vec<_>>(), vec![1, 1, 1, 3]);
        assert_eq!(evs[1].done, Some(1));
        assert_eq!(evs[1].total, None);
        assert_eq!(evs[2], ProgressEvent { done: Some(2), total: Some(5), ..evs[2].clone() });
        assert_eq!(evs[3].phase, "a", "finish keeps the last phase label");
    }

    #[test]
    fn elapsed_ms_is_non_decreasing() {
        let evs = collect(
            |r| {
                r.phase("a");
                for i in 0..50 {
                    r.tick(i, Some(49));
                }
                r.phase("b");
                r.finish();
            },
            Duration::ZERO,
        );
        assert!(evs.windows(2).all(|w| w[0].elapsed_ms <= w[1].elapsed_ms));
    }

    #[test]
    fn serializes_with_the_frontend_field_names() {
        let ev = ProgressEvent {
            job: "risk",
            phase: "Rolling validation".into(),
            step: 2,
            steps: 4,
            done: Some(3),
            total: None,
            elapsed_ms: 12,
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "job": "risk", "phase": "Rolling validation", "step": 2, "steps": 4,
                "done": 3, "total": null, "elapsed_ms": 12
            })
        );
    }

    #[test]
    fn noop_sink_accepts_events() {
        let mut sink = noop();
        let mut r = Reporter::new("risk", 4, &mut sink);
        r.phase("x");
        r.tick(1, Some(1));
        r.finish();
    }
}
