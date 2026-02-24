//! Demo FFI function for testing library loading.

use std::io::{self, Write};

/// Demo function to verify library is working.
#[no_mangle]
pub extern "C" fn subset_julia_vm_demo() {
    println!("[subset_julia_vm] hello stdout");
    let _ = writeln!(io::stderr(), "[subset_julia_vm] hello stderr");

    // Force buffer flush
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();

    // Example: demonstrate some processing
    for i in 0..3 {
        println!("[subset_julia_vm] step {}", i);
    }
    let _ = io::stdout().flush();
}
