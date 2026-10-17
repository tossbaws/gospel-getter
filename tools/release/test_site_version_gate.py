"""Tests for site_version_gate.py. Run from the repository root:
    python3 -m unittest discover -s tools/release
"""

import contextlib
import io
import os
import tempfile
import unittest
import unittest.mock
from pathlib import Path

import site_version_gate as g

ROOT = Path(__file__).resolve().parents[2]


def page(*versions):
    return "".join(f"<p>Version {v}, for Windows and Linux.</p>" for v in versions)


class Parse(unittest.TestCase):
    def test_tags_and_versions_compare_as_numbers(self):
        self.assertEqual(g.parse("v2.16.0"), (2, 16, 0))
        self.assertEqual(g.parse("2.16.0"), (2, 16, 0))
        self.assertLess(g.parse("v2.9.0"), g.parse("v2.10.0"))

    def test_anything_else_is_refused(self):
        for bad in ["", "v2.16", "2.16.0-rc1", "latest", "v2.16.0; rm -rf /"]:
            with self.assertRaises(ValueError, msg=bad):
                g.parse(bad)


class SiteVersion(unittest.TestCase):
    def test_the_one_version_the_page_shows(self):
        self.assertEqual(g.site_version(page("2.16.0", "2.16.0")), "2.16.0")

    def test_two_different_versions_or_none_are_refused(self):
        with self.assertRaises(ValueError):
            g.site_version(page("2.16.0", "2.15.0"))
        with self.assertRaises(ValueError):
            g.site_version("<p>No version here.</p>")

    def test_the_real_site_shows_one_version(self):
        html = (ROOT / "site/index.html").read_text(encoding="utf-8")
        g.parse(g.site_version(html))


class Decide(unittest.TestCase):
    def test_a_release_prep_merge_waits_for_its_release(self):
        deploy, reason = g.decide("2.17.0", ["v2.16.0"])
        self.assertFalse(deploy)
        self.assertIn("deploys when 2.17.0 is published", reason)

    def test_the_published_release_deploys_its_site(self):
        self.assertTrue(g.decide("2.17.0", ["v2.17.0"])[0])

    def test_a_site_only_fix_deploys(self):
        self.assertTrue(g.decide("2.16.0", ["v2.16.0"])[0])

    def test_the_newest_published_release_counts(self):
        # The latest-release API can lag just behind the release that
        # started the deploy, so both are passed.
        self.assertTrue(g.decide("2.17.0", ["v2.16.0", "v2.17.0"])[0])
        self.assertTrue(g.decide("2.17.0", ["v2.17.0", "v2.16.0"])[0])

    def test_numbers_not_strings(self):
        self.assertFalse(g.decide("2.10.0", ["v2.9.0"])[0])
        self.assertTrue(g.decide("2.9.0", ["v2.10.0"])[0])

    def test_nothing_published_never_deploys(self):
        self.assertFalse(g.decide("2.16.0", [])[0])


class Main(unittest.TestCase):
    def run_main(self, html, *args):
        with tempfile.TemporaryDirectory() as d:
            site = Path(d) / "index.html"
            site.write_text(html, encoding="utf-8")
            out, err = io.StringIO(), io.StringIO()
            env = {k: v for k, v in os.environ.items() if k != "GITHUB_STEP_SUMMARY"}
            with unittest.mock.patch.dict(os.environ, env, clear=True), \
                    contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = g.main(["--site", str(site), *args])
            return code, out.getvalue(), err.getvalue()

    def test_prints_the_output_line_and_the_reason(self):
        code, out, err = self.run_main(page("2.17.0"), "--published", "v2.16.0")
        self.assertEqual((code, out), (0, "deploy=false\n"))
        self.assertIn("newest published release is v2.16.0", err)
        code, out, _ = self.run_main(page("2.17.0"), "--published", "v2.16.0",
                                     "--published", "v2.17.0")
        self.assertEqual((code, out), (0, "deploy=true\n"))

    def test_an_empty_published_tag_is_ignored(self):
        code, out, _ = self.run_main(page("2.16.0"), "--published", "v2.16.0", "--published", "")
        self.assertEqual((code, out), (0, "deploy=true\n"))

    def test_the_site_version(self):
        self.assertEqual(self.run_main(page("2.16.0"), "--site-version")[:2], (0, "2.16.0\n"))

    def test_a_bad_tag_or_site_fails_instead_of_deciding(self):
        self.assertEqual(self.run_main(page("2.16.0"), "--published", "main")[:2], (2, ""))
        self.assertEqual(self.run_main(page("2.16.0", "2.15.0"), "--published", "v2.16.0")[:2], (2, ""))

    def test_writes_the_step_summary(self):
        with tempfile.TemporaryDirectory() as d:
            summary = Path(d) / "summary.md"
            site = Path(d) / "index.html"
            site.write_text(page("2.17.0"), encoding="utf-8")
            with unittest.mock.patch.dict(os.environ, {"GITHUB_STEP_SUMMARY": str(summary)}), \
                    contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                g.main(["--site", str(site), "--published", "v2.16.0"])
            self.assertIn("Not deploying: the site shows 2.17.0", summary.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
