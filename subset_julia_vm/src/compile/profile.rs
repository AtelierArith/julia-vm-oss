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

    thread_local! {
        static EVENTS: RefCell<Vec<(&'static str, Duration)>> = const { RefCell::new(Vec::new()) };
        static NOTES: RefCell<Vec<(&'static str, String)>> = const { RefCell::new(Vec::new()) };
        static START: RefCell<Option<Instant>> = const { RefCell::new(None) };
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
            eprintln!(
                "[CompileProfile] {label}: {} events, wall {:.3} ms",
                events.len(),
                wall.as_secs_f64() * 1000.0
            );
            for (event_label, elapsed) in events.iter() {
                eprintln!(
                    "[CompileProfile]   {:<42} {:>9.3} ms",
                    event_label,
                    elapsed.as_secs_f64() * 1000.0
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
    pub(crate) fn print_summary(_label: &str) {}

    #[derive(Debug)]
    pub(crate) struct Timer;
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
pub(crate) fn time_immediate<T>(label: &'static str, f: impl FnOnce() -> T) -> T {
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
pub(crate) fn print_summary(label: &str) {
    imp::print_summary(label);
}
