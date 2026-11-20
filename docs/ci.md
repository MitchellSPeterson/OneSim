# CI

CI runs only the checks a change affects, and a pull request runs none until it is ready to merge. There are three levels:

| Level | When | What runs | Workflow |
|---|---|---|---|
| **fast** | Locally (`scripts/ci-local.sh --affected --level fast`) and manual runs; no automatic trigger | Compilation, targeted tests, smoke states for the affected checks | `ci.yml` (manual) |
| **premerge** | When a PR has the `ready-to-merge` label, and again on every later commit. PR commits without the label run nothing | The full suites of every affected check: every iOS 27.2 and Android state, fallback fonts, web parity | `premerge.yml`, required status `ci-premerge` |
| **full** | Nightly (08:00 UTC), `v*` tags, manual runs | Every check, whatever changed, including Linux and Windows (`portable`) | `ci.yml` |

Both workflows call `checks.yml`. Its `plan` job runs `scripts/ci_select.py`, which picks the jobs and writes the changed paths, the selected checks, the skipped checks and the reasons into the job summary. The plan job also runs the selector tests, `actionlint` and `node scripts/api-coverage.mjs --check` on every run. Each check job installs its toolchains, then runs `scripts/ci-local.sh --check <name> --level <level>`. Local runs use the same script, so CI and local runs share both the selection and the steps.

The pixel-diff thresholds are the same at every level. The fast level compares fewer states, not with looser limits.

**iOS releases:** only iOS 27.2 (`tests/fixtures/ios27/`) is gated for now. The iOS 26.5 references stay in `tests/fixtures/` and can be checked locally with `SIMCORE_IOS="26.5 27.2" scripts/compare-ios.sh`. CI doesn't check them, and changes to them select no check.

## Checks

| Check | fast | premerge adds |
|---|---|---|
| `rust` | build, test, clippy of the changed crates and their reverse dependencies; when the CLI is affected, the window build, the showcase PNGs and the fake-guest navigation | the whole workspace |
| `swiftui` | wire test, headless counter, iOS 27.2 smoke states (`SIMCORE_SMOKE=1`) | every iOS 27.2 state, Apple and fallback fonts |
| `compose` | headless counter | Android 17 pixel diffs |
| `expo` | React Native smoke states on iOS 27.2 | every iOS 27.2 state (showcase, Expo Router, Expo UI), fallback fonts, Android |
| `web-swiftui` | Wasm clippy, counter build and Node harness | showcase, Chrome presenter, browser vs native parity |
| `web-compose` | counter build and Node harness | showcase and input apps, Chrome presenter, parity |
| `web-expo` | Wasm clippy and build | browser vs native parity |
| `portable` | (full level only) | (full level only): Linux and Windows host, CLI and headless guests |

## How changes map to checks

`RULES` in `scripts/ci_select.py` is a first-match table from path globs to checks. Each check gets the earliest level that needs it. Some examples:

| Changed | fast | premerge |
|---|---|---|
| `docs/**`, `*.md` | none | none |
| `frameworks/swiftui/**`, `examples/swiftui-*` | `swiftui` | `swiftui`, `web-swiftui` |
| `frameworks/compose/**` | `compose` | `compose`, `web-compose` |
| `frameworks/react-native/**`, `examples/expo-*` | `expo` | `expo`, `web-expo` |
| `web/**`, `scripts/build-web.sh` | all three web checks | the same |
| `crates/onesim-render/**` (or protocol, host, CLI) | `rust` (changed crates + dependents), `swiftui`, `compose`, `expo` | every check |
| `crates/onesim-web/**` | `rust` (`-p onesim-web`), web checks | the same |
| `tests/fixtures/ios27/**` | none | the framework's check |
| `tests/fixtures/<set>-ios/**` (iOS 26.5) | none | none |
| `.github/workflows/**`, `scripts/ci_select.py`, `scripts/ci-local.sh`, `docs/compatibility/versions.json` | none (the plan job still lints the workflows and tests the selector) | every check |
| `Cargo.lock`, `Cargo.toml` | every check | every check |
| any path no rule knows | every check | every check |

Crates under `crates/` follow `cargo metadata`: a change selects that crate and every crate that depends on it. Anything the CLI depends on reaches every native comparison, because the CLI drives them all. The web crates reach only the browser checks.

The diff is taken from the merge base, with renames counted as a delete plus an add. For a PR, that is the base commit against the PR's merge commit. If git can't compute the diff, every check runs. `scripts/test_ci_select.py` holds the selector's table tests, including real git repositories with renames, deletions, merges and empty diffs. Its tests also fail when a tracked file matches no rule or a rule matches no file.

## Locally

```sh
scripts/ci-local.sh --explain                 # what would run for this branch, and why (premerge level)
scripts/ci-local.sh --explain --level fast
scripts/ci-local.sh --affected --level fast   # run the per-commit checks
scripts/ci-local.sh --affected                # run the pre-merge checks
scripts/ci-local.sh --all                     # every check (not portable)
scripts/ci-local.sh --check swiftui --level fast
```

Local runs include uncommitted and untracked files, stop at the first failure, and exit nonzero. Toolchains: CONTRIBUTING.md and docs/web.md.

## Merge safety

- **No CI before the label:** a commit on a PR without `ready-to-merge` starts only `premerge.yml`'s gate job (a few seconds on Linux), which marks the required status pending. Nothing builds or tests until someone is trying to merge.
- **`ci-premerge`** (commit status from `premerge.yml`; `ci` carries the same state for the current ruleset): pending on every PR head until the `ready-to-merge` label is added. With the label, it is pending while the checks run, then success only if `checks / verify` passed. `verify` fails if the plan failed, or if any selected job failed, was cancelled, or never ran; an unselected job must be skipped. A new commit gets a fresh pending status and runs again because the label is still there, so a pass on an older commit never counts. The ruleset's "branches must be up to date" setting makes the PR head the exact commit that merges.
- **Cost of the label:** each push to a PR that has the label runs the premerge checks again, so add it once review is done, and remove it to push follow-ups without running CI.
- **Cancellation:** a new commit cancels that PR's previous premerge run only. Other labels never cancel a premerge run. Tags, nightly and manual runs each get their own concurrency group and never cancel each other. Pushes to `main` run nothing: with "branches must be up to date", the merged tree is the one the premerge run checked.
- **Merge queue:** GitHub offers merge queues only on organization-owned repositories, so this one gates on the PR head instead. `premerge.yml` already handles `merge_group`: it runs at the premerge level and sets `ci-premerge` on the queue's commit, so moving the repository to an organization and turning the queue on needs no workflow change.
- **Forks:** the `GITHUB_TOKEN` of a pull request from a fork can't write statuses. A maintainer runs the premerge checks for those PRs another way, such as pushing the branch to this repository.

### GitHub settings

Ruleset "Main CI checks" (Settings > Rules > Rulesets):

1. Required status checks: replace `ci` with **`ci-premerge`**. Keep "Require branches to be up to date before merging" on.
2. Then drop `ci` from `CONTEXTS` in `premerge.yml`.
3. The label exists: `gh label create ready-to-merge --color 0E8A16 --description "Run the pre-merge CI checks"`.
4. Optional, organization-owned repositories only: turn on "Require merge queue" and keep the same required check.

## Timings

Baseline before this layout: successful PR runs between 2026-11-18 and 2026-11-20 took 44–83 min from start to finish. Each run started every macOS job, regardless of what changed: `web` 18–21 min, `swiftui` 15–17 min, `expo` 11–16 min, `rust` 3 min and `compose` 2 min. iOS pixel diffs were most of the `swiftui` and `expo` time (iOS 26.5 + 27.2, Apple and fallback fonts: about 13 min each). The total far exceeds the longest job, so most of the time was spent waiting for macOS runners while several PRs pushed at once.

| Change | Before (per push) | Now, push without the label | Now, with `ready-to-merge` |
|---|---|---|---|
| docs only | all checks, ~45 min | gate job only | plan job only, about 1 min |
| SwiftUI only | all checks, ~45 min | gate job only | `swiftui`, `web-swiftui` |
| shared Rust | all checks, ~45 min | gate job only | every check |

Post-change durations are not yet measured. Record them here from real Actions runs, giving the median and the worst case per change type, before claiming improvements.

Remaining bottlenecks: macOS runner concurrency, the swift.org toolchain install in `web-swiftui`, and Homebrew `binaryen` in each web job.
