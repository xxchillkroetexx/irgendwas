---
name: feature
description: Run the feature pipeline for one new feature or larger change (spec → tests → implementation → gate → review → fix loop). Use when the user runs /feature <name>.
argument-hint: <feature-name>  (kebab-case; an existing spec name in docs/specs/ to extend it)
disable-model-invocation: true
---

Run the pipeline for feature `$ARGUMENTS`. The main session orchestrates; the agents do the work. Run stages **sequentially**; never run two agents that write code at the same time.

## 0. Preconditions
- Read `AGENTS.md` and the entries in `docs/decisions.md` for the areas the feature touches.
- Check that `git status` is clean and the gate passes on it, so the feature's diff is reviewable on its own. If not, stop and say what's wrong.
- Work on a branch off `dev` (e.g. `feature/$ARGUMENTS`); create it if you are on `dev` or `main`.

## 1. Spec
- If `docs/specs/$ARGUMENTS.md` is missing (or the user asked for a change to an existing spec), run the **spec-writer** agent with the feature name and the user's request.
- If the spec is `status: draft`: show the user its summary, the design decisions and the open questions, and **stop**. Continue only after the user has answered the open questions and approved; then set `status: approved` in the spec.

## 2. Tests
Run **test-writer** with the feature name. Check its report: every new or changed AC is mapped, and the new tests compile and fail. If any AC is untestable as written, go back to the user and fix the spec first.

## 3. Implementation
Run **leptos-implementer** with the feature name. If it stops because of a spec/test conflict, bring the conflict to the user; don't resolve it yourself by editing tests.

## 4. Gate (main session verifies; don't trust the report alone)
Run fmt check, clippy `-D warnings`, `cargo leptos test` and `cargo leptos end-to-end`. If anything fails, send the failure output back to the implementer (SendMessage to the same agent) and repeat. After 3 failed rounds, stop and report to the user.

Also check `git diff --stat` to confirm that no test files were modified by the implementer.

## 5. Review
Run **reviewer**. For `CHANGES REQUIRED`, send the blocker and should-fix findings to the implementer, re-run the gate, then re-review. At most 2 review rounds, then escalate to the user with the remaining findings.

## 6. Wrap-up
- Append the implementer's undocumented decisions to `docs/decisions.md`.
- Report to the user: ACs covered, migrations added, review verdict, and anything deferred.
- Propose a commit message. Commit only when the user approves.
- After the commit, run `cargo clean` (debug test binaries pile up to tens of GB).
