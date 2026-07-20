#!/usr/bin/env python3
"""Report differential fuzz findings and optionally create GitHub issues.

Reads JSONL emitted by scripts/differential_fuzz_runner.py, filters out known
fingerprints from docs/vm/DIFFERENTIAL_FUZZ_KNOWN_FINDINGS.tsv, writes a markdown
report for new findings, and optionally creates one GitHub issue per new
fingerprint.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jsonl", required=True)
    parser.add_argument("--known", default="docs/vm/DIFFERENTIAL_FUZZ_KNOWN_FINDINGS.tsv")
    parser.add_argument("--out-md", required=True)
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", "AtelierArith/ailujsoi"))
    parser.add_argument("--create-issues", action="store_true")
    parser.add_argument("--label", action="append", default=["bug", "prevention"])
    return parser.parse_args()


def load_known(path: Path) -> set[str]:
    if not path.exists():
        return set()
    known: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines()[1:]:
        if not line.strip():
            continue
        known.add(line.split("\t", 1)[0])
    return known


def github_known_fingerprints(repo: str, candidates: set[str]) -> set[str]:
    known: set[str] = set()
    for fingerprint in sorted(candidates):
        proc = subprocess.run(
            [
                "gh",
                "issue",
                "list",
                "--repo",
                repo,
                "--state",
                "all",
                "--search",
                fingerprint,
                "--json",
                "number",
                "--limit",
                "1",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if proc.returncode == 0 and proc.stdout.strip() not in ("", "[]"):
            known.add(fingerprint)
    return known


def load_new_findings(jsonl: Path, known: set[str]) -> list[dict[str, object]]:
    findings = []
    seen = set()
    for line in jsonl.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        if row.get("status") != "fail":
            continue
        fingerprint = str(row["fingerprint"])
        if fingerprint in known or fingerprint in seen:
            continue
        seen.add(fingerprint)
        findings.append(row)
    return findings


def clip(text: str, limit: int = 4000) -> str:
    if len(text) <= limit:
        return text
    return text[:limit] + "\n... <truncated>"


def issue_body(row: dict[str, object]) -> str:
    upstream = row["upstream"]
    sjulia = row["sjulia"]
    assert isinstance(upstream, dict)
    assert isinstance(sjulia, dict)
    source = str(row.get("shrunk_source") or row.get("source") or "")
    return f"""Found by differential fuzzing (Issue #8717, parent #8692).

Fingerprint: `{row['fingerprint']}`
Seed: `{row['seed']}`
Case: `{row['case_index']}`
Failure kind: `{row['failure_kind']}`

## MWE

```julia
{source.rstrip()}
```

## julia vs sjulia

| runner | status | exception | stdout | stderr |
|---|---|---|---|---|
| upstream julia | `{upstream.get('status', '')}` | `{upstream.get('exception_kind', '')}` | <pre>{clip(str(upstream.get('stdout', '')))}</pre> | <pre>{clip(str(upstream.get('stderr', '')))}</pre> |
| sjulia | `{sjulia.get('status', '')}` | `{sjulia.get('exception_kind', '')}` | <pre>{clip(str(sjulia.get('stdout', '')))}</pre> | <pre>{clip(str(sjulia.get('stderr', '')))}</pre> |

## Triage

- If upstream Julia is valid and sjulia errors: label `unsupported-feature` when
  sjulia cannot run the construct, or `bug` when it runs but produces wrong
  output.
- Add this fingerprint to `docs/vm/DIFFERENTIAL_FUZZ_KNOWN_FINDINGS.tsv` when
  the finding is triaged.
"""


def write_report(path: Path, findings: list[dict[str, object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = ["# Differential Fuzz Findings", "", f"New findings: {len(findings)}", ""]
    for row in findings:
        lines.append(f"## {row['fingerprint']} — {row['failure_kind']}")
        lines.append("")
        lines.append(issue_body(row))
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def create_issue(repo: str, labels: list[str], row: dict[str, object]) -> None:
    title = f"differential fuzz finding {row['fingerprint']}: {row['failure_kind']}"
    cmd = ["gh", "issue", "create", "--repo", repo, "--title", title]
    for label in labels:
        cmd.extend(["--label", label])
    cmd.extend(["--body-file", "-"])
    subprocess.run(cmd, input=issue_body(row), text=True, check=True, cwd=ROOT)


def main() -> int:
    args = parse_args()
    findings = load_new_findings(Path(args.jsonl), load_known(ROOT / args.known))
    if args.create_issues and findings:
        existing = github_known_fingerprints(args.repo, {str(row["fingerprint"]) for row in findings})
        findings = [row for row in findings if str(row["fingerprint"]) not in existing]
    write_report(Path(args.out_md), findings)
    if args.create_issues:
        for row in findings:
            create_issue(args.repo, args.label, row)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
