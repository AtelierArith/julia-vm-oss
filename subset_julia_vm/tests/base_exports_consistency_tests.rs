use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn parse_exports(path: &PathBuf) -> Option<BTreeSet<String>> {
    let contents = fs::read_to_string(path).ok()?;
    let mut exports = BTreeSet::new();
    let mut in_export = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("export") {
            in_export = true;
            let rest = trimmed.strip_prefix("export").unwrap_or("");
            for part in rest.split(',') {
                let item = part.trim();
                if !item.is_empty() {
                    exports.insert(item.to_string());
                }
            }
            continue;
        }
        if !in_export {
            continue;
        }
        let without_comment = line.split('#').next().unwrap_or("");
        for part in without_comment.split(',') {
            let item = part.trim();
            if !item.is_empty() {
                exports.insert(item.to_string());
            }
        }
    }
    Some(exports)
}

#[test]
fn base_exports_do_not_exceed_upstream() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let subset_exports_path = manifest_dir.join("src/julia/base/exports.jl");
    let upstream_exports_path = manifest_dir.join("../julia/base/exports.jl");

    let subset_exports = match parse_exports(&subset_exports_path) {
        Some(e) => e,
        None => panic!(
            "SubsetJuliaVM exports.jl not found at {:?}",
            subset_exports_path
        ),
    };

    let upstream_exports = match parse_exports(&upstream_exports_path) {
        Some(e) => e,
        None => {
            // julia submodule not checked out (e.g. worktree or CI without submodules)
            eprintln!(
                "Skipping: upstream julia/base/exports.jl not found at {:?}",
                upstream_exports_path
            );
            return;
        }
    };

    let extras: Vec<_> = subset_exports
        .difference(&upstream_exports)
        .cloned()
        .collect();

    if !extras.is_empty() {
        panic!(
            "SubsetJuliaVM Base exports include identifiers not in upstream Julia: {:?}",
            extras
        );
    }
}
