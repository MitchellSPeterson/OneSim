#!/usr/bin/env python3
"""Tests for scripts/ci_select.py (the CI impact selector). Run: python3 scripts/test_ci_select.py"""
import fnmatch
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ci_select as cs  # noqa: E402

# The workspace as of this test (kept in sync by test_graph_matches_cargo_metadata).
GRAPH = {
    "onesim-protocol": {"dir": "crates/onesim-protocol", "deps": set()},
    "onesim-display": {"dir": "crates/onesim-display", "deps": {"onesim-protocol"}},
    "onesim-render": {"dir": "crates/onesim-render", "deps": {"onesim-display", "onesim-protocol"}},
    "onesim-render-gpu": {"dir": "crates/onesim-render-gpu", "deps": {"onesim-display", "onesim-render", "onesim-protocol"}},
    "onesim-host": {"dir": "crates/onesim-host", "deps": {"onesim-protocol", "onesim-render", "onesim-render-gpu"}},
    "onesim-cli": {"dir": "crates/onesim-cli",
                   "deps": {"onesim-host", "onesim-protocol", "onesim-render", "onesim-render-gpu"}},
    "onesim-runtime": {"dir": "crates/onesim-runtime", "deps": set()},
    "onesim-web": {"dir": "crates/onesim-web", "deps": {"onesim-protocol", "onesim-render"}},
    "onesim-web-gpu": {"dir": "crates/onesim-web-gpu", "deps": {"onesim-display", "onesim-render-gpu"}},
}
ALL = set(cs.SELECTABLE)
WEB = set(cs.WEB)
NATIVE = {"swiftui", "compose", "expo"}

# (name, changed paths, checks at the fast level, checks at the premerge level)
MATRIX = [
    ("docs only", ["README.md", "docs/web.md", "LICENSE"], set(), set()),
    ("empty diff", [], set(), set()),
    ("SwiftUI only", ["frameworks/swiftui/Sources/SwiftUI/Button.swift"], {"swiftui"}, {"swiftui", "web-swiftui"}),
    ("SwiftUI example", ["examples/swiftui-tabs/App.swift"], {"swiftui"}, {"swiftui", "web-swiftui"}),
    ("Expo only", ["frameworks/react-native/guest/bridge.js"], {"expo"}, {"expo", "web-expo"}),
    ("Expo example", ["examples/expo-ui/App.tsx"], {"expo"}, {"expo", "web-expo"}),
    ("Compose only", ["frameworks/compose/src/Button.kt"], {"compose"}, {"compose", "web-compose"}),
    ("Android pixel-diff gate", ["scripts/compare-android.sh"], {"compose"}, {"compose"}),
    ("web runtime", ["web/main.js"], WEB, WEB),
    ("web Expo host", ["web/expo/host.js"], {"web-expo"}, {"web-expo"}),
    ("web build script", ["scripts/build-web.sh"], WEB, WEB),
    ("SwiftUI web parity script", ["scripts/compare-web.sh"], {"web-swiftui"}, {"web-swiftui"}),
    ("multiple frameworks", ["frameworks/swiftui/Sources/SwiftUI/Text.swift", "frameworks/compose/src/Text.kt"],
     {"swiftui", "compose"}, {"swiftui", "compose", "web-swiftui", "web-compose"}),
    ("shared renderer", ["crates/onesim-render/src/layout.rs"], {"rust"} | NATIVE, ALL),
    ("shared protocol", ["crates/onesim-protocol/src/wire.rs"], {"rust"} | NATIVE, ALL),
    ("host", ["crates/onesim-host/src/lib.rs"], {"rust"} | NATIVE, {"rust"} | NATIVE | WEB),
    ("web crate", ["crates/onesim-web/src/lib.rs"], {"rust"} | WEB, {"rust"} | WEB),
    ("web GPU crate", ["crates/onesim-web-gpu/src/lib.rs"], {"rust"} | WEB, {"rust"} | WEB),
    ("guest runtime crate", ["crates/onesim-runtime/include/onesim.h"], {"rust", "swiftui", "web-swiftui"},
     {"rust", "swiftui", "web-swiftui"}),
    ("lockfile", ["Cargo.lock"], ALL, ALL),
    ("workflow", [".github/workflows/ci.yml"], set(), ALL),
    ("selector", ["scripts/ci_select.py"], set(), ALL),
    ("selector tests", ["scripts/test_ci_select.py"], set(), set()),
    ("unknown path", ["tools/new-thing.sh"], ALL, ALL),
    ("crate outside the workspace", ["crates/onesim-removed/src/lib.rs"], ALL, ALL),
    ("crate README is docs", ["crates/onesim-render/README.md"], set(), set()),
    ("iOS 27.2 SwiftUI reference", ["tests/fixtures/ios27/showcase-ios/home.png"], set(), {"swiftui"}),
    ("iOS 27.2 device reference", ["tests/fixtures/ios27/devices/iphone-air/home.png"], set(), {"swiftui"}),
    ("iOS 27.2 Expo reference", ["tests/fixtures/ios27/rn-showcase-ios/home.png"], set(), {"expo"}),
    ("iOS 26.5 reference (not gated)", ["tests/fixtures/showcase-ios/home.png", "tests/fixtures/devices/x/home.png"],
     set(), set()),
    ("Android reference", ["tests/fixtures/compose-android/home.png"], set(), {"compose"}),
    ("pixel-diff tool", ["scripts/compare_ios.py"], NATIVE, NATIVE | WEB),
    ("animation diff tool", ["scripts/compare_animation.py", "scripts/test_compare_animation.py"], {"swiftui"}, {"swiftui"}),
    ("animation behavior harness", ["scripts/test-animation-behavior.py"], {"swiftui"}, {"swiftui"}),
    ("fonts", ["assets/fonts/Inter.ttf"], {"rust"} | NATIVE, ALL),
    ("pinned versions", ["docs/compatibility/versions.json"], set(), ALL),
]


def checks(plan):
    return {c for c, on in plan["checks"].items() if on}


class Matrix(unittest.TestCase):
    def test_matrix(self):
        for name, paths, fast, premerge in MATRIX:
            with self.subTest(name):
                self.assertEqual(checks(cs.select(paths, "fast", GRAPH)), fast, "fast")
                self.assertEqual(checks(cs.select(paths, "premerge", GRAPH)), premerge, "premerge")
                # the full level always runs everything, portability included
                self.assertEqual(checks(cs.select(paths, "full", GRAPH)), set(cs.CHECKS), "full")

    def test_premerge_covers_fast(self):
        for name, paths, _, _ in MATRIX:
            with self.subTest(name):
                self.assertLessEqual(checks(cs.select(paths, "fast", GRAPH)), checks(cs.select(paths, "premerge", GRAPH)))

    def test_unknown_diff_runs_everything(self):
        self.assertEqual(checks(cs.select(None, "fast", GRAPH)), ALL)
        self.assertIsNone(cs.select(None, "fast", GRAPH)["rust_packages"])

    def test_portable_only_at_full(self):
        self.assertFalse(cs.select(["tools/x"], "premerge", GRAPH)["checks"]["portable"])

    def test_rust_packages_follow_reverse_deps(self):
        pk = lambda paths, level="fast": cs.select(paths, level, GRAPH)["rust_packages"]
        self.assertEqual(pk(["crates/onesim-web/src/lib.rs"]), ["onesim-web"])
        self.assertEqual(pk(["crates/onesim-render-gpu/src/lib.rs"]),
                         ["onesim-cli", "onesim-host", "onesim-render-gpu", "onesim-web-gpu"])
        self.assertEqual(pk(["crates/onesim-protocol/src/lib.rs"]), sorted(set(GRAPH) - {"onesim-runtime"}))
        # whole workspace: before merge, for lockfiles, and when a non-crate rule selects rust
        self.assertIsNone(pk(["crates/onesim-web/src/lib.rs"], "premerge"))
        self.assertIsNone(pk(["Cargo.lock"]))
        self.assertIsNone(pk(["crates/onesim-web/src/lib.rs", "assets/fonts/x.ttf"]))

    def test_skip_reasons(self):
        plan = cs.select(["frameworks/swiftui/Sources/SwiftUI/Button.swift"], "fast", GRAPH)
        self.assertIn("premerge", plan["skipped"]["web-swiftui"])
        self.assertEqual(plan["skipped"]["compose"], "no changed path affects it")
        self.assertIn("full regression", plan["skipped"]["portable"])
        text = cs.explain(plan, "origin/main")
        self.assertIn("frameworks/swiftui/Sources/SwiftUI/Button.swift: SwiftUI framework", text)
        self.assertIn("Skipped:", text)


class RepoTable(unittest.TestCase):
    """The table must keep up with the repository."""

    def tracked(self):
        out = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=cs.ROOT,
                             capture_output=True, text=True, check=True).stdout
        return [p for p in out.split("\0") if p]

    def test_every_tracked_file_is_mapped(self):
        unmapped = [p for p in self.tracked() if not any(fnmatch.fnmatchcase(p, g) for g, _, _ in cs.RULES)]
        self.assertEqual(unmapped, [], "add a rule to scripts/ci_select.py RULES (and a MATRIX row here)")

    def test_every_rule_matches_something(self):
        files = self.tracked()
        dead = [g for g, _, _ in cs.RULES if not any(fnmatch.fnmatchcase(p, g) for p in files)]
        self.assertEqual(dead, [], "stale rule")

    @unittest.skipUnless(shutil.which("cargo"), "cargo not installed")
    def test_graph_matches_cargo_metadata(self):
        self.assertEqual(cs.cargo_graph(), GRAPH)


class Git(unittest.TestCase):
    """changed_paths on a scratch repository: renames, deletions, merges, empty and multi-commit diffs."""

    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.old_root = cs.ROOT
        cs.ROOT = Path(self.dir)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "ci@example.invalid")
        self.git("config", "user.name", "ci")
        self.write("frameworks/swiftui/A.swift", "a")
        self.write("docs/x.md", "x")
        self.commit("base")
        self.git("checkout", "-q", "-b", "topic")

    def tearDown(self):
        cs.ROOT = self.old_root
        shutil.rmtree(self.dir)

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.dir, capture_output=True, text=True, check=True).stdout

    def write(self, path, text):
        p = Path(self.dir, path)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def commit(self, msg):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", msg)

    def test_empty(self):
        self.assertEqual(cs.changed_paths("main"), [])

    def test_multi_commit(self):
        self.write("frameworks/compose/B.kt", "b")
        self.commit("one")
        self.write("web/main.js", "w")
        self.commit("two")
        self.assertEqual(cs.changed_paths("main"), ["frameworks/compose/B.kt", "web/main.js"])

    def test_rename_maps_both_sides(self):
        Path(self.dir, "frameworks/compose").mkdir()
        self.git("mv", "frameworks/swiftui/A.swift", "frameworks/compose/A.kt")
        self.commit("rename")
        paths = cs.changed_paths("main")
        self.assertEqual(paths, ["frameworks/compose/A.kt", "frameworks/swiftui/A.swift"])
        self.assertEqual(checks(cs.select(paths, "fast", GRAPH)), {"swiftui", "compose"})

    def test_delete(self):
        self.git("rm", "-q", "frameworks/swiftui/A.swift")
        self.commit("delete")
        self.assertEqual(cs.changed_paths("main"), ["frameworks/swiftui/A.swift"])

    def test_merge_from_main_excludes_main_changes(self):
        self.write("frameworks/compose/B.kt", "b")
        self.commit("topic work")
        self.git("checkout", "-q", "main")
        self.write("crates/onesim-render/src/lib.rs", "r")
        self.commit("main moved")
        self.git("checkout", "-q", "topic")
        self.git("merge", "-q", "--no-edit", "main")
        self.assertEqual(cs.changed_paths("main"), ["frameworks/compose/B.kt"])

    def test_worktree_includes_uncommitted_and_untracked(self):
        self.write("docs/x.md", "edited")
        self.write("web/new.js", "n")
        self.assertEqual(cs.changed_paths("main", worktree=True), ["docs/x.md", "web/new.js"])
        self.assertEqual(cs.changed_paths("main"), [])

    def test_bad_base_falls_back(self):
        self.assertIsNone(cs.changed_paths("no-such-ref"))


class GitHub(unittest.TestCase):
    def test_events(self):
        sha = "a" * 40
        self.assertEqual(cs.github_plan("pull_request", "refs/pull/1/merge", {"pull_request": {"base": {"sha": sha}}}),
                         ("fast", sha, "HEAD"))
        self.assertEqual(cs.github_plan("pull_request", "", {"pull_request": {"base": {"sha": sha}}}, "premerge"),
                         ("premerge", sha, "HEAD"))
        self.assertEqual(cs.github_plan("merge_group", "", {"merge_group": {"base_sha": sha, "head_sha": "b" * 40}}),
                         ("premerge", sha, "b" * 40))
        self.assertEqual(cs.github_plan("push", "refs/heads/main", {"before": sha, "after": "c" * 40}),
                         ("fast", sha, "c" * 40))
        self.assertEqual(cs.github_plan("push", "refs/heads/main", {"before": "0" * 40})[:2], ("fast", None))
        self.assertEqual(cs.github_plan("push", "refs/tags/v1.0.0", {})[0], "full")
        self.assertEqual(cs.github_plan("schedule", "refs/heads/main", {})[0], "full")
        self.assertEqual(cs.github_plan("workflow_dispatch", "", {"inputs": {"level": "premerge"}})[0], "premerge")
        self.assertEqual(cs.github_plan("workflow_dispatch", "", {})[0], "full")

    def test_outputs(self):
        import io
        buf = io.StringIO()
        cs.write_outputs(cs.select(["crates/onesim-web/src/lib.rs"], "fast", GRAPH), buf)
        out = dict(l.split("=", 1) for l in buf.getvalue().splitlines())
        self.assertEqual(out["rust"], "true")
        self.assertEqual(out["swiftui"], "false")
        self.assertEqual(out["rust_packages"], "onesim-web")
        self.assertEqual(json.loads(out["plan"])["level"], "fast")


class Verify(unittest.TestCase):
    def run_verify(self, selected, results, plan_result="success"):
        plan = json.dumps({"level": "fast", "checks": {c: c in selected for c in cs.CHECKS}})
        needs = {"plan": {"result": plan_result}}
        needs.update({c: {"result": results.get(c, "success" if c in selected else "skipped")} for c in cs.CHECKS})
        return cs.verify(plan, json.dumps(needs))

    def test_pass(self):
        self.assertEqual(self.run_verify({"swiftui"}, {}), [])

    def test_failed_cancelled_or_missing_selected_job(self):
        for result in ("failure", "cancelled", "skipped"):
            with self.subTest(result):
                self.assertTrue(self.run_verify({"swiftui"}, {"swiftui": result}))

    def test_plan_failed_or_missing(self):
        self.assertTrue(self.run_verify(set(), {}, plan_result="failure"))
        self.assertTrue(cs.verify("", json.dumps({"plan": {"result": "success"}})))
        self.assertTrue(cs.verify("{not json", "{}"))

    def test_job_missing_from_needs(self):
        plan = json.dumps({"level": "fast", "checks": {c: c == "rust" for c in cs.CHECKS}})
        self.assertIn("rust: selected but missing", cs.verify(plan, json.dumps({"plan": {"result": "success"}})))


if __name__ == "__main__":
    os.environ.pop("GIT_DIR", None)
    unittest.main(verbosity=1)
