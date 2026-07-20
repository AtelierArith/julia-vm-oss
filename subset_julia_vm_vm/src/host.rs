//! Integration services needed by the standalone interpreter crate.

use std::sync::OnceLock;

pub trait VmHost: Sync {
    fn is_cancel_requested(&self) -> bool;
    fn package_file(&self, normalized_path: &str) -> Option<&'static str>;
}

static HOST: OnceLock<&'static dyn VmHost> = OnceLock::new();

pub fn install(host: &'static dyn VmHost) {
    let _ = HOST.set(host);
}

pub fn get() -> Option<&'static dyn VmHost> {
    HOST.get().copied()
}
