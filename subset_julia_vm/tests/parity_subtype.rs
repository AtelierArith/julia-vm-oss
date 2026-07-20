mod common;

use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct SubtypeParityCorpus {
    definitions: Option<String>,
    cases: Vec<SubtypeCase>,
}

#[derive(Debug, Deserialize)]
struct SubtypeCase {
    name: String,
    left: String,
    right: String,
    expected: bool,
}

fn load_corpus() -> SubtypeParityCorpus {
    toml::from_str(include_str!("fixtures/subtype_parity/corpus.toml"))
        .expect("subtype parity corpus should parse")
}

fn subtype_script(corpus: &SubtypeParityCorpus) -> String {
    let mut script = String::new();
    if let Some(definitions) = &corpus.definitions {
        script.push_str(definitions);
        script.push('\n');
    }
    for case in &corpus.cases {
        script.push_str("println((");
        script.push_str(&case.left);
        script.push_str(") <: (");
        script.push_str(&case.right);
        script.push_str("))\n");
    }
    script.push_str("true\n");
    script
}

fn parse_bool_lines(output: &str, expected_len: usize, source: &str) -> Vec<bool> {
    let parsed = output
        .lines()
        .filter_map(|line| match line.trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        parsed.len(),
        expected_len,
        "{source} produced {} boolean lines, expected {expected_len}. Output:\n{output}",
        parsed.len()
    );
    parsed
}

fn upstream_julia_results(script: &str) -> Option<Vec<bool>> {
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
        "upstream julia subtype parity script failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Some(parse_bool_lines(
        &String::from_utf8_lossy(&output.stdout),
        script.matches("println((").count(),
        "upstream julia",
    ))
}

#[test]
fn subtype_parity_corpus_matches_upstream_julia_issue_8439() {
    let corpus = load_corpus();
    assert!(
        !corpus.cases.is_empty(),
        "subtype parity corpus must contain at least one case"
    );
    let script = subtype_script(&corpus);

    let (_, sjulia_output) = common::compile_and_run_program_direct(&script, 0);
    let sjulia_results = parse_bool_lines(&sjulia_output, corpus.cases.len(), "sjulia");
    let expected_results = corpus
        .cases
        .iter()
        .map(|case| case.expected)
        .collect::<Vec<_>>();
    assert_eq!(
        sjulia_results, expected_results,
        "sjulia subtype results must match corpus expectations before upstream comparison"
    );

    let Some(upstream_results) = upstream_julia_results(&script) else {
        eprintln!("skipping upstream Julia subtype parity comparison: julia binary not found");
        return;
    };

    for (idx, case) in corpus.cases.iter().enumerate() {
        assert_eq!(
            sjulia_results[idx], upstream_results[idx],
            "subtype parity mismatch for {}: ({}) <: ({})",
            case.name, case.left, case.right
        );
    }
}
