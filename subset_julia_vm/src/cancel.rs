use std::sync::atomic::{AtomicBool, Ordering};

static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request() {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn reset() {
    CANCEL_REQUESTED.store(false, Ordering::SeqCst);
}

pub fn is_requested() -> bool {
    CANCEL_REQUESTED.load(Ordering::SeqCst)
}
