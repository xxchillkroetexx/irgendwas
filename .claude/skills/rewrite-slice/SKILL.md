---
name: rewrite-slice
description: Run the rewrite pipeline for one slice of the PHP → Rust/Leptos rewrite (spec → tests → implementation → gate → review → fix loop). Use when the user runs /rewrite-slice <slice>.
argument-hint: <slice>  (i18n-foundation | auth | account | groups | wishlists | exclusions | draw | i18n-polish)
disable-model-invocation: true
---

Run the pipeline for slice `$ARGUMENTS`. The main session orchestrates; the agents do the work. Run stages **sequentially**; never run two agents that write to `rust/` at the same time.

## 0. Preconditions
- Read `docs/rewrite/PLAN.md`. Check that the slice exists in the tracker and that every earlier slice is `done` (or that the user has explicitly said to go out of order).
- Check that Phase 1 (foundation) is complete: `rust/` builds and the gate passes on a clean tree. If not, stop and say what's missing.
- Check that `git status` is clean, so the slice's diff is reviewable on its own.

## 1. Spec
- If `docs/rewrite/specs/$ARGUMENTS.md` is missing, run the **spec-writer** agent for the slice.
- If the spec is `status: draft`: show the user its summary, the redesign decisions and the open questions, and **stop**. Continue only after the user has answered the open questions and approved; then set `status: approved` in the spec and update the tracker.

## 2. Tests
Run **test-writer** with the slice name. Check its report: every AC is mapped, and the tests compile and fail. If any AC is untestable as written, go back to the user and fix the spec first.

## 3. Implementation
Run **leptos-implementer** with the slice name. If it stops because of a spec/test conflict, bring the conflict to the user; don't resolve it yourself by editing tests.

## 4. Gate (main session verifies; don't trust the report alone)
From `rust/`, run fmt check, clippy `-D warnings`, `cargo leptos test` and `cargo leptos end-to-end`. If anything fails, send the failure output back to the implementer (SendMessage to the same agent) and repeat. After 3 failed rounds, stop and report to the user.

Also check `git diff --stat` to confirm that no test files were modified by the implementer.

## 5. Review
Run **rewrite-reviewer**. For `CHANGES REQUIRED`, send the blocker and should-fix findings to the implementer, re-run the gate, then re-review. At most 2 review rounds, then escalate to the user with the remaining findings.

## 6. Wrap-up
- Append the implementer's undocumented decisions to the decision log in `PLAN.md`, and set the slice's tracker row to `done`.
- Report to the user: ACs covered, migrations added, review verdict, and anything deferred.
- Propose a commit message for the slice. Commit only when the user approves.
