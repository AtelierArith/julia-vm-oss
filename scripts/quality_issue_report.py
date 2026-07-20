#!/usr/bin/env python3
"""Reproducible GitHub Issue triage and weekly quality metrics (Issue #10452).

The original survey queried a mutable label.  ``triage`` reconstructs label
membership at a historical timestamp by replaying each candidate Issue's label
events, then proposes one transparent root-cause owner.  A committed review TSV
can freeze the canonical class/owner/reason fields while membership and state are
regenerated. It prints TSV to stdout so snapshots remain ordinary diffs.

Only Python's standard library and an authenticated ``gh`` CLI are required.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import pathlib
import re
import statistics
import subprocess
import sys
import time
import urllib.parse
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any, Iterable, Optional


REPO = "AtelierArith/ailujsoi"
UTC = dt.timezone.utc


@dataclass(frozen=True)
class RootClass:
    number: int
    owner: str
    title_pattern: re.Pattern[str]
    issue_refs: tuple[int, ...]


ROOT_CLASSES = (
    RootClass(10, "#10815", re.compile(r"\b(?:aot|juliars|vm.?aot)\b", re.I), (10815,)),
    RootClass(
        9,
        "#10814",
        re.compile(r"typed[- ]loop|transactional|bail(?:out)?|deopt", re.I),
        (10814,),
    ),
    RootClass(
        8,
        "#10813",
        re.compile(
            r"exception (?:type|class|layer)|wrong (?:error|exception)|catchability|"
            r"@test_throws|try.?catch parity",
            re.I,
        ),
        (10813,),
    ),
    RootClass(
        4,
        "#10462",
        re.compile(r"cache|preload|prelude|rehydrat|restor|snapshot|serialize|relocat", re.I),
        (10462, 10438, 10051, 10265),
    ),
    RootClass(
        6,
        "#10464",
        re.compile(
            r"lowering|lowered|destructur|assignment value|tail value|source intent|"
            r"macro expansion|parser",
            re.I,
        ),
        (10464,),
    ),
    RootClass(
        5,
        "#10463",
        re.compile(r"iterator|iterate\b|generator|collect\b|eltype|broadcast|range trait", re.I),
        (10463, 10050),
    ),
    RootClass(
        3,
        "#10461",
        re.compile(
            r"callable|higher[- ]order|\bhof\b|function value|call resolver|"
            r"direct.?call|runtime specializer",
            re.I,
        ),
        (10461,),
    ),
    RootClass(
        2,
        "#10460",
        re.compile(
            r"unionall|typevar|subtyp|typejoin|type object|parametric|promotion|"
            r"type inference|dispatch type",
            re.I,
        ),
        (10460, 10049),
    ),
    RootClass(
        1,
        "#10459",
        re.compile(
            r"name collision|same[- ]name|owner.?scoped|module scope|shadow|"
            r"binding identity|struct identity|bare name|name.?based",
            re.I,
        ),
        (10459, 10279, 10436),
    ),
    RootClass(
        7,
        "#10465",
        re.compile(r"\b(?:audit|ratchet|fixture|test harness|ci|main[- ]red|gate)\b", re.I),
        (10465,),
    ),
)


def parse_time(value: str) -> dt.datetime:
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=UTC)
    return parsed.astimezone(UTC)


def gh_token() -> str:
    result = subprocess.run(
        ["gh", "auth", "token"], check=True, capture_output=True, text=True
    )
    return result.stdout.strip()


class GitHub:
    def __init__(self) -> None:
        self.token = gh_token()

    def get_json(self, url: str) -> tuple[Any, dict[str, str]]:
        request = urllib.request.Request(
            url,
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {self.token}",
                "User-Agent": "ailujsoi-quality-issue-report",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        return self.request_json(request)

    def post_json(self, url: str, payload: dict[str, Any]) -> tuple[Any, dict[str, str]]:
        request = urllib.request.Request(
            url,
            data=json.dumps(payload).encode("utf-8"),
            method="POST",
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
                "User-Agent": "ailujsoi-quality-issue-report",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        return self.request_json(request)

    def request_json(
        self, request: urllib.request.Request
    ) -> tuple[Any, dict[str, str]]:
        for attempt in range(7):
            try:
                with urllib.request.urlopen(request, timeout=60) as response:
                    headers = {
                        key.lower(): value for key, value in response.headers.items()
                    }
                    return json.load(response), headers
            except urllib.error.HTTPError as error:
                if error.code not in (403, 429, 502, 503) or attempt == 6:
                    raise
                retry_after = error.headers.get("Retry-After")
                delay = int(retry_after) if retry_after else min(2**attempt, 30)
                time.sleep(delay)
        raise AssertionError("unreachable retry loop")

    def paged(self, url: str) -> list[dict[str, Any]]:
        rows: list[dict[str, Any]] = []
        while url:
            payload, headers = self.get_json(url)
            if isinstance(payload, dict) and "items" in payload:
                rows.extend(payload["items"])
            elif isinstance(payload, list):
                rows.extend(payload)
            else:
                raise RuntimeError(f"unexpected GitHub response shape from {url}")
            url = next_link(headers.get("link", ""))
        return rows

    def search_issues(self, query: str) -> list[dict[str, Any]]:
        encoded = urllib.parse.urlencode({"q": query, "per_page": 100})
        return self.paged(f"https://api.github.com/search/issues?{encoded}")

    def issue_events(self, number: int) -> list[dict[str, Any]]:
        return self.paged(
            f"https://api.github.com/repos/{REPO}/issues/{number}/events?per_page=100"
        )

    def issue_events_batch(
        self, issues: list[dict[str, Any]], batch_size: int = 40
    ) -> dict[int, list[dict[str, Any]]]:
        query = """
          query($ids: [ID!]!) {
            nodes(ids: $ids) {
              ... on Issue {
                number
                timelineItems(
                  first: 100
                  itemTypes: [LABELED_EVENT, UNLABELED_EVENT, CLOSED_EVENT, REOPENED_EVENT]
                ) {
                  pageInfo { hasNextPage }
                  nodes {
                    __typename
                    ... on LabeledEvent { createdAt label { name } }
                    ... on UnlabeledEvent { createdAt label { name } }
                    ... on ClosedEvent { createdAt }
                    ... on ReopenedEvent { createdAt }
                  }
                }
              }
            }
          }
        """
        result: dict[int, list[dict[str, Any]]] = {}
        for offset in range(0, len(issues), batch_size):
            batch = issues[offset : offset + batch_size]
            payload, _ = self.post_json(
                "https://api.github.com/graphql",
                {"query": query, "variables": {"ids": [row["node_id"] for row in batch]}},
            )
            if payload.get("errors"):
                raise RuntimeError(f"GitHub GraphQL errors: {payload['errors']}")
            for node in payload["data"]["nodes"]:
                timeline = node["timelineItems"]
                if timeline["pageInfo"]["hasNextPage"]:
                    raise RuntimeError(
                        f"Issue #{node['number']} has more than 100 relevant timeline events"
                    )
                events = []
                for item in timeline["nodes"]:
                    typename = item["__typename"]
                    event = {
                        "LabeledEvent": "labeled",
                        "UnlabeledEvent": "unlabeled",
                        "ClosedEvent": "closed",
                        "ReopenedEvent": "reopened",
                    }[typename]
                    row: dict[str, Any] = {
                        "event": event,
                        "created_at": item["createdAt"],
                    }
                    if item.get("label"):
                        row["label"] = {"name": item["label"]["name"]}
                    events.append(row)
                result[int(node["number"])] = events
        return result


def next_link(header: str) -> str:
    for part in header.split(","):
        match = re.match(r'\s*<([^>]+)>;\s*rel="([^"]+)"', part)
        if match and match.group(2) == "next":
            return match.group(1)
    return ""


def labels_at(events: Iterable[dict[str, Any]], cutoff: dt.datetime) -> set[str]:
    labels: set[str] = set()
    for event in sorted(events, key=lambda row: row.get("created_at", "")):
        created = event.get("created_at")
        if not created or parse_time(created) > cutoff:
            break
        name = (event.get("label") or {}).get("name")
        if not name:
            continue
        if event.get("event") == "labeled":
            labels.add(name)
        elif event.get("event") == "unlabeled":
            labels.discard(name)
    return labels


def state_at(
    events: Iterable[dict[str, Any]], cutoff: dt.datetime
) -> tuple[str, Optional[dt.datetime]]:
    state = "open"
    closed_at: Optional[dt.datetime] = None
    for event in sorted(events, key=lambda row: row.get("created_at", "")):
        created = event.get("created_at")
        if not created:
            continue
        timestamp = parse_time(created)
        if timestamp > cutoff:
            break
        if event.get("event") == "closed":
            state = "closed"
            closed_at = timestamp
        elif event.get("event") == "reopened":
            state = "open"
            closed_at = None
    return state, closed_at


def explicit_ref(text: str, issue_refs: tuple[int, ...]) -> Optional[int]:
    for issue in issue_refs:
        if re.search(rf"(?<!\d)#{issue}(?!\d)", text):
            return issue
    return None


def classify(issue: dict[str, Any]) -> tuple[int, str, str]:
    title = issue.get("title") or ""
    body = issue.get("body") or ""
    combined = f"{title}\n{body}"

    # Explicit architecture links are stronger than keyword inference.  Keep
    # the ordered class table deterministic when an Issue names several parents.
    for root in sorted(ROOT_CLASSES, key=lambda item: item.number):
        found = explicit_ref(combined, root.issue_refs)
        if found is not None:
            return root.number, root.owner, f"explicit-ref-#{found}"
    for root in ROOT_CLASSES:
        match = root.title_pattern.search(title)
        if match:
            token = re.sub(r"\s+", " ", match.group(0)).strip().lower()
            return root.number, root.owner, f"title:{token}"

    number = int(issue["number"])
    return 0, f"#{number}", "self-owned/no-structural-match"


def reconstruct_bug_population(
    github: GitHub, created: str, cutoff: dt.datetime
) -> list[dict[str, Any]]:
    candidates = github.search_issues(f"repo:{REPO} is:issue created:{created}")
    candidates = [
        issue for issue in candidates if parse_time(issue["created_at"]) <= cutoff
    ]

    event_map = github.issue_events_batch(candidates)

    def inspect(issue: dict[str, Any]) -> Optional[dict[str, Any]]:
        events = event_map[int(issue["number"])]
        if "bug" not in labels_at(events, cutoff):
            return None
        result = dict(issue)
        result["_events"] = events
        return result

    selected = [result for issue in candidates if (result := inspect(issue)) is not None]
    return sorted(selected, key=lambda row: int(row["number"]))


def tsv_cell(value: object) -> str:
    return str(value).replace("\t", " ").replace("\r", " ").replace("\n", " ")


def reviewed_assignments(path: Optional[str]) -> dict[int, tuple[int, str, str]]:
    if path is None:
        return {}
    rows: dict[int, tuple[int, str, str]] = {}
    with pathlib.Path(path).open(encoding="utf-8") as source:
        header = source.readline().rstrip("\n").split("\t")
        expected = ["issue", "created_at", "state_at_snapshot", "class", "owner", "reason", "title"]
        if header != expected:
            raise ValueError(f"unexpected reviewed TSV header in {path}")
        for line in source:
            fields = line.rstrip("\n").split("\t", 6)
            if len(fields) != 7:
                raise ValueError(f"malformed reviewed TSV row in {path}: {line.rstrip()}")
            number = int(fields[0].removeprefix("#"))
            rows[number] = (int(fields[3]), fields[4], fields[5])
    return rows


def command_triage(args: argparse.Namespace) -> int:
    github = GitHub()
    cutoff = parse_time(args.as_of)
    issues = reconstruct_bug_population(github, args.created, cutoff)
    reviewed = reviewed_assignments(args.reviewed_from)
    lines = ["issue\tcreated_at\tstate_at_snapshot\tclass\towner\treason\ttitle"]
    for issue in issues:
        number = int(issue["number"])
        class_number, owner, reason = reviewed.get(number, classify(issue))
        state, _ = state_at(issue["_events"], cutoff)
        lines.append(
            "\t".join(
                tsv_cell(value)
                for value in (
                    f"#{issue['number']}",
                    issue["created_at"],
                    state,
                    class_number,
                    owner,
                    reason,
                    issue["title"],
                )
            )
        )
    rendered = "\n".join(lines) + "\n"
    if args.output:
        pathlib.Path(args.output).write_text(rendered, encoding="utf-8")
    else:
        sys.stdout.write(rendered)
    print(
        f"triage rows: {len(issues)} (created={args.created}, as_of={args.as_of})",
        file=sys.stderr,
    )
    if args.expect is not None and len(issues) != args.expect:
        print(f"ERROR: expected {args.expect} rows", file=sys.stderr)
        return 1
    return 0


def command_selftest(_: argparse.Namespace) -> int:
    cutoff = parse_time("2026-07-11T03:00:00Z")
    events = [
        {"event": "labeled", "created_at": "2026-07-10T00:00:00Z", "label": {"name": "bug"}},
        {"event": "unlabeled", "created_at": "2026-07-12T00:00:00Z", "label": {"name": "bug"}},
    ]
    assert labels_at(events, cutoff) == {"bug"}
    state_events = [
        {"event": "closed", "created_at": "2026-07-10T01:00:00Z"},
        {"event": "reopened", "created_at": "2026-07-12T01:00:00Z"},
    ]
    assert state_at(state_events, cutoff) == (
        "closed",
        parse_time("2026-07-10T01:00:00Z"),
    )
    assert classify({"number": 1, "title": "cache restore loses globals", "body": ""})[:2] == (4, "#10462")
    assert classify({"number": 2, "title": "odd result", "body": "Related to #10814"})[:2] == (9, "#10814")
    assert classify({"number": 3, "title": "local typo", "body": ""})[:2] == (0, "#3")
    print("quality_issue_report selftest: OK")
    return 0


PREVENTION_TITLE = re.compile(r"prevention|design|audit|tech[- ]?debt|ratchet", re.I)
SILENT_WRONG_TITLE = re.compile(
    r"silent(?:ly)?|wrong (?:result|value|type|identity)|returns? .+ instead|"
    r"mis[- ]?(?:infer|dispatch|resolv|classif)|corrupt",
    re.I,
)
MAIN_RED_TITLE = re.compile(
    r"main[- ]red|red on main|pre-existing test failure on main|main .* (?:fails|failure)",
    re.I,
)


def median_text(values: list[float], digits: int = 1) -> str:
    if not values:
        return "n/a"
    return f"{statistics.median(values):.{digits}f}"


def weekly_row(
    issues: list[dict[str, Any]],
    cutoff: dt.datetime,
    reviewed: Optional[dict[int, tuple[int, str, str]]] = None,
) -> dict[str, object]:
    closed_by_end: list[dict[str, Any]] = []
    open_at_end: list[dict[str, Any]] = []
    rapid = 0
    classes = {number: 0 for number in range(0, 11)}

    for issue in issues:
        created = parse_time(issue["created_at"])
        state, closed = state_at(issue["_events"], cutoff)
        number = int(issue["number"])
        class_number, _, _ = (reviewed or {}).get(number, classify(issue))
        classes[class_number] += 1
        if state == "closed" and closed is not None:
            closed_by_end.append(issue)
            issue["_snapshot_closed_at"] = closed.isoformat()
            if (closed - created).total_seconds() <= 24 * 60 * 60:
                rapid += 1
        else:
            open_at_end.append(issue)

    open_ages = [
        (cutoff - parse_time(issue["created_at"])).total_seconds() / 86400
        for issue in open_at_end
    ]
    main_red_issue_close_hours = []
    for issue in closed_by_end:
        if MAIN_RED_TITLE.search(issue.get("title") or ""):
            main_red_issue_close_hours.append(
                (
                    parse_time(issue["_snapshot_closed_at"])
                    - parse_time(issue["created_at"])
                ).total_seconds()
                / 3600
            )

    return {
        "new_bug": len(issues),
        "closed_by_end": len(closed_by_end),
        "closed_24h": rapid,
        "preventionish": sum(
            1 for issue in issues if PREVENTION_TITLE.search(issue.get("title") or "")
        ),
        "silent_wrong": sum(
            1 for issue in issues if SILENT_WRONG_TITLE.search(issue.get("title") or "")
        ),
        "open_cohort": len(open_at_end),
        "open_age_median_days": median_text(open_ages),
        "main_red_issue_closed": len(main_red_issue_close_hours),
        "main_red_issue_close_median_hours": median_text(main_red_issue_close_hours),
        "classes": classes,
    }


def render_weekly(records: list[tuple[str, dict[str, object]]]) -> str:
    lines = [
        "# Weekly bug-cohort quality baseline",
        "",
        "Generated by `scripts/quality_issue_report.py weekly` for Issue #10452.",
        "Each population replays label events at the UTC end of its window; later",
        "label edits therefore do not rewrite the historical cohort.",
        "",
        "`silent-wrong` and `prevention-shaped` are explicit title heuristics, not",
        "incident-severity labels. `open age` is the age of the window's still-open",
        "cohort at that window end, not the age of the repository's entire backlog.",
        "`main-red issue-close` is an explicit proxy from Issue creation to its",
        "historical close event; it is not the fixing PR merge-to-main timestamp.",
        "",
        "| UTC created window | new bug | closed by end | closed <=24h | prevention-shaped | silent-wrong | open cohort | median open age (days) | main-red issues closed | median issue-close proxy (hours) |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for window, record in records:
        lines.append(
            "| "
            + " | ".join(
                str(value)
                for value in (
                    window,
                    record["new_bug"],
                    record["closed_by_end"],
                    record["closed_24h"],
                    record["preventionish"],
                    record["silent_wrong"],
                    record["open_cohort"],
                    record["open_age_median_days"],
                    record["main_red_issue_closed"],
                    record["main_red_issue_close_median_hours"],
                )
            )
            + " |"
        )

    lines.extend(
        [
            "",
            "## Root-cause recurrence within each weekly cohort",
            "",
            "Class 0 means the symptom Issue remains its own owner because the",
            "reviewed mapping (when available) or the transparent title/reference",
            "classifier found no structural owner.",
            "",
            "| UTC created window | self | C1 | C2 | C3 | C4 | C5 | C6 | C7 | C8 | C9 | C10 |",
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
        ]
    )
    for window, record in records:
        classes = record["classes"]
        assert isinstance(classes, dict)
        values = [window] + [str(classes[number]) for number in range(0, 11)]
        lines.append("| " + " | ".join(values) + " |")

    lines.extend(
        [
            "",
            "These four windows are the baseline, not evidence of improvement.",
            "A later report may claim improvement only by regenerating comparable",
            "full UTC windows and interpreting campaign-driven discovery separately.",
            "",
        ]
    )
    return "\n".join(lines)


def window_cutoff(window: str) -> dt.datetime:
    try:
        _, end = window.split("..", 1)
        return parse_time(f"{end}T23:59:59Z")
    except ValueError as error:
        raise argparse.ArgumentTypeError(f"invalid date window: {window}") from error


def command_weekly(args: argparse.Namespace) -> int:
    github = GitHub()
    reviewed = reviewed_assignments(args.reviewed_from)
    records: list[tuple[str, dict[str, object]]] = []
    for window in args.window:
        cutoff = window_cutoff(window)
        issues = reconstruct_bug_population(github, window, cutoff)
        records.append((window, weekly_row(issues, cutoff, reviewed)))
        print(f"weekly cohort {window}: {len(issues)}", file=sys.stderr)
    rendered = render_weekly(records)
    if args.output:
        pathlib.Path(args.output).write_text(rendered, encoding="utf-8")
    else:
        sys.stdout.write(rendered)
    return 0


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subparsers = result.add_subparsers(dest="command", required=True)

    triage = subparsers.add_parser("triage", help="reconstruct and classify a historical bug population")
    triage.add_argument("--created", required=True, help="GitHub date range, e.g. 2026-07-04..2026-07-11")
    triage.add_argument("--as-of", required=True, help="UTC cutoff timestamp")
    triage.add_argument("--expect", type=int)
    triage.add_argument("--output", help="write the generated TSV to this path")
    triage.add_argument(
        "--reviewed-from",
        help="preserve canonical class/owner/reason assignments from a reviewed TSV",
    )
    triage.set_defaults(func=command_triage)

    weekly = subparsers.add_parser("weekly", help="generate comparable historical bug-cohort metrics")
    weekly.add_argument("--window", action="append", required=True, help="UTC created date range")
    weekly.add_argument("--output", help="write the generated Markdown to this path")
    weekly.add_argument(
        "--reviewed-from",
        help="use canonical reviewed class assignments where Issue numbers overlap",
    )
    weekly.set_defaults(func=command_weekly)

    selftest = subparsers.add_parser("selftest", help="run deterministic parser/classifier controls")
    selftest.set_defaults(func=command_selftest)
    return result


def main() -> int:
    args = parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
