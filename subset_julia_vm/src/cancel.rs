use std::sync::atomic::{AtomicBool, Ordering};

static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request() {
    CANCEL_REQUESTED.store(true, Ordering::Relaxed);
}

pub fn reset() {
    CANCEL_REQUESTED.store(false, Ordering::Relaxed);
}

pub fn is_requested() -> bool {
    CANCEL_REQUESTED.load(Ordering::Relaxed)
}
