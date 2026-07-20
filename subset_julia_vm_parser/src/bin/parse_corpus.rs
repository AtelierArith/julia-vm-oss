//! parse_corpus — parse-only entry point for parser corpus differential
//! testing vs upstream Julia (Issue #8614 / #8635).
//!
//! Reads `.jl` file paths (arguments and/or `--files-from LIST`, `-` = stdin),
//! parses each file with the sjulia parser (no lowering, no execution), and
//! writes one TSV record per parse error / panic to stdout
//! (columns: file, span, error_kind, snippet, message). A human-readable
//! summary (file counts, success rate, per-error-kind counts) goes to stderr.
//!
//! Driven by `scripts/parser_corpus_sweep.sh`.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::process;

use subset_julia_vm_parser::corpus::{sweep_source, CorpusRecord, FileOutcome, TSV_HEADER};

const USAGE: &str = "usage: parse_corpus [--files-from LIST|-] [FILE.jl ...]\n\
  Parses each file (parse only) and prints divergence records as TSV to stdout.\n\
  --files-from LIST  read newline-separated file paths from LIST ('-' = stdin)";

fn fail(message: &str) -> ! {
    eprintln!("parse_corpus: {message}");
    process::exit(2);
}

fn collect_files() -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                eprintln!("{USAGE}");
                process::exit(0);
            }
            "--files-from" => {
                let list = args
                    .next()
                    .unwrap_or_else(|| fail("--files-from requires a path ('-' = stdin)"));
                let reader: Box<dyn BufRead> = if list == "-" {
                    Box::new(std::io::stdin().lock())
                } else {
                    let file = std::fs::File::open(&list)
                        .unwrap_or_else(|e| fail(&format!("cannot open {list}: {e}")));
                    Box::new(std::io::BufReader::new(file))
                };
                for line in reader.lines() {
                    let line = line.unwrap_or_else(|e| fail(&format!("cannot read {list}: {e}")));
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        files.push(trimmed.to_string());
                    }
                }
            }
            flag if flag.starts_with("--") => fail(&format!("unknown flag {flag}\n{USAGE}")),
            path => files.push(path.to_string()),
        }
    }
    files
}

fn main() {
    let files = collect_files();
    if files.is_empty() {
        fail(&format!("no input files\n{USAGE}"));
    }

    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    fn emit(out: &mut impl Write, record: &CorpusRecord) {
        writeln!(out, "{}", record.to_tsv())
            .unwrap_or_else(|e| fail(&format!("cannot write to stdout: {e}")));
    }
    writeln!(out, "{TSV_HEADER}").unwrap_or_else(|e| fail(&format!("cannot write to stdout: {e}")));

    let mut ok_files = 0usize;
    let mut error_files = 0usize;
    let mut panic_files = 0usize;
    let mut unreadable_files = 0usize;
    let mut record_count = 0usize;
    let mut kind_counts: BTreeMap<String, usize> = BTreeMap::new();

    for file in &files {
        let source = match std::fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                unreadable_files += 1;
                record_count += 1;
                *kind_counts.entry("ReadError".to_string()).or_default() += 1;
                emit(
                    &mut out,
                    &CorpusRecord {
                        file: file.clone(),
                        span: String::new(),
                        error_kind: "ReadError".to_string(),
                        snippet: String::new(),
                        message: error.to_string(),
                    },
                );
                continue;
            }
        };
        match sweep_source(file, &source) {
            FileOutcome::Ok => ok_files += 1,
            FileOutcome::Errors(records) => {
                error_files += 1;
                for record in &records {
                    record_count += 1;
                    *kind_counts.entry(record.error_kind.clone()).or_default() += 1;
                    emit(&mut out, record);
                }
            }
            FileOutcome::Panic(record) => {
                panic_files += 1;
                record_count += 1;
                *kind_counts.entry(record.error_kind.clone()).or_default() += 1;
                emit(&mut out, &record);
            }
        }
    }
    out.flush()
        .unwrap_or_else(|e| fail(&format!("cannot flush stdout: {e}")));

    let total = files.len();
    let ok_rate = if total > 0 {
        100.0 * ok_files as f64 / total as f64
    } else {
        0.0
    };
    eprintln!(
        "parse_corpus: {total} files | ok {ok_files} ({ok_rate:.2}%) | \
         with parse errors {error_files} | panicked {panic_files} | unreadable {unreadable_files} | \
         {record_count} divergence records"
    );
    let mut kinds: Vec<(&String, &usize)> = kind_counts.iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    for (kind, count) in kinds {
        eprintln!("parse_corpus:   {kind}: {count}");
    }
}
