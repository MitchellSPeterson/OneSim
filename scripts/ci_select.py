#!/usr/bin/env python3
"""Pick the CI checks a change needs. GitHub Actions and scripts/ci-local.sh share this one selector.

  ci_select.py explain [--level L] [--base REF | --paths]   print the plan and the reason for each check
  ci_select.py plan    [--level L] [--base REF | --paths]   the same plan as JSON
  ci_select.py github  [--level L]                          CI: plan from the event, write $GITHUB_OUTPUT + job summary
  ci_select.py verify                                       CI: fail unless every selected job passed ($PLAN, $NEEDS)

Levels (docs/ci.md):
  fast      every PR commit: compilation, targeted tests, small smoke checks
  premerge  before merge: the full suites of every affected check (pixel diffs, web parity)
  full      nightly, `v*` tags, manual runs: every check, whatever changed

Each path maps to the checks it affects, and the earliest level each one is needed at. Paths under crates/ follow the
Cargo dependency graph (`cargo metadata`). A path that no rule knows runs every check at the fast level: an unmapped
file runs everything rather than nothing. --paths reads the changed paths from stdin (for tests and dry runs).
"""
import argparse
import fnmatch
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEVELS = ["fast", "premerge", "full"]
FAST, PREMERGE = "fast", "premerge"
# CI job ids, in display order. `portable` (Linux + Windows) runs at the full level only.
CHECKS = ["rust", "swiftui", "compose", "expo", "web-swiftui", "web-compose", "web-expo", "portable"]
SELECTABLE = [c for c in CHECKS if c != "portable"]
WEB = ["web-swiftui", "web-compose", "web-expo"]

# Every path selects every check: each at the fast level (unknown paths), or before merge only (CI plumbing).
EVERYTHING_FAST = {c: FAST for c in SELECTABLE}
EVERYTHING_PREMERGE = {c: PREMERGE for c in SELECTABLE}

# First match wins. Globs are fnmatch, so `*` also matches `/`. "crate" defers to the Cargo graph below.
# iOS 26.5 references (tests/fixtures/<set>-ios, tests/fixtures/devices) are not gated: CI checks iOS 27.2 only.
RULES = [
    ("*.md", {}, "documentation"),
    ("LICENSE", {}, "documentation"),
    ("docs/compatibility/versions.json", EVERYTHING_PREMERGE, "pinned target versions (read by the comparison tooling)"),
    ("docs/*", {}, "documentation"),
    (".github/ISSUE_TEMPLATE/*", {}, "issue templates"),
    (".github/workflows/*", EVERYTHING_PREMERGE, "CI workflow: syntax and selector tests now, every check before merge"),
    ("scripts/ci_select.py", EVERYTHING_PREMERGE, "CI selector: selector tests now, every check before merge"),
    ("scripts/test_ci_select.py", {}, "selector tests (run by the plan job)"),
    ("scripts/ci-local.sh", EVERYTHING_PREMERGE, "check steps shared by CI and local runs"),
    ("Cargo.toml", "crate-all", "Cargo workspace"),
    ("Cargo.lock", "crate-all", "Cargo lockfile"),
    ("crates/*", "crate", "Rust crate"),
    ("assets/*", {"rust": FAST, "swiftui": FAST, "compose": FAST, "expo": FAST, **{w: PREMERGE for w in WEB}},
     "bundled fonts and symbols"),
    ("scripts/compare_ios.py", {"swiftui": FAST, "compose": FAST, "expo": FAST, **{w: PREMERGE for w in WEB}},
     "pixel-diff tool shared by every comparison"),
    ("scripts/compare-ios.sh", {"swiftui": FAST, "expo": FAST}, "iOS pixel-diff gate"),
    ("scripts/build-swift-app.sh", {"swiftui": FAST, "web-swiftui": PREMERGE}, "native SwiftUI app build"),
    ("scripts/build-ios-sim.sh", {}, "builds for the real iOS Simulator (reference capture only)"),
    ("frameworks/swiftui/*", {"swiftui": FAST, "web-swiftui": PREMERGE}, "SwiftUI framework"),
    ("examples/swiftui-*", {"swiftui": FAST, "web-swiftui": PREMERGE}, "SwiftUI example"),
    ("frameworks/compose/*", {"compose": FAST, "web-compose": PREMERGE}, "Compose framework"),
    ("examples/compose-*", {"compose": FAST, "web-compose": PREMERGE}, "Compose example"),
    ("frameworks/react-native/*", {"expo": FAST, "web-expo": PREMERGE}, "React Native / Expo framework"),
    ("examples/expo-*", {"expo": FAST, "web-expo": PREMERGE}, "Expo example"),
    ("scripts/compare-expo-*", {"expo": FAST}, "Expo pixel-diff gates"),
    ("scripts/compare-android.sh", {"compose": FAST}, "Android Compose pixel-diff gate"),
    ("tests/fixtures/ios27/rn-*", {"expo": PREMERGE}, "iOS 27.2 Expo reference"),
    ("tests/fixtures/ios27/expo-*", {"expo": PREMERGE}, "iOS 27.2 Expo reference"),
    ("tests/fixtures/ios27/*", {"swiftui": PREMERGE}, "iOS 27.2 SwiftUI reference"),
    ("tests/fixtures/compose-android/*", {"compose": PREMERGE}, "Android Compose reference"),
    ("tests/fixtures/expo-android/*", {"expo": PREMERGE}, "Android Expo reference"),
    ("tests/fixtures/expo-ui-android/*", {"expo": PREMERGE}, "Android Expo reference"),
    ("tests/fixtures/*", {}, "iOS 26.5 reference (not gated: CI checks iOS 27.2 only)"),
    ("web/expo/*", {"web-expo": FAST}, "Expo browser host"),
    ("web/*", {w: FAST for w in WEB}, "browser runtime shared by every web target"),
    ("scripts/build-web.sh", {w: FAST for w in WEB}, "web build shared by every web target"),
    ("scripts/compare-web.sh", {"web-swiftui": FAST}, "SwiftUI web parity gate"),
    ("scripts/compare-web-compose.sh", {"web-compose": FAST}, "Compose web parity gate"),
    ("scripts/compare-web-expo.sh", {"web-expo": FAST}, "Expo web parity gate"),
    ("scripts/serve-web.sh", {}, "local web dev server"),
    ("scripts/api-coverage.mjs", {}, "compatibility docs check (run by the plan job)"),
    ("scripts/*-inventory.mjs", {}, "API inventory generators"),
    ("sdk/*", {}, "TypeScript SDK (no CI gate yet)"),
    ("examples/demokit-integration/*", {}, "integration sample (no CI gate)"),
    (".gitignore", {}, "git metadata"),
]

# What the Rust crates feed. The CLI (and the host under it) drives every native comparison, so anything it
# depends on gets the frameworks' smoke checks on each commit. The web crates only reach the browser builds.
DRIVER_CRATES = {"onesim-cli", "onesim-host"}
WEB_CRATES = {"onesim-web", "onesim-web-gpu"}


def cargo_graph(root=ROOT):
    """{package: {"dir": "crates/x", "deps": {local deps of any kind}}} from `cargo metadata`."""
    meta = json.loads(subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"],
                                     cwd=root, capture_output=True, text=True, check=True).stdout)
    graph = {}
    for p in meta["packages"]:
        rel = Path(p["manifest_path"]).parent.resolve().relative_to(Path(root).resolve()).as_posix()
        graph[p["name"]] = {"dir": rel, "deps": {d["name"] for d in p["dependencies"] if "path" in d}}
    return graph


def reverse_closure(graph, changed):
    """The changed packages plus everything that depends on them, directly or not."""
    out, todo = set(changed), list(changed)
    while todo:
        cur = todo.pop()
        for name, info in graph.items():
            if cur in info["deps"] and name not in out:
                out.add(name)
                todo.append(name)
    return out


def crate_impact(graph, changed):
    """Checks for a set of changed packages, and the packages the rust check must cover."""
    closure = reverse_closure(graph, changed)
    impact = {"rust": FAST}
    if closure & DRIVER_CRATES:
        impact.update({c: FAST for c in ("swiftui", "compose", "expo")})
        impact.update({w: PREMERGE for w in WEB})
    if "onesim-runtime" in closure:  # its C headers are compiled into every SwiftUI guest, native and Wasm
        impact.update({"swiftui": FAST, "web-swiftui": FAST})
    if closure & WEB_CRATES:
        tier = FAST if changed & WEB_CRATES else PREMERGE
        for w in WEB:
            impact[w] = min(impact.get(w, PREMERGE), tier, key=LEVELS.index)
    return impact, closure


def merge(into, impact):
    for check, tier in impact.items():
        into[check] = min(into.get(check, tier), tier, key=LEVELS.index)


def select(paths, level, graph=None):
    """The plan for `paths` (None: the diff is unknown) at `level`. `graph` is loaded lazily when a crate changed."""
    assert level in LEVELS, level
    tiers, rows, crates, workspace = {}, [], set(), False
    if paths is None:
        merge(tiers, EVERYTHING_FAST)
        workspace = True
        rows.append(("(diff unavailable)", "could not compute the change: every check", EVERYTHING_FAST))
    for path in sorted(set(paths or [])):
        rule = next(((imp, why) for glob, imp, why in RULES if fnmatch.fnmatchcase(path, glob)), None)
        if rule is None:
            merge(tiers, EVERYTHING_FAST)
            workspace = True
            rows.append((path, "unmapped path: every check (conservative fallback)", EVERYTHING_FAST))
            continue
        impact, why = rule
        if impact in ("crate", "crate-all"):
            graph = graph if graph is not None else cargo_graph()
            if impact == "crate-all":  # a lockfile or workspace manifest change can touch any package
                names = set(graph)
                workspace = True
            else:
                names = {n for n, info in graph.items() if path == info["dir"] or path.startswith(info["dir"] + "/")}
            if not names:  # a crate that is not (or no longer) a workspace member
                merge(tiers, EVERYTHING_FAST)
                workspace = True
                rows.append((path, "path under crates/ outside every workspace package: every check", EVERYTHING_FAST))
                continue
            if rule[0] == "crate":
                why = f"{why} {', '.join(sorted(names))} (and its dependents)"
            crates |= names
            impact, _ = crate_impact(graph, names)
        elif impact.get("rust"):
            workspace = True
        merge(tiers, impact)
        rows.append((path, why, impact))

    if level == "full":
        selected = {c: True for c in CHECKS}
    else:
        selected = {c: c in tiers and LEVELS.index(tiers[c]) <= LEVELS.index(level) for c in CHECKS}
    packages = None
    if selected["rust"] and level == FAST and not workspace and crates:
        packages = sorted(reverse_closure(graph, crates))
    skipped = {}
    for c in CHECKS:
        if selected[c]:
            continue
        if c == "portable":
            skipped[c] = "full regression only (nightly, `v*` tags, manual runs)"
        elif c in tiers:
            skipped[c] = f"needed before merge; runs at the {tiers[c]} level"
        else:
            skipped[c] = "no changed path affects it"
    return {"level": level, "checks": selected, "tiers": tiers, "rust_packages": packages,
            "paths": sorted(set(paths or [])) if paths is not None else None,
            "rows": [[p, why, imp] for p, why, imp in rows], "skipped": skipped}


# ---- git -----------------------------------------------------------------------------------------------------------
def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout


def changed_paths(base, head="HEAD", worktree=False):
    """Paths changed since the merge base of `base` and `head`. Renames count as a delete plus an add, so both
    sides are mapped. worktree=True (local runs) also takes uncommitted and untracked files. None if git fails."""
    try:
        mb = git("merge-base", base, head).strip()
        out = git("diff", "--name-only", "--no-renames", "-z", mb, *([] if worktree else [head])).split("\0")
        if worktree:
            out += git("ls-files", "--others", "--exclude-standard", "-z").split("\0")
    except subprocess.CalledProcessError as e:
        print(f"warning: git failed ({e.stderr.strip() or e}); running every check", file=sys.stderr)
        return None
    return sorted({p for p in out if p})


def github_plan(event_name, ref, event, level_override=None):
    """(level, base, head) for a GitHub event. base None means every check at the level (no diff to narrow it)."""
    zero = "0" * 40
    if event_name == "pull_request":
        level, base, head = FAST, event["pull_request"]["base"]["sha"], "HEAD"
    elif event_name == "merge_group":
        level, base, head = PREMERGE, event["merge_group"]["base_sha"], event["merge_group"]["head_sha"]
    elif event_name == "push" and ref.startswith("refs/tags/"):
        level, base, head = "full", None, "HEAD"
    elif event_name == "push":  # after merge: lightweight validation of what the push added
        before = event.get("before") or zero
        level, base, head = FAST, (None if before == zero else before), event.get("after") or "HEAD"
    elif event_name == "workflow_dispatch":
        level = (event.get("inputs") or {}).get("level") or "full"
        base, head = "origin/main", "HEAD"
    else:  # schedule (nightly) and anything new: everything
        level, base, head = "full", None, "HEAD"
    return level_override or level, base, head


# ---- output --------------------------------------------------------------------------------------------------------
def explain(plan, base=None, markdown=False):
    lv = plan["level"]
    lines = []
    h = "### " if markdown else ""
    lines.append(f"{h}CI plan: {lv} level" + (f" (diff against {base})" if base else ""))
    paths = plan["paths"]
    if paths is not None and not paths and lv != "full":
        lines.append("No changed paths: nothing beyond the plan job's checks.")
    lines.append("")
    lines.append(f"{h}Changed paths" if markdown else "Changed paths:")
    if lv == "full" and not plan["rows"]:
        lines.append("  (not diffed: the full level runs every check)")
    for p, why, imp in plan["rows"]:
        tags = ", ".join(f"{c} ({t})" for c, t in imp.items()) or "no check"
        lines.append(f"{'- ' if markdown else '  '}{'`' + p + '`' if markdown else p}: {why} -> {tags}")
    lines.append("")
    lines.append(f"{h}Selected" if markdown else "Selected:")
    sel = [c for c in CHECKS if plan["checks"][c]]
    for c in sel:
        extra = ""
        if c == "rust" and plan["rust_packages"]:
            extra = f" (packages: {' '.join(plan['rust_packages'])})"
        elif c == "rust":
            extra = " (whole workspace)"
        lines.append(f"{'- ' if markdown else '  '}{c}{extra}")
    if not sel:
        lines.append(f"{'- ' if markdown else '  '}none (the plan job still runs api-coverage, selector tests, actionlint)")
    lines.append("")
    lines.append(f"{h}Skipped" if markdown else "Skipped:")
    for c, why in plan["skipped"].items():
        lines.append(f"{'- ' if markdown else '  '}{c}: {why}")
    return "\n".join(lines) + "\n"


def write_outputs(plan, fh):
    for c in CHECKS:
        fh.write(f"{c}={'true' if plan['checks'][c] else 'false'}\n")
    fh.write(f"level={plan['level']}\n")
    fh.write(f"rust_packages={' '.join(plan['rust_packages'] or [])}\n")
    slim = {"level": plan["level"], "checks": plan["checks"]}
    fh.write(f"plan={json.dumps(slim, separators=(',', ':'))}\n")


def verify(plan_json, needs_json):
    """Errors (empty = pass) for an aggregate gate: every selected job must have succeeded, the rest skipped.
    A missing plan, a cancelled job or a job that never ran all fail."""
    try:
        plan = json.loads(plan_json) if plan_json else None
        needs = json.loads(needs_json) if needs_json else {}
    except json.JSONDecodeError as e:
        return [f"unreadable plan or needs: {e}"]
    errors = []
    if needs.get("plan", {}).get("result") != "success":
        errors.append(f"plan: {needs.get('plan', {}).get('result', 'missing')} (no trustworthy selection)")
    if not plan or "checks" not in plan:
        return errors + ["plan output missing"]
    for c in CHECKS:
        want = plan["checks"].get(c)
        got = needs.get(c, {}).get("result", "missing")
        if want is None:
            errors.append(f"{c}: not in the plan")
        elif want and got != "success":
            errors.append(f"{c}: selected but {got}")
        elif not want and got not in ("skipped", "success"):
            errors.append(f"{c}: not selected but {got}")
    return errors


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["explain", "plan", "github", "verify"])
    ap.add_argument("--level", choices=LEVELS)
    ap.add_argument("--base", default="origin/main")
    ap.add_argument("--paths", action="store_true", help="read changed paths from stdin")
    a = ap.parse_args(argv)

    if a.command == "verify":
        errors = verify(os.environ.get("PLAN", ""), os.environ.get("NEEDS", ""))
        for e in errors:
            print(f"::error::{e}")
        if not errors:
            print("every selected check passed; the others were skipped by the plan")
        return 1 if errors else 0

    if a.command == "github":
        with open(os.environ["GITHUB_EVENT_PATH"]) as f:
            event = json.load(f)
        level, base, head = github_plan(os.environ["GITHUB_EVENT_NAME"], os.environ.get("GITHUB_REF", ""), event,
                                        a.level)
        paths = None if level == "full" or base is None else changed_paths(base, head)
        plan = select([] if level == "full" else paths, level)
        text = explain(plan, base, markdown=True)
        print(explain(plan, base))
        with open(os.environ["GITHUB_OUTPUT"], "a") as fh:
            write_outputs(plan, fh)
        if os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as fh:
                fh.write(text)
        return 0

    level = a.level or PREMERGE
    if a.paths:
        paths, base = [l.strip() for l in sys.stdin if l.strip()], None
    elif level == "full":
        paths, base = [], None
    else:
        paths, base = changed_paths(a.base, worktree=True), a.base
    plan = select(paths, level)
    if a.command == "plan":
        print(json.dumps(plan, indent=2))
    else:
        print(explain(plan, base), end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
