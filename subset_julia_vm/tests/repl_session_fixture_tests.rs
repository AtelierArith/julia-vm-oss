use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use subset_julia_vm::repl::{REPLResult, REPLSession};
use subset_julia_vm_bytecode::Value;

#[derive(Debug, Deserialize)]
struct ReplSessionFixture {
    name: String,
    description: String,
    steps: Vec<ReplStep>,
}

#[derive(Debug, Deserialize)]
struct ReplStep {
    name: String,
    input: String,
    success: bool,
    #[serde(default)]
    stdout: String,
    display: Option<String>,
    error_contains: Option<String>,
}

fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
    let handler = std::thread::Builder::new()
        .name("repl-session-fixture".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(f)
        .unwrap();
    if let Err(e) = handler.join() {
        std::panic::resume_unwind(e);
    }
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/repl_session")
}

fn load_fixture(path: &Path) -> ReplSessionFixture {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()));
    toml::from_str(&source)
        .unwrap_or_else(|e| panic!("failed to parse fixture {}: {e}", path.display()))
}

fn function_definition_display(input: &str) -> Option<String> {
    let trimmed = input.trim();
    let rest = trimmed.strip_prefix("function ")?;
    let name_end = rest.find('(')?;
    let name = rest[..name_end].trim();
    (!name.is_empty()).then(|| format!("{name} (generic function with 1 method)"))
}

fn suppresses_display(input: &str) -> bool {
    input.trim_end().ends_with(';')
}

fn result_display(result: &REPLResult, input: &str) -> Option<String> {
    if !result.success {
        return None;
    }
    if suppresses_display(input) {
        return None;
    }
    if let Some(display) = function_definition_display(input) {
        return Some(display);
    }
    if let Some(display) = &result.value_display {
        return Some(display.clone());
    }
    result.value.as_ref().map(format_fixture_value)
}

fn format_fixture_value(value: &Value) -> String {
    match value {
        Value::I64(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Str(v) => format!("\"{v}\""),
        Value::Function(f) => format!("{} (generic function)", f.name),
        other => format!("{other:?}"),
    }
}

fn assert_step(fixture_name: &str, index: usize, step: &ReplStep, result: &REPLResult) {
    let label = format!("{fixture_name} step {} ({})", index + 1, step.name);
    assert_eq!(
        result.success, step.success,
        "{label}: success mismatch; error={:?} output={:?}",
        result.error, result.output
    );
    assert_eq!(result.output, step.stdout, "{label}: stdout mismatch");

    if step.success {
        assert_eq!(
            result_display(result, &step.input),
            step.display,
            "{label}: display mismatch"
        );
    } else if let Some(expected) = &step.error_contains {
        let actual = result.error.as_deref().unwrap_or_default();
        assert!(
            actual.contains(expected),
            "{label}: error {actual:?} did not contain {expected:?}"
        );
    }
}

#[test]
fn repl_session_fixtures_match_expected_steps_8714() {
    run_with_large_stack(|| {
        let mut paths: Vec<PathBuf> = fs::read_dir(fixture_dir())
            .expect("failed to read repl_session fixture dir")
            .map(|entry| entry.expect("failed to read fixture dir entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .collect();
        paths.sort();
        assert!(
            !paths.is_empty(),
            "expected at least one repl_session fixture"
        );

        for path in paths {
            let fixture = load_fixture(&path);
            assert!(
                !fixture.steps.is_empty(),
                "{}: fixture has no steps ({})",
                fixture.name,
                fixture.description
            );

            let mut session = REPLSession::new(0);
            for (index, step) in fixture.steps.iter().enumerate() {
                let result = session.eval(&step.input);
                assert_step(&fixture.name, index, step, &result);
            }
        }
    });
}
