#!/usr/bin/env python3
"""What .github/workflows/audit.yml's report job tells the `audit` issue,
from the cargo-audit job's result and the JSON reports it uploaded.

Only a run on main reports (anything else: nothing).

- success: close the open `audit` issue, with a comment naming the green
  commit and run.
- failure or cancelled: open an `audit` issue, or comment on the open one,
  listing each lock file's findings; when a lock file has no report (the
  audit couldn't run on it), say so and name the step that failed, so a
  red run is never silent. Such a run closes nothing.

Advisory text (titles, crate names, versions) is untrusted: it reaches
GitHub only through the files this writes, escaped for a Markdown table
cell, never through the workflow's shell or an expression. The workflow
passes fixed paths, the ref and the job's result, and makes the `gh`
calls.

Usage, from the workflow:
    python3 tools/ci/audit_report.py --ref refs/heads/main --result failure \\
        --commit <sha> --run-url <url> --reports <dir> --out <dir>
prints the decision (open-or-comment, close or nothing) and writes
<out>/title.txt and <out>/body.md for open-or-comment, <out>/body.md for
close.
"""

import argparse
import json
import re
import sys
from pathlib import Path

# (lock file's directory, its report's file name, the step that audits
# it), in audit.yml's order. The file names are the ones its audit steps
# write; move both together.
LOCKS = [
    ("src-tauri", "src-tauri.json", "app"),
    ("ui-leptos", "ui-leptos.json", "ui"),
    ("tests/gui", "tests-gui.json", "gui"),
]

# The cargo-audit job's step ids, as its outcome step writes them to
# outcomes.txt, and the step names they stand for. Move both together.
STEPS = {
    "checkout": "actions/checkout",
    "rust": "Install Rust",
    "install": "Install cargo-audit",
    "app": "Audit the app (src-tauri)",
    "ui": "Audit the Leptos UI (ui-leptos)",
    "gui": "Audit the GUI checks (tests/gui)",
}

TITLE_LIMIT = 120
CELL_LIMIT = 200
ADVISORY_ID = re.compile(r"RUSTSEC-\d{4}-\d{4}")
COMMIT = re.compile(r"[0-9a-f]{40}")
CLOSING_LINE = "This issue closes by itself when the audit on main is green again."

# Warning kinds cargo-audit reports, in the order the rows list them.
WARNING_KINDS = ["unsound", "unmaintained", "yanked"]

# What GitHub's Markdown gives a meaning inside a table cell: `|` ends the
# cell, the rest format, link or inline HTML. A backslash before ASCII
# punctuation shows it as itself.
MARKDOWN = set("\\`*_[]<>|~&!#")


def cell(text):
    """Untrusted text as one table cell: on one line, its Markdown escaped,
    no @mention that notifies anyone."""
    text = " ".join(str(text).split())
    if len(text) > CELL_LIMIT:
        text = text[: CELL_LIMIT - 1] + "…"
    out = []
    for char in text:
        if char in MARKDOWN:
            out.append("\\" + char)
        elif char == "@":
            # A word joiner after @ breaks the mention and shows nothing.
            out.append("@⁠")
        elif ord(char) < 32 or ord(char) == 127:
            out.append(" ")
        else:
            out.append(char)
    return "".join(out)


def plain(text):
    """Untrusted text for the issue's title: one line of printable text."""
    return " ".join("".join(c if c.isprintable() else " " for c in str(text)).split())


def load(path):
    """A lock file's report, or None when there's none to read: missing,
    empty or not cargo-audit's JSON (the audit didn't run on it)."""
    try:
        report = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    if not isinstance(report, dict) or "vulnerabilities" not in report:
        return None
    return report


def findings(report):
    """A report's findings, a dict each: id, kind, crate, version, patched,
    title. Vulnerabilities first, then the warnings by kind."""
    rows = []
    for item in (report.get("vulnerabilities") or {}).get("list") or []:
        rows.append(finding(item, "vulnerability"))
    warnings = report.get("warnings") or {}
    kinds = WARNING_KINDS + sorted(k for k in warnings if k not in WARNING_KINDS)
    for kind in kinds:
        for item in warnings.get(kind) or []:
            shown = item.get("kind") or kind
            rows.append(finding(item, shown if shown in WARNING_KINDS else "other warning"))
    return rows


def finding(item, kind):
    advisory = item.get("advisory") or {}
    package = item.get("package") or {}
    versions = item.get("versions") or {}
    patched = versions.get("patched") or []
    if advisory:
        title = advisory.get("title") or ""
    elif kind == "yanked":
        title = "Yanked from crates.io"
    else:
        title = ""
    return {
        "id": advisory.get("id") or "",
        "kind": kind,
        "crate": package.get("name") or advisory.get("package") or "",
        "version": package.get("version") or "",
        "patched": ", ".join(str(p) for p in patched) if advisory else "",
        "title": title,
    }


def advisory_link(advisory_id):
    if ADVISORY_ID.fullmatch(advisory_id):
        return f"[{advisory_id}](https://rustsec.org/advisories/{advisory_id})"
    return cell(advisory_id) if advisory_id else "—"


def table(rows):
    lines = [
        "| Advisory | Kind | Crate | In the lock | Patched | Title |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for row in rows:
        if row["id"]:
            patched = cell(row["patched"]) if row["patched"] else "none yet"
        else:
            patched = "—"
        lines.append(
            "| "
            + " | ".join(
                [
                    advisory_link(row["id"]),
                    row["kind"],
                    cell(row["crate"]),
                    cell(row["version"]),
                    patched,
                    cell(row["title"]) or "—",
                ]
            )
            + " |"
        )
    return lines


def shorten(items, keep):
    shown = items[:keep]
    if len(items) > keep:
        shown.append(f"and {len(items) - keep} more")
    return ", ".join(shown)


def title(rows):
    """`Audit: <crates> (<advisory IDs>)`, under TITLE_LIMIT characters,
    the lists cut short with ", and N more" when they don't fit."""
    crates = list(dict.fromkeys(plain(r["crate"]) for r in rows if r["crate"]))
    ids = list(dict.fromkeys(plain(r["id"]) for r in rows if r["id"]))
    if not ids:
        ids = list(dict.fromkeys(r["kind"] for r in rows))
    for keep in range(max(len(crates), len(ids)), 0, -1):
        text = f"Audit: {shorten(crates, keep)} ({shorten(ids, keep)})"
        if len(text) < TITLE_LIMIT:
            return text
    return text[: TITLE_LIMIT - 2] + "…"


def run_link(run_url):
    run_id = run_url.rstrip("/").rsplit("/", 1)[-1]
    label = f"run {run_id}" if run_id.isdigit() else "the run"
    return f"[{label}]({run_url})"


def failed_steps(outcomes):
    return [STEPS.get(step, step) for step, outcome in outcomes if outcome == "failure"]


def read_outcomes(path):
    """The cargo-audit job's step outcomes, (step id, outcome) in order,
    from the `<id> <outcome>` lines its outcome step writes."""
    try:
        text = Path(path).read_text(encoding="utf-8")
    except OSError:
        return []
    pairs = []
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0] in STEPS:
            pairs.append((parts[0], parts[1]))
    return pairs


def report(ref, result, commit, run_url, reports, outcomes):
    """The decision and the files' text: (decision, title, body). The
    title is None unless the decision is open-or-comment, the body None
    when it's nothing.

    reports: {lock directory: its parsed report, or None}.
    outcomes: the cargo-audit job's [(step id, outcome)]."""
    if ref != "refs/heads/main" or result not in ("success", "failure", "cancelled"):
        return "nothing", None, None
    link = run_link(run_url)
    if result == "success":
        return "close", None, f"The audit on {commit} is green ({link}).\n"

    outcome = dict(outcomes)
    failed = failed_steps(outcomes)
    rows = []
    missing = []
    sections = []
    for lock, _, step in LOCKS:
        sections += [f"### {lock}/Cargo.lock", ""]
        lock_report = reports.get(lock)
        if lock_report is None:
            missing.append(lock)
            how = f" (its outcome: {outcome[step]})" if step in outcome else ""
            sections.append(f"Not audited: no report from the step **{STEPS[step]}**{how}.")
        else:
            found = findings(lock_report)
            rows += found
            sections += table(found) if found else ["No findings."]
        sections.append("")

    if result == "cancelled":
        head = [f"The audit of {commit} was cancelled before it finished ({link})."]
    else:
        head = [f"The audit of {commit} failed ({link})."]
    head.append("")
    if rows:
        issue_title = title(rows)
        if missing:
            head += [f"**Not every lock file was audited**: {', '.join(missing)}.", ""]
    else:
        if result == "cancelled":
            reason, cause = "the run was cancelled", "cancelled"
        elif failed:
            reason, cause = f"the step **{failed[0]}** failed", failed[0]
        else:
            reason, cause = "the job failed and left no report to say why", "no report"
        issue_title = f"Audit: couldn't run ({cause})"
        head += [f"**The audit couldn't run**: {reason}. Nothing below is a finding.", ""]
    # With every report in, the findings say why the steps failed.
    if failed and (missing or not rows):
        head += ["Failed steps: " + ", ".join(f"**{s}**" for s in failed) + ".", ""]

    body = "\n".join(head + sections + [CLOSING_LINE]) + "\n"
    return "open-or-comment", issue_title, body


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--ref", required=True, help="the run's github.ref")
    parser.add_argument("--result", required=True, help="needs.cargo-audit.result")
    parser.add_argument("--commit", required=True)
    parser.add_argument("--run-url", required=True)
    parser.add_argument("--reports", required=True, type=Path, help="the downloaded artifact")
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args(argv)
    if not COMMIT.fullmatch(args.commit):
        parser.error(f"not a commit SHA: {args.commit!r}")
    if not re.fullmatch(r"https://[^\s()\[\]<>]+", args.run_url):
        parser.error(f"not a run URL: {args.run_url!r}")

    reports = {lock: load(args.reports / name) for lock, name, _ in LOCKS}
    outcomes = read_outcomes(args.reports / "outcomes.txt")
    decision, issue_title, body = report(
        args.ref, args.result, args.commit, args.run_url, reports, outcomes
    )
    args.out.mkdir(parents=True, exist_ok=True)
    if issue_title is not None:
        (args.out / "title.txt").write_text(issue_title + "\n", encoding="utf-8")
    if body is not None:
        (args.out / "body.md").write_text(body, encoding="utf-8")
    print(decision)
    return 0


if __name__ == "__main__":
    sys.exit(main())
