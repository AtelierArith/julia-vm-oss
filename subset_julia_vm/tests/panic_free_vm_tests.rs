//! Prevention tests for panic-free VM runtime (Issue #2193)
//!
//! These tests ensure the VM runtime code remains panic-free by checking that
//! the count of `.unwrap()` and `.expect()` calls doesn't increase over time.
//!
//! The VM targets iOS App Store where crashes lead to rejection, so we must
//! return proper `VmError` results instead of panicking.

use std::fs;
use std::path::PathBuf;

/// Count occurrences of a pattern in non-test code
///
/// This function reads Rust source files and counts pattern occurrences
/// that appear BEFORE any `#[cfg(test)]` block, excluding:
/// - `unwrap_or`, `unwrap_or_else`, `unwrap_or_default` (safe alternatives)
/// - Doc comments (lines starting with `///`)
fn count_in_non_test_code(dir: &PathBuf, pattern: &str) -> usize {
    let mut total = 0;

    fn visit_dir(dir: &PathBuf, pattern: &str, total: &mut usize) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit_dir(&path, pattern, total);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    if let Ok(contents) = fs::read_to_string(&path) {
                        let count = count_pattern_before_test_block(&contents, pattern);
                        *total += count;
                    }
                }
            }
        }
    }

    visit_dir(dir, pattern, &mut total);
    total
}

/// Count pattern occurrences before `#[cfg(test)]` block
fn count_pattern_before_test_block(contents: &str, pattern: &str) -> usize {
    let mut count = 0;
    let mut in_test_block = false;

    for line in contents.lines() {
        // Once we hit #[cfg(test)], assume the rest is test code
        if line.contains("#[cfg(test)]") {
            in_test_block = true;
        }

        if in_test_block {
            continue;
        }

        // Skip doc comments (which may contain example code)
        let trimmed = line.trim();
        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            continue;
        }

        // For .unwrap(), exclude safe alternatives
        if pattern == ".unwrap()" {
            if line.contains(".unwrap()")
                && !line.contains("unwrap_or")
                && !line.contains("unwrap_or_else")
                && !line.contains("unwrap_or_default")
            {
                count += 1;
            }
        } else if line.contains(pattern) {
            count += 1;
        }
    }

    count
}

/// Test that .unwrap() count in VM runtime code doesn't exceed baseline
///
/// Current baseline: 0 (all .unwrap() calls are in test code or doc comments)
///
/// If this test fails, you have two options:
/// 1. Refactor the code to use `ok_or_else(|| VmError::...)` instead
/// 2. If the .unwrap() is truly safe (e.g., infallible operation), update the baseline
///    with a comment explaining why it's acceptable
#[test]
fn vm_unwrap_count_does_not_regress() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vm_dir = manifest_dir.join("src/vm");

    let unwrap_count = count_in_non_test_code(&vm_dir, ".unwrap()");

    // Baseline: 0 unwrap() calls in non-test VM code
    // All current .unwrap() calls are either:
    // - In #[cfg(test)] blocks
    // - In doc comments (examples)
    const UNWRAP_BASELINE: usize = 0;

    assert!(
        unwrap_count == UNWRAP_BASELINE,
        "VM .unwrap() count increased! Found {} (baseline: {})\n\
         \n\
         To fix this:\n\
         1. Refactor to use `ok_or_else(|| VmError::...)` or `?` operator\n\
         2. Use `unwrap_or_default()` if a default value is appropriate\n\
         3. If truly infallible, add `#[allow(clippy::unwrap_used)]` with justification\n\
         \n\
         See docs/vm/PANIC_FREE.md for approved patterns.",
        unwrap_count,
        UNWRAP_BASELINE
    );
}

/// Test that .expect() count in VM runtime code doesn't exceed baseline
///
/// Current baseline: 1
/// - `exec/mod.rs`: `SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards")`
///   This is acceptable because time going backwards is essentially impossible in practice.
///
/// If this test fails, consider whether the .expect() is truly necessary.
#[test]
fn vm_expect_count_does_not_regress() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vm_dir = manifest_dir.join("src/vm");

    let expect_count = count_in_non_test_code(&vm_dir, ".expect(");

    // Baseline: 1 expect() call in non-test VM code
    // Allowed:
    // - exec/mod.rs: SystemTime duration_since (infallible in practice)
    const EXPECT_BASELINE: usize = 1;

    assert!(
        expect_count <= EXPECT_BASELINE,
        "VM .expect() count increased! Found {} (baseline: {})\n\
         \n\
         To fix this:\n\
         1. Refactor to use `ok_or_else(|| VmError::...)` or `?` operator\n\
         2. If the operation is truly infallible, document why and update baseline\n\
         \n\
         See docs/vm/PANIC_FREE.md for approved patterns.",
        expect_count,
        EXPECT_BASELINE
    );
}

/// Test that panic!() macro is not used in VM runtime code
///
/// Current baseline: 0
#[test]
fn vm_panic_count_does_not_regress() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vm_dir = manifest_dir.join("src/vm");

    let panic_count = count_in_non_test_code(&vm_dir, "panic!(");

    // Baseline: 0 panic!() calls in non-test VM code
    const PANIC_BASELINE: usize = 0;

    assert!(
        panic_count == PANIC_BASELINE,
        "VM panic!() count increased! Found {} (baseline: {})\n\
         \n\
         To fix this:\n\
         1. Return `Err(VmError::...)` instead of panicking\n\
         2. Use `unreachable!()` only for truly unreachable code paths\n\
         \n\
         See docs/vm/PANIC_FREE.md for approved patterns.",
        panic_count,
        PANIC_BASELINE
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_pattern_excludes_test_blocks() {
        let code = r#"
fn main() {
    let x = foo().unwrap();
}

#[cfg(test)]
mod tests {
    fn test_foo() {
        let y = bar().unwrap();
    }
}
"#;
        // Only the first .unwrap() should be counted
        assert_eq!(count_pattern_before_test_block(code, ".unwrap()"), 1);
    }

    #[test]
    fn test_count_pattern_excludes_doc_comments() {
        let code = r#"
/// Example: `foo().unwrap()`
fn main() {
    let x = bar();
}
"#;
        // Doc comment .unwrap() should not be counted
        assert_eq!(count_pattern_before_test_block(code, ".unwrap()"), 0);
    }

    #[test]
    fn test_count_pattern_excludes_safe_unwrap_variants() {
        let code = r#"
fn main() {
    let x = foo().unwrap_or(0);
    let y = bar().unwrap_or_else(|| 1);
    let z = baz().unwrap_or_default();
}
"#;
        // Safe unwrap variants should not be counted
        assert_eq!(count_pattern_before_test_block(code, ".unwrap()"), 0);
    }
}
