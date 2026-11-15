"""Tests for superseded_caches.py. Run from the repository root:
    python3 -m unittest discover -s tools/ci

The keys are main's real ones (gh cache list --ref refs/heads/main), as
listed on 2026-11-15 before its superseded entries were deleted.
"""

import io
import json
import unittest

import superseded_caches as s

CONFIG = "d36664e4029526d3d071669ad59be620af59cf58b1fc4aa6a116290655d00f06"

# id, createdAt, sizeInBytes, key
MAIN = [
    (10549252256, "2026-11-15T09:19:35.499892Z", 9026541,
     "build-tools-signer-Linux-tauri-cli-2.12.4-eb2eff0c74a3a0ee"),
    (10549240817, "2026-11-15T09:19:07.543598Z", 128976512,
     "v0-rust-gui-windows-Windows_NT-x64-1325795d-badc5e60"),
    (10549071520, "2026-11-15T09:11:20.617927Z", 86704975,
     "build-tools-trunk-Windows-trunk-0.21.14-wasm-bindgen-0.2.129-wasm-opt-version_123-b36c0d58621e7edb"),
    (10425464803, "2026-11-12T16:24:41.364332Z", 164392468,
     "v0-rust-gui-linux-Linux-x64-872dfffb-badc5e60"),
    (10548982825, "2026-11-15T09:07:14.678307Z", 120010311,
     "build-tools-trunk-Linux-trunk-0.21.14-wasm-bindgen-0.2.129-wasm-opt-version_123-eb0f17aefc644c96"),
    (10324344174, "2026-11-10T22:42:03.442841Z", 4197080,
     "node-cache-Linux-x64-npm-0e004ddfea3f56b2513f818362bc4b21c178f20b512dfb50c1f04e1c5d55d604"),
    (10324564920, "2026-11-10T22:47:20.339444Z", 35389870,
     "build-tools-bundler-Linux-tauri-cli-2.12.4-68647f0612a34399"),
    (10276012728, "2026-11-10T04:59:46.456323Z", 42924748,
     "build-tools-bundler-Windows-tauri-cli-2.12.4-e897b7cca18798ed"),
    (10548445571, "2026-11-15T08:41:17.467832Z", 1428761668,
     "v0-rust-linux-Linux-x64-872dfffb-d449d622"),
    (10548332819, "2026-11-15T08:35:59.52514Z", 844602379,
     f"v0-rust-{CONFIG}-frontend-tests-Linux-x64-872dfffb-d449d622"),
    (10548382923, "2026-11-15T08:38:20.252841Z", 1750591280,
     f"v0-rust-{CONFIG}-build-and-test-Linux-x64-872dfffb-d449d622"),
    (10548572601, "2026-11-15T08:47:22.254817Z", 1427684601,
     "v0-rust-windows-Windows_NT-x64-1325795d-d449d622"),
    (10548405994, "2026-11-15T08:39:23.14912Z", 892953426,
     f"v0-rust-{CONFIG}-test-windows-Windows_NT-x64-1325795d-d3301805"),
    (10526831285, "2026-11-14T16:02:44.206313Z", 1749812513,
     f"v0-rust-{CONFIG}-build-and-test-Linux-x64-872dfffb-7af57b98"),
    (10526674963, "2026-11-14T15:56:32.966472Z", 879979698,
     "v0-rust-linux-Linux-x64-872dfffb-7af57b98"),
    (10526739643, "2026-11-14T15:58:42.447487Z", 844699007,
     f"v0-rust-{CONFIG}-frontend-tests-Linux-x64-872dfffb-7af57b98"),
]


def caches(rows=MAIN):
    return [dict(id=i, createdAt=at, sizeInBytes=size, key=key) for i, at, size, key in rows]


def ids(found):
    return [cache["id"] for cache in found]


class Families(unittest.TestCase):
    def test_a_key_loses_its_hashes_and_keeps_its_names_and_versions(self):
        for key, want in [
            ("v0-rust-linux-Linux-x64-872dfffb-d449d622", "v0-rust-linux-Linux-x64"),
            (f"v0-rust-{CONFIG}-build-and-test-Linux-x64-872dfffb-7af57b98",
             "v0-rust-build-and-test-Linux-x64"),
            ("v0-rust-gui-windows-Windows_NT-x64-1325795d-badc5e60",
             "v0-rust-gui-windows-Windows_NT-x64"),
            ("build-tools-signer-Linux-tauri-cli-2.12.4-eb2eff0c74a3a0ee",
             "build-tools-signer-Linux-tauri-cli-2.12.4"),
            ("build-tools-trunk-Linux-trunk-0.21.14-wasm-bindgen-0.2.129-wasm-opt-version_123-"
             "eb0f17aefc644c96",
             "build-tools-trunk-Linux-trunk-0.21.14-wasm-bindgen-0.2.129-wasm-opt-version_123"),
            ("node-cache-Linux-x64-npm-0e004ddfea3f56b2513f818362bc4b21c178f20b512dfb50c1f04e1c5d55d604",
             "node-cache-Linux-x64-npm"),
        ]:
            with self.subTest(key=key):
                self.assertEqual(s.family(key), want)

    def test_each_platform_and_job_is_a_family_of_its_own(self):
        families = {s.family(cache["key"]) for cache in caches()}
        self.assertEqual(len(families), 13, sorted(families))
        self.assertNotEqual(
            s.family("build-tools-trunk-Linux-trunk-0.21.14-wasm-bindgen-0.2.129-wasm-opt-version_123-1"),
            s.family("build-tools-trunk-Windows-trunk-0.21.14-wasm-bindgen-0.2.129-wasm-opt-version_123-1"),
        )

    def test_a_new_tool_version_is_a_new_family_never_superseding_the_old(self):
        # A key's versions are part of its family: an entry for another
        # Trunk is left to expire, never deleted as superseded.
        self.assertNotEqual(
            s.family("build-tools-signer-Linux-tauri-cli-2.12.4-eb2eff0c74a3a0ee"),
            s.family("build-tools-signer-Linux-tauri-cli-2.12.5-0123456789abcdef"),
        )


class Superseded(unittest.TestCase):
    def test_mains_list_loses_only_its_three_older_lockfile_entries(self):
        old = s.superseded(caches())
        self.assertEqual(ids(old), [10526674963, 10526739643, 10526831285])
        self.assertTrue(all(cache["key"].endswith("-7af57b98") for cache in old))

    def test_after_that_nothing_is_superseded(self):
        kept = [row for row in MAIN if not row[3].endswith("-7af57b98")]
        self.assertEqual(s.superseded(caches(kept)), [])

    def test_a_cargo_config_change_supersedes_the_old_config_hash_too(self):
        new = "0" * 63 + "1"
        rows = [
            (1, "2026-11-14T16:02:44Z", 10, f"v0-rust-{CONFIG}-build-and-test-Linux-x64-872dfffb-d449d622"),
            (2, "2026-11-16T09:00:00Z", 10, f"v0-rust-{new}-build-and-test-Linux-x64-872dfffb-d449d622"),
        ]
        self.assertEqual(ids(s.superseded(caches(rows))), [1])

    def test_the_newest_and_any_saved_with_it_are_never_deleted(self):
        rows = [
            (1, "2026-11-14T10:00:00Z", 10, "v0-rust-linux-Linux-x64-872dfffb-aaaaaaaa"),
            (2, "2026-11-15T10:00:00.5Z", 10, "v0-rust-linux-Linux-x64-872dfffb-bbbbbbbb"),
            (3, "2026-11-15T10:00:00.500000Z", 10, "v0-rust-linux-Linux-x64-872dfffb-cccccccc"),
        ]
        self.assertEqual(ids(s.superseded(caches(rows))), [1])
        # Nor a family's only entry, however old.
        self.assertEqual(s.superseded(caches(rows[:1])), [])

    def test_saved_times_compare_as_times_not_text(self):
        # gh trims trailing zeros from the fraction (.52514Z), so the newer
        # of two can sort first as text.
        rows = [
            (1, "2026-11-15T08:35:59.9Z", 10, "node-cache-Linux-x64-npm-aaaaaaaa"),
            (2, "2026-11-15T08:35:59.52514Z", 10, "node-cache-Linux-x64-npm-bbbbbbbb"),
        ]
        self.assertEqual(ids(s.superseded(caches(rows))), [2])

    def test_main_prints_what_to_delete_and_the_totals(self):
        out, err = io.StringIO(), io.StringIO()
        status = s.main(io.StringIO(json.dumps(caches())), out, err)
        self.assertEqual(status, 0)
        self.assertEqual(
            out.getvalue().splitlines(),
            [
                "10526674963 879979698 v0-rust-linux-Linux-x64-872dfffb-7af57b98",
                f"10526739643 844699007 v0-rust-{CONFIG}-frontend-tests-Linux-x64-872dfffb-7af57b98",
                f"10526831285 1749812513 v0-rust-{CONFIG}-build-and-test-Linux-x64-872dfffb-7af57b98",
            ],
        )
        self.assertEqual(
            err.getvalue(), "3 of 16 caches superseded, 3474491218 of 10410707077 bytes\n"
        )


if __name__ == "__main__":
    unittest.main()
