"""Tests for audit_report.py. Run from the repository root:
    python3 -m unittest discover -s tools/ci

testdata/audit/cookie.json is `cargo audit --deny warnings --no-fetch --json`
(cargo-audit 0.22.2, with audit.yml's src-tauri ignore list) on
src-tauri/Cargo.lock at 52c462155a62d69cc6193e6a591042c9bf16f4e1, which has
cookie 0.18.2. testdata/audit/unsound-unmaintained.json is the same run on
the same lock file without the glib and chrono ignores, and ignoring the
cookie advisory instead: two warnings, red under --deny warnings.
"""

import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path

import audit_report as a

DATA = Path(__file__).parent / "testdata" / "audit"
MAIN = "refs/heads/main"
COMMIT = "855e274c7f4d05438eb4cf561620cfb77a3a5ca1"
RUN = "https://github.com/tossbaws/gospel-getter/actions/runs/123"
CLEAN = {"vulnerabilities": {"found": False, "count": 0, "list": []}, "warnings": {}}
ALL_RAN = [(step, "success") for step in a.STEPS]


def fixture(name):
    return json.loads((DATA / name).read_text(encoding="utf-8"))


def reports(app=CLEAN, ui=CLEAN, gui=CLEAN):
    return {"src-tauri": app, "ui-leptos": ui, "tests/gui": gui}


def vulnerability(crate, advisory_id, title="A title", patched=(">=1.0.1",)):
    return {
        "advisory": {"id": advisory_id, "package": crate, "title": title},
        "versions": {"patched": list(patched), "unaffected": []},
        "package": {"name": crate, "version": "1.0.0"},
    }


def with_vulnerabilities(*items):
    return {"vulnerabilities": {"found": True, "count": len(items), "list": list(items)}, "warnings": {}}


def rows_of(body):
    return [line for line in body.splitlines() if line.startswith("| ") and "---" not in line][1:]


class RedRun(unittest.TestCase):
    def test_a_vulnerability_opens_an_issue_naming_it(self):
        decision, title, body = a.report(
            MAIN, "failure", COMMIT, RUN, reports(app=fixture("cookie.json")), ALL_RAN
        )
        self.assertEqual(decision, "open-or-comment")
        self.assertEqual(title, "Audit: cookie (RUSTSEC-2026-0594)")
        self.assertIn(COMMIT, body)
        self.assertIn(f"[run 123]({RUN})", body)
        self.assertEqual(
            rows_of(body),
            [
                "| [RUSTSEC-2026-0594](https://rustsec.org/advisories/RUSTSEC-2026-0594)"
                " | vulnerability | cookie | 0.18.2 | \\>=0.18.3"
                " | Private cookie nonces can repeat in processes that fork |"
            ],
        )
        self.assertEqual(body.count("No findings."), 2)
        self.assertTrue(body.endswith(a.CLOSING_LINE + "\n"))
        self.assertNotIn("couldn't run", body)

    def test_denied_warnings_are_findings_by_kind(self):
        decision, title, body = a.report(
            MAIN, "failure", COMMIT, RUN, reports(app=fixture("unsound-unmaintained.json")), ALL_RAN
        )
        self.assertEqual(decision, "open-or-comment")
        self.assertEqual(title, "Audit: glib, chrono (RUSTSEC-2024-0429, RUSTSEC-2026-0543)")
        rows = rows_of(body)
        self.assertEqual(len(rows), 2)
        self.assertRegex(rows[0], r"^\| \[RUSTSEC-2024-0429\]\(.*\) \| unsound \| glib \| 0\.18\.5 \| \\>=0\.20\.0 \| ")
        self.assertRegex(rows[1], r"^\| \[RUSTSEC-2026-0543\]\(.*\) \| unmaintained \| chrono \| [^|]+ \| none yet \| ")

    def test_a_yanked_crate_and_another_kind(self):
        report = {
            "vulnerabilities": {"found": False, "count": 0, "list": []},
            "warnings": {
                "yanked": [{"kind": "yanked", "advisory": None, "versions": None,
                            "package": {"name": "foo", "version": "0.1.0"}}],
                "notice": [{"kind": "notice", "advisory": {"id": "RUSTSEC-2026-0001", "title": "Note"},
                            "versions": {"patched": []}, "package": {"name": "bar", "version": "2.0.0"}}],
            },
        }
        _, title, body = a.report(MAIN, "failure", COMMIT, RUN, reports(gui=report), ALL_RAN)
        self.assertEqual(title, "Audit: foo, bar (RUSTSEC-2026-0001)")
        self.assertEqual(
            rows_of(body),
            [
                "| — | yanked | foo | 0.1.0 | — | Yanked from crates.io |",
                "| [RUSTSEC-2026-0001](https://rustsec.org/advisories/RUSTSEC-2026-0001)"
                " | other warning | bar | 2.0.0 | none yet | Note |",
            ],
        )

    def test_findings_from_every_lock_file_each_under_its_own(self):
        _, title, body = a.report(
            MAIN, "failure", COMMIT, RUN,
            reports(app=fixture("cookie.json"), ui=with_vulnerabilities(vulnerability("url", "RUSTSEC-2026-0002"))),
            ALL_RAN,
        )
        self.assertEqual(title, "Audit: cookie, url (RUSTSEC-2026-0594, RUSTSEC-2026-0002)")
        ui = body.index("### ui-leptos/Cargo.lock")
        self.assertLess(body.index("| cookie |"), ui)
        self.assertGreater(body.index("| url |"), ui)

    def test_the_title_stays_under_120_characters(self):
        items = [vulnerability(f"crate-number-{n}", f"RUSTSEC-2026-{n:04d}") for n in range(20)]
        _, title, _ = a.report(MAIN, "failure", COMMIT, RUN, reports(app=with_vulnerabilities(*items)), ALL_RAN)
        self.assertLess(len(title), 120)
        self.assertRegex(title, r"^Audit: crate-number-0, .*, and \d+ more \(RUSTSEC-2026-0000, .*, and \d+ more\)$")
        # What fits is kept whole.
        _, short, _ = a.report(
            MAIN, "failure", COMMIT, RUN, reports(app=with_vulnerabilities(*items[:2])), ALL_RAN
        )
        self.assertEqual(short, "Audit: crate-number-0, crate-number-1 (RUSTSEC-2026-0000, RUSTSEC-2026-0001)")

    def test_a_title_too_long_even_for_one_is_cut(self):
        item = vulnerability("x" * 200, "RUSTSEC-2026-0003")
        _, title, _ = a.report(MAIN, "failure", COMMIT, RUN, reports(app=with_vulnerabilities(item)), ALL_RAN)
        self.assertLess(len(title), 120)
        self.assertTrue(title.endswith("…"))


class UntrustedText(unittest.TestCase):
    HOSTILE = "Pipes | and\nnewlines\r\n| [link](https://x.test) <b>bold</b> `code` *em* @someone ![img](y) \\"

    def test_advisory_text_cant_break_the_table_or_format(self):
        item = vulnerability("evil|crate\n", "RUSTSEC-2026-0004\n| x", title=self.HOSTILE, patched=[">=1 | <2\n"])
        item["package"]["version"] = "1.0|\n0"
        _, title, body = a.report(MAIN, "failure", COMMIT, RUN, reports(app=with_vulnerabilities(item)), ALL_RAN)
        rows = rows_of(body)
        self.assertEqual(len(rows), 1)
        row = rows[0]
        # Six cells: every pipe but the seven separators is escaped.
        self.assertEqual(len([i for i, c in enumerate(row) if c == "|" and row[i - 1 : i] != "\\"]), 7)
        for raw in ["\n", "<b>", "`code`", "*em*", "[link]", "![img]", "@someone"]:
            self.assertNotIn(raw, row)
        self.assertIn("\\<b\\>bold\\</b\\>", row)
        self.assertIn("\\[link\\](https://x.test)", row)
        self.assertIn("@\u2060someone", row)
        # An ID that isn't one is shown escaped, not linked.
        self.assertNotIn("rustsec.org", row)
        self.assertIn("RUSTSEC-2026-0004 \\| x", row)
        # The body keeps its shape: no line but the table's starts a cell.
        self.assertEqual(len([l for l in body.splitlines() if l.startswith("|")]), 3)
        self.assertNotIn("\n", title)
        self.assertNotIn("\r", title)

    def test_a_long_title_is_cut(self):
        self.assertEqual(len(a.cell("y" * 500)), a.CELL_LIMIT)


class GreenRun(unittest.TestCase):
    def test_all_three_clean_closes(self):
        decision, title, body = a.report(MAIN, "success", COMMIT, RUN, reports(), ALL_RAN)
        self.assertEqual(decision, "close")
        self.assertIsNone(title)
        self.assertEqual(body, f"The audit on {COMMIT} is green ([run 123]({RUN})).\n")

    def test_success_closes_whatever_the_reports_say(self):
        # The job's result is the audit's: a green audit closes even when
        # its upload failed.
        decision, _, _ = a.report(MAIN, "success", COMMIT, RUN, reports(None, None, None), [])
        self.assertEqual(decision, "close")


class CouldntRun(unittest.TestCase):
    def test_no_reports_at_all_names_the_failed_step(self):
        outcomes = [("checkout", "success"), ("rust", "success"), ("install", "failure"),
                    ("app", "skipped"), ("ui", "failure"), ("gui", "failure")]
        decision, title, body = a.report(MAIN, "failure", COMMIT, RUN, reports(None, None, None), outcomes)
        self.assertEqual(decision, "open-or-comment")
        self.assertEqual(title, "Audit: couldn't run (Install cargo-audit)")
        self.assertIn("**The audit couldn't run**: the step **Install cargo-audit** failed.", body)
        self.assertIn("Failed steps: **Install cargo-audit**, **Audit the Leptos UI (ui-leptos)**", body)
        self.assertIn("no report from the step **Audit the app (src-tauri)** (its outcome: skipped)", body)
        self.assertEqual(body.count("Not audited"), 3)
        self.assertTrue(body.endswith(a.CLOSING_LINE + "\n"))

    def test_a_cancelled_run_reports_and_closes_nothing(self):
        decision, title, body = a.report(MAIN, "cancelled", COMMIT, RUN, reports(None, None, None), [])
        self.assertEqual(decision, "open-or-comment")
        self.assertEqual(title, "Audit: couldn't run (cancelled)")
        self.assertIn("was cancelled before it finished", body)
        self.assertIn("**The audit couldn't run**: the run was cancelled.", body)

    def test_a_cancelled_run_never_closes_even_with_clean_reports(self):
        decision, _, _ = a.report(MAIN, "cancelled", COMMIT, RUN, reports(), ALL_RAN)
        self.assertEqual(decision, "open-or-comment")

    def test_a_failure_with_clean_reports_still_reports(self):
        outcomes = ALL_RAN[:-1] + [("gui", "failure")]
        decision, title, body = a.report(MAIN, "failure", COMMIT, RUN, reports(), outcomes)
        self.assertEqual(decision, "open-or-comment")
        self.assertEqual(title, "Audit: couldn't run (Audit the GUI checks (tests/gui))")
        self.assertIn("couldn't run", body)

    def test_one_lock_file_without_a_report_beside_findings(self):
        outcomes = ALL_RAN[:-1] + [("gui", "failure")]
        _, title, body = a.report(
            MAIN, "failure", COMMIT, RUN, reports(app=fixture("cookie.json"), gui=None), outcomes
        )
        self.assertEqual(title, "Audit: cookie (RUSTSEC-2026-0594)")
        self.assertIn("**Not every lock file was audited**: tests/gui.", body)
        self.assertIn("Failed steps: **Audit the GUI checks (tests/gui)**.", body)


class NotMain(unittest.TestCase):
    def test_another_ref_or_result_reports_nothing(self):
        for ref, result in [("refs/heads/feature", "failure"), ("refs/pull/1/merge", "success"),
                            (MAIN, "skipped"), (MAIN, "")]:
            self.assertEqual(a.report(ref, result, COMMIT, RUN, reports(), ALL_RAN), ("nothing", None, None))


class Files(unittest.TestCase):
    def run_main(self, files, result="failure", ref=MAIN):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "reports").mkdir()
            for name, text in files.items():
                (tmp / "reports" / name).write_text(text, encoding="utf-8")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                a.main(["--ref", ref, "--result", result, "--commit", COMMIT, "--run-url", RUN,
                        "--reports", str(tmp / "reports"), "--out", str(tmp / "out")])
            written = {p.name: p.read_text(encoding="utf-8") for p in (tmp / "out").iterdir()}
            return out.getvalue().strip(), written

    def test_the_red_path_writes_the_title_and_body(self):
        clean = json.dumps(CLEAN)
        decision, written = self.run_main({
            "src-tauri.json": (DATA / "cookie.json").read_text(encoding="utf-8"),
            "ui-leptos.json": clean, "tests-gui.json": clean,
            "outcomes.txt": "checkout success\napp failure\n",
        })
        self.assertEqual(decision, "open-or-comment")
        self.assertEqual(written["title.txt"], "Audit: cookie (RUSTSEC-2026-0594)\n")
        self.assertIn("| cookie | 0.18.2 |", written["body.md"])

    def test_empty_or_broken_json_is_no_report(self):
        decision, written = self.run_main({"src-tauri.json": "", "ui-leptos.json": "{not json",
                                           "tests-gui.json": "[]"})
        self.assertEqual(decision, "open-or-comment")
        self.assertEqual(written["body.md"].count("Not audited"), 3)
        self.assertEqual(written["title.txt"], "Audit: couldn't run (no report)\n")

    def test_the_green_path_writes_only_the_comment(self):
        decision, written = self.run_main({}, result="success")
        self.assertEqual(decision, "close")
        self.assertEqual(sorted(written), ["body.md"])

    def test_nothing_writes_nothing(self):
        decision, written = self.run_main({}, ref="refs/heads/other")
        self.assertEqual((decision, written), ("nothing", {}))

    def test_a_malformed_commit_or_url_is_refused(self):
        for args in (["--commit", "abc"], ["--run-url", "https://x.test/a b"]):
            argv = ["--ref", MAIN, "--result", "failure", "--commit", COMMIT, "--run-url", RUN,
                    "--reports", ".", "--out", "."]
            argv[argv.index(args[0]) + 1] = args[1]
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                a.main(argv)


if __name__ == "__main__":
    unittest.main()
