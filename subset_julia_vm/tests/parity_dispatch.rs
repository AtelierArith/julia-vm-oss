//! Dispatch parity harness against upstream Julia (Issue #8547, parent #8438).
//!
//! Method-selection analogue of `parity_subtype.rs` (Issue #8439 / PR #8517):
//! the corpus in `tests/fixtures/dispatch_parity/corpus.toml` defines small
//! method tables in which every method returns a unique string marker, plus
//! call-signature queries. Each group is rendered into one Julia script that
//! prints, per query, which method won (behavior-level `which`) — or the
//! sentinel `MethodError` when the call must throw (no matching method, or an
//! upstream ambiguity error). The identical script runs through:
//!
//! - sjulia's full pipeline (parse → lower → compile → VM). Every query is
//!   evaluated twice: as a `#static` direct call at top level (exercises the
//!   compile-time resolution path — `CallResolved` / `CallTypedDispatch` from
//!   `inference_core/dispatch_resolver.rs`) and as a `#dynamic` closure-wrapped
//!   call behind a HOF (forces the runtime dispatcher). Compile-time and
//!   runtime answers must agree (Issue #6836) and both must match upstream.
//! - upstream `julia --startup-file=no` (one fresh process per group as the
//!   sandbox), when a `julia` binary is on PATH; skipped cleanly otherwise.
//!   The corpus `expected` field records the upstream answer, so the sjulia
//!   side is validated even without julia; the live upstream run guards the
//!   recorded answers against staleness.
//!
//! Corpus coverage (see corpus.toml groups):
//! - diagonal `where T` methods vs untyped fallbacks (`diagonal_where`)
//! - bounded `where T<:Number` / `T<:Integer` ranking (`bounded_where`)
//! - typed varargs `f(xs::T...) where T` vs `f(xs...)` (`typed_varargs`)
//! - keyword-forwarding fallbacks `f(xs...; kws...)` (`keyword_forwarding`)
//! - `Type{T}` singletons (`type_singletons`)
//! - `Union` arguments vs abstract/concrete methods (`union_arguments`)
//! - ambiguity + tie-breaker methods (`ambiguity`)
//! - abstract hierarchy and parametric struct params (`struct_hierarchy`)
//! - no matching method → MethodError (`no_method`)
//! - loose-match negative MethodError oracle cells (`negative_methoderror_9567`)
//!
//! Mismatch policy: any sjulia answer that differs from the recorded upstream
//! answer fails the test unless the case carries `allow_mismatch = NNNN`
//! citing a GitHub issue. An allowlisted case that starts matching again fails
//! the test so stale entries get removed. A live upstream answer that differs
//! from `expected` always fails (stale corpus).
//!
//! Deferred shapes (grow the corpus in follow-ups): structural
//! `which`/`Base.morespecific`/`isambiguous` queries against the resolver
//! API, module-qualified methods, invoke/invokelatest, `Vararg{T,N}` with
//! fixed N, and keyword-argument-type-driven selection.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct DispatchParityCorpus {
    groups: Vec<DispatchGroup>,
}

#[derive(Debug, Deserialize)]
struct DispatchGroup {
    name: String,
    definitions: String,
    cases: Vec<DispatchCase>,
}

#[derive(Debug, Deserialize)]
struct DispatchCase {
    name: String,
    call: String,
    expected: String,
    /// GitHub issue number documenting a known sjulia divergence.
    #[serde(default)]
    allow_mismatch: Option<u64>,
}

/// Prefix of machine-readable result lines; everything else (warnings, banner
/// output) is ignored when parsing.
const RESULT_PREFIX: &str = "DP8547";

/// Runner shared by every group script: canonicalizes thrown errors so both
/// runtimes report a plain `MethodError` marker for no-method and ambiguity
/// failures.
const RUNNER: &str = r#"function dp_try_8547(f)
    try
        return string(f())
    catch err
        if err isa MethodError
            return "MethodError"
        end
        return "ERROR"
    end
end
"#;

fn load_corpus() -> DispatchParityCorpus {
    toml::from_str(include_str!("fixtures/dispatch_parity/corpus.toml"))
        .expect("dispatch parity corpus should parse")
}

fn expects_error(case: &DispatchCase) -> bool {
    case.expected == "MethodError" || case.expected == "ERROR"
}

/// Static (direct-call) lines are emitted only for cases that neither expect
/// an error nor are allowlisted: an uncaught runtime error from a direct call
/// would abort the rest of the group script.
fn emits_static_line(case: &DispatchCase) -> bool {
    !expects_error(case) && case.allow_mismatch.is_none()
}

fn group_script(group: &DispatchGroup) -> String {
    let mut script = String::new();
    script.push_str(RUNNER);
    script.push_str(&group.definitions);
    if !script.ends_with('\n') {
        script.push('\n');
    }
    for case in &group.cases {
        if emits_static_line(case) {
            let _ = writeln!(
                script,
                "println(\"{RESULT_PREFIX} {}#static \", string({}))",
                case.name, case.call
            );
        }
        let _ = writeln!(
            script,
            "println(\"{RESULT_PREFIX} {}#dynamic \", dp_try_8547(() -> {}))",
            case.name, case.call
        );
    }
    script.push_str("true\n");
    script
}

/// Parses `DP8547 <case>#<variant> <answer>` lines into a
/// `case#variant → answer` map.
fn parse_result_lines(output: &str) -> BTreeMap<String, String> {
    let mut results = BTreeMap::new();
    for line in output.lines() {
        let Some(rest) = line.trim().strip_prefix(RESULT_PREFIX) else {
            continue;
        };
        let rest = rest.trim_start();
        if let Some((key, answer)) = rest.split_once(' ') {
            results.insert(key.to_string(), answer.trim().to_string());
        }
    }
    results
}

/// Runs one group script through sjulia's cached pipeline (parse → lower →
/// compile with the thread-local Base cache → VM) without panicking, so a
/// group that fails to compile or run is reported as per-case mismatches
/// instead of aborting the harness.
fn run_sjulia_group(script: &str) -> Result<BTreeMap<String, String>, String> {
    use subset_julia_vm::compile::host_support::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    let program = parse_and_lower(script).map_err(|err| format!("pipeline error: {err}"))?;
    let compiled = compile_with_cache(&program).map_err(|err| format!("compile error: {err:?}"))?;
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let run_result = vm.run();
    let output = vm.get_output().to_string();
    match run_result {
        Ok(_) => Ok(parse_result_lines(&output)),
        Err(err) => Err(format!("runtime error: {err}\npartial output:\n{output}")),
    }
}

/// Runs one group script under upstream julia. Returns `None` when no julia
/// binary is available (harness skips the upstream comparison).
fn run_upstream_group(group: &DispatchGroup, script: &str) -> Option<BTreeMap<String, String>> {
    let output = match Command::new("julia")
        .arg("--startup-file=no")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => panic!("failed to spawn upstream julia: {err}"),
    };
    assert!(
        output.status.success(),
        "upstream julia failed on dispatch parity group '{}' with status {:?}\nscript:\n{script}\nstdout:\n{}\nstderr:\n{}",
        group.name,
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Some(parse_result_lines(&String::from_utf8_lossy(&output.stdout)))
}

#[derive(Debug)]
struct Mismatch {
    group: String,
    case: String,
    variant: String,
    call: String,
    expected: String,
    actual: String,
    allow_mismatch: Option<u64>,
}

impl Mismatch {
    fn render(&self) -> String {
        format!(
            "dispatch parity mismatch [{}/{}#{}]\n  call: {}\n  expected (upstream julia): {}\n  sjulia: {}",
            self.group, self.case, self.variant, self.call, self.expected, self.actual
        )
    }
}

fn case_variants(case: &DispatchCase) -> Vec<&'static str> {
    if emits_static_line(case) {
        vec!["static", "dynamic"]
    } else {
        vec!["dynamic"]
    }
}

/// Compares sjulia answers for one group against the recorded upstream
/// answers. A failed group run (`Err`) marks every case as mismatching.
fn collect_group_mismatches(
    group: &DispatchGroup,
    sjulia: &Result<BTreeMap<String, String>, String>,
) -> Vec<Mismatch> {
    let mut mismatches = Vec::new();
    for case in &group.cases {
        for variant in case_variants(case) {
            let actual = match sjulia {
                Ok(results) => results
                    .get(&format!("{}#{variant}", case.name))
                    .cloned()
                    .unwrap_or_else(|| "<missing output line>".to_string()),
                Err(err) => format!("<group failed: {err}>"),
            };
            if actual != case.expected {
                mismatches.push(Mismatch {
                    group: group.name.clone(),
                    case: case.name.clone(),
                    variant: variant.to_string(),
                    call: case.call.clone(),
                    expected: case.expected.clone(),
                    actual,
                    allow_mismatch: case.allow_mismatch,
                });
            }
        }
    }
    mismatches
}

#[test]
fn dispatch_parity_corpus_matches_upstream_julia_issue_8547() {
    let corpus = load_corpus();
    assert!(
        !corpus.groups.is_empty(),
        "dispatch parity corpus must contain at least one group"
    );

    let mut failures = Vec::new();
    let mut allowlisted = Vec::new();
    let mut stale_allowlist = Vec::new();
    let mut julia_available = true;

    for group in &corpus.groups {
        assert!(
            !group.cases.is_empty(),
            "dispatch parity group '{}' must contain at least one case",
            group.name
        );
        let script = group_script(group);

        // sjulia vs recorded upstream answers (always runs).
        let sjulia_results = run_sjulia_group(&script);
        let mismatches = collect_group_mismatches(group, &sjulia_results);
        let mismatching_cases: Vec<&str> = mismatches.iter().map(|m| m.case.as_str()).collect();
        for case in &group.cases {
            if case.allow_mismatch.is_some() && !mismatching_cases.contains(&case.name.as_str()) {
                stale_allowlist.push(format!(
                    "{}/{} matches upstream again; remove allow_mismatch = {} from the corpus",
                    group.name,
                    case.name,
                    case.allow_mismatch.unwrap_or_default()
                ));
            }
        }
        for mismatch in mismatches {
            match mismatch.allow_mismatch {
                Some(issue) => allowlisted.push(format!(
                    "{} (allowlisted, Issue #{issue})",
                    mismatch.render()
                )),
                None => failures.push(mismatch.render()),
            }
        }

        // Live upstream julia vs recorded answers (guards corpus staleness);
        // skipped when julia is unavailable.
        if !julia_available {
            continue;
        }
        let Some(upstream_results) = run_upstream_group(group, &script) else {
            eprintln!("skipping upstream Julia dispatch parity comparison: julia binary not found");
            julia_available = false;
            continue;
        };
        for case in &group.cases {
            for variant in case_variants(case) {
                let key = format!("{}#{variant}", case.name);
                let actual = upstream_results
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| "<missing output line>".to_string());
                assert_eq!(
                    actual, case.expected,
                    "stale dispatch parity corpus: upstream julia answers '{actual}' for \
                     {}/{}#{variant} ({}), but the corpus records '{}' — update expected",
                    group.name, case.name, case.call, case.expected
                );
            }
        }
    }

    for warning in &allowlisted {
        eprintln!("{warning}");
    }
    assert!(
        stale_allowlist.is_empty(),
        "stale dispatch parity allowlist entries:\n{}",
        stale_allowlist.join("\n")
    );
    assert!(
        failures.is_empty(),
        "dispatch parity mismatches (file an issue and allowlist with its number, \
         or fix the divergence):\n{}",
        failures.join("\n\n")
    );
}

/// Proves the harness reports a divergence: a synthetic corpus entry whose
/// recorded answer is deliberately wrong must produce a mismatch (and an
/// entry with the correct answer must not). Runs only the sjulia side, so it
/// also covers machines without julia.
#[test]
fn dispatch_parity_harness_detects_artificial_mismatch_issue_8547() {
    let group = DispatchGroup {
        name: "artificial".to_string(),
        definitions: "dpartificial8547(x) = \"real_answer\"\n".to_string(),
        cases: vec![
            DispatchCase {
                name: "artificial_wrong".to_string(),
                call: "dpartificial8547(1)".to_string(),
                expected: "wrong_answer".to_string(),
                allow_mismatch: None,
            },
            DispatchCase {
                name: "artificial_right".to_string(),
                call: "dpartificial8547(1)".to_string(),
                expected: "real_answer".to_string(),
                allow_mismatch: None,
            },
        ],
    };
    let script = group_script(&group);
    let results = run_sjulia_group(&script);
    let mismatches = collect_group_mismatches(&group, &results);
    let cases: Vec<(&str, &str)> = mismatches
        .iter()
        .map(|m| (m.case.as_str(), m.variant.as_str()))
        .collect();
    assert_eq!(
        cases,
        vec![
            ("artificial_wrong", "static"),
            ("artificial_wrong", "dynamic")
        ],
        "harness must flag exactly the deliberately wrong case on both paths; got {mismatches:?}"
    );
}
