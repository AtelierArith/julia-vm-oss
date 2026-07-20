//! Compile-phase timing for warm-start performance investigations.

// This module's whole purpose is to print compile-phase timing diagnostics to
// stderr (gated behind the `SJULIA_COMPILE_PROFILE` env var), so the crate-wide
// `#![deny(clippy::print_stderr)]` (lib.rs) does not apply here. Allowing it at
// the module level keeps `cargo clippy --features repl` green (Issue #7969).
#[cfg(feature = "profiling")]
#[allow(clippy::print_stderr)]
mod imp {
    use std::cell::RefCell;
    use std::time::{Duration, Instant};

    const ENV_VAR: &str = "SJULIA_COMPILE_PROFILE";

    /// Cold-start profiling (Issue #10119): a separate opt-in from the warm
    /// `SJULIA_COMPILE_PROFILE` above, so a cold-start investigation doesn't
    /// have to also wade through warm per-compile noise. Enabled by either
    /// `SJULIA_COLD_COMPILE_PROFILE` (any value) or `SJULIA_COMPILE_PROFILE=cold`.
    const COLD_ENV_VAR: &str = "SJULIA_COLD_COMPILE_PROFILE";

    thread_local! {
        static EVENTS: RefCell<Vec<(&'static str, Duration)>> = const { RefCell::new(Vec::new()) };
        static NOTES: RefCell<Vec<(&'static str, String)>> = const { RefCell::new(Vec::new()) };
        static START: RefCell<Option<Instant>> = const { RefCell::new(None) };

        static COLD_EVENTS: RefCell<Vec<(String, Duration)>> = const { RefCell::new(Vec::new()) };
        static COLD_START: RefCell<Option<Instant>> = const { RefCell::new(None) };
    }

    pub(crate) fn reset() {
        if enabled() {
            EVENTS.with(|events| events.borrow_mut().clear());
            NOTES.with(|notes| notes.borrow_mut().clear());
            START.with(|start| *start.borrow_mut() = Some(Instant::now()));
        }
    }

    pub(crate) fn time<T>(label: &'static str, f: impl FnOnce() -> T) -> T {
        if !enabled() {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        EVENTS.with(|events| events.borrow_mut().push((label, elapsed)));
        result
    }

    /// Time a phase and print it immediately to stderr.
    ///
    /// Used for pipeline phases (prelude load, merge, user parse) that run
    /// BEFORE `compile_with_cache` calls `reset()`; recording them in `EVENTS`
    /// would be wiped by the reset, so print right away instead (Issue #6348).
    pub(crate) fn time_immediate<T>(label: &'static str, f: impl FnOnce() -> T) -> T {
        if !enabled() {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        eprintln!(
            "[CompileProfile]   {:<42} {:>9.3} ms (immediate)",
            label,
            elapsed.as_secs_f64() * 1000.0
        );
        result
    }

    pub(crate) fn start(label: &'static str) -> Timer {
        Timer {
            label,
            start: enabled().then(Instant::now),
        }
    }

    pub(crate) fn finish(timer: Timer) {
        let Some(start) = timer.start else {
            return;
        };
        EVENTS.with(|events| events.borrow_mut().push((timer.label, start.elapsed())));
    }

    pub(crate) fn note(label: &'static str, value: impl FnOnce() -> String) {
        if enabled() {
            NOTES.with(|notes| notes.borrow_mut().push((label, value())));
        }
    }

    /// Like [`note`], but print immediately to stderr instead of buffering into
    /// `NOTES`. Used by phases (the Base-cache decode, Issue #9201) that run
    /// BEFORE `compile_with_cache` calls `reset()` — buffered notes would be
    /// wiped by that reset and never surface (same rationale as
    /// [`time_immediate`]).
    pub(crate) fn note_immediate(label: &'static str, value: impl FnOnce() -> String) {
        if enabled() {
            eprintln!("[CompileProfile]   {:<42} {}", label, value());
        }
    }

    pub(crate) fn print_summary(label: &str) {
        if !enabled() {
            return;
        }

        EVENTS.with(|events| {
            let events = events.borrow();
            let wall = START
                .with(|start| *start.borrow())
                .map(|start| start.elapsed())
                .unwrap_or(Duration::ZERO);
            let wall_ms = wall.as_secs_f64() * 1000.0;
            eprintln!(
                "[CompileProfile] {label}: {} events, wall {:.3} ms",
                events.len(),
                wall_ms
            );
            // Print each phase's share of the compile wall so cache-decode /
            // per-section costs are read off directly as a percentage — the
            // number the Performance Decision Protocol thresholds on for
            // Issue #9201 (e.g. `cache.base_cache_decode_total` share > 5%).
            for (event_label, elapsed) in events.iter() {
                let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
                let pct_wall = if wall_ms > 0.0 {
                    elapsed_ms / wall_ms * 100.0
                } else {
                    0.0
                };
                eprintln!(
                    "[CompileProfile]   {:<42} {:>9.3} ms  ({:>5.1}% wall)",
                    event_label, elapsed_ms, pct_wall
                );
            }
        });

        NOTES.with(|notes| {
            for (note_label, value) in notes.borrow().iter() {
                eprintln!("[CompileProfile]   {:<42} {}", note_label, value);
            }
        });
    }

    fn enabled() -> bool {
        std::env::var_os(ENV_VAR).is_some()
    }

    #[derive(Debug)]
    pub(crate) struct Timer {
        label: &'static str,
        start: Option<Instant>,
    }

    // ------------------------------------------------------------------
    // Cold-start profiling (Issue #10119).
    //
    // Cold start (no Base cache, no embedded/persistent prelude Program) is
    // dominated by parsing + lowering the ~65 Base source files and then
    // compiling them from scratch. This is a separate timeline from the warm
    // `[CompileProfile]` one above: it covers `parse_prelude_from_source()`
    // (per-file parse + lower, Issue #10122) and
    // `compile_base_functions_from_source()`, both of which run BEFORE a
    // normal `compile_with_cache` call and either may never run at all on a
    // warm process (embedded/persistent cache hit).
    // ------------------------------------------------------------------

    pub(crate) fn cold_enabled() -> bool {
        if std::env::var_os(COLD_ENV_VAR).is_some() {
            return true;
        }
        std::env::var(ENV_VAR).map(|v| v == "cold").unwrap_or(false)
    }

    pub(crate) fn cold_reset() {
        if cold_enabled() {
            COLD_EVENTS.with(|events| events.borrow_mut().clear());
            COLD_START.with(|start| *start.borrow_mut() = Some(Instant::now()));
        }
    }

    /// Time a cold-start phase and record it for the final [`cold_print_summary`]
    /// rollup. Unlike the warm [`time_immediate`] (which prints right away
    /// because `compile_with_cache` calls `reset()` again later, wiping
    /// anything recorded before it), cold profiling's `cold_reset()` runs
    /// exactly once at the very start of a cold compile, so recording here is
    /// never at risk of being wiped before `cold_print_summary` reads it back
    /// — deferring to one clean end-of-batch table instead of printing (and
    /// then re-printing in the summary) every one of the ~65 Base files twice.
    pub(crate) fn cold_time_immediate<T>(label: impl Into<String>, f: impl FnOnce() -> T) -> T {
        if !cold_enabled() {
            return f();
        }
        let label = label.into();
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        COLD_EVENTS.with(|events| events.borrow_mut().push((label, elapsed)));
        result
    }

    /// Record an ALREADY-MEASURED cold-start phase duration (mirrors
    /// [`cold_time_immediate`], for work timed off the main thread — e.g. the
    /// parallel per-file Base parse (Issue #10122) times each file on its own
    /// spawned thread via a plain `Instant`, then the joining main thread
    /// records every file's duration here so `cold_print_summary`'s rollup
    /// includes them (a thread-local push from the worker thread itself would
    /// die with that thread and never reach the main thread's summary).
    pub(crate) fn cold_record_immediate(label: impl Into<String>, elapsed: Duration) {
        if !cold_enabled() {
            return;
        }
        let label = label.into();
        COLD_EVENTS.with(|events| events.borrow_mut().push((label, elapsed)));
    }

    /// Print a free-form note immediately (mirrors [`note_immediate`]).
    pub(crate) fn cold_note_immediate(note: &str) {
        if cold_enabled() {
            eprintln!("[ColdProfile]   {note}");
        }
    }

    pub(crate) fn cold_print_summary(label: &str) {
        if !cold_enabled() {
            return;
        }
        COLD_EVENTS.with(|events| {
            let events = events.borrow();
            let wall = COLD_START
                .with(|start| *start.borrow())
                .map(|start| start.elapsed())
                .unwrap_or(Duration::ZERO);
            let wall_ms = wall.as_secs_f64() * 1000.0;
            eprintln!(
                "[ColdProfile] {label}: {} events, wall {:.3} ms",
                events.len(),
                wall_ms
            );
            for (event_label, elapsed) in events.iter() {
                let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
                let pct_wall = if wall_ms > 0.0 {
                    elapsed_ms / wall_ms * 100.0
                } else {
                    0.0
                };
                eprintln!(
                    "[ColdProfile]   {:<42} {:>9.3} ms  ({:>5.1}% wall)",
                    event_label, elapsed_ms, pct_wall
                );
            }
        });
    }
}

#[cfg(not(feature = "profiling"))]
mod imp {
    #[inline(always)]
    pub(crate) fn reset() {}

    #[inline(always)]
    pub(crate) fn time<T>(_label: &'static str, f: impl FnOnce() -> T) -> T {
        f()
    }

    #[inline(always)]
    pub(crate) fn time_immediate<T>(_label: &'static str, f: impl FnOnce() -> T) -> T {
        f()
    }

    #[inline(always)]
    pub(crate) fn start(_label: &'static str) -> Timer {
        Timer
    }

    #[inline(always)]
    pub(crate) fn finish(_timer: Timer) {}

    #[inline(always)]
    pub(crate) fn note(_label: &'static str, _value: impl FnOnce() -> String) {}

    #[inline(always)]
    pub(crate) fn note_immediate(_label: &'static str, _value: impl FnOnce() -> String) {}

    #[inline(always)]
    pub(crate) fn print_summary(_label: &str) {}

    #[derive(Debug)]
    pub(crate) struct Timer;

    #[inline(always)]
    pub(crate) fn cold_enabled() -> bool {
        false
    }

    #[inline(always)]
    pub(crate) fn cold_reset() {}

    #[inline(always)]
    pub(crate) fn cold_time_immediate<T>(_label: impl Into<String>, f: impl FnOnce() -> T) -> T {
        f()
    }

    #[inline(always)]
    pub(crate) fn cold_record_immediate(_label: impl Into<String>, _elapsed: std::time::Duration) {}

    #[inline(always)]
    pub(crate) fn cold_note_immediate(_note: &str) {}

    #[inline(always)]
    pub(crate) fn cold_print_summary(_label: &str) {}
}

#[inline(always)]
pub(crate) fn reset() {
    imp::reset();
}

#[inline(always)]
pub(crate) fn time<T>(label: &'static str, f: impl FnOnce() -> T) -> T {
    imp::time(label, f)
}

#[inline(always)]
pub fn time_immediate<T>(label: &'static str, f: impl FnOnce() -> T) -> T {
    imp::time_immediate(label, f)
}

#[inline(always)]
pub(crate) fn start(label: &'static str) -> imp::Timer {
    imp::start(label)
}

#[inline(always)]
pub(crate) fn finish(timer: imp::Timer) {
    imp::finish(timer);
}

#[inline(always)]
pub(crate) fn note(label: &'static str, value: impl FnOnce() -> String) {
    imp::note(label, value);
}

#[inline(always)]
pub(crate) fn note_immediate(label: &'static str, value: impl FnOnce() -> String) {
    imp::note_immediate(label, value);
}

#[inline(always)]
pub(crate) fn print_summary(label: &str) {
    imp::print_summary(label);
}

/// True when cold-start profiling (Issue #10119) is enabled via
/// `SJULIA_COLD_COMPILE_PROFILE` or `SJULIA_COMPILE_PROFILE=cold`. Exposed so
/// callers can skip building an otherwise-unused per-file label when disabled.
#[inline(always)]
pub fn cold_enabled() -> bool {
    imp::cold_enabled()
}

#[inline(always)]
pub fn cold_reset() {
    imp::cold_reset();
}

#[inline(always)]
pub fn cold_time_immediate<T>(label: impl Into<String>, f: impl FnOnce() -> T) -> T {
    imp::cold_time_immediate(label, f)
}

#[inline(always)]
pub fn cold_record_immediate(label: impl Into<String>, elapsed: std::time::Duration) {
    imp::cold_record_immediate(label, elapsed);
}

#[inline(always)]
pub fn cold_note_immediate(note: &str) {
    imp::cold_note_immediate(note);
}

#[inline(always)]
pub fn cold_print_summary(label: &str) {
    imp::cold_print_summary(label);
}
