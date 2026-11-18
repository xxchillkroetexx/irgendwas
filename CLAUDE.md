@AGENTS.md

## Claude Code

- `/feature <name>` runs the feature pipeline (`.agents/skills/feature/`; `.claude/skills` is a symlink to `.agents/skills`): spec-writer →
  approval → test-writer → leptos-implementer → gate → reviewer.
- Run one building agent at a time; after a feature's commit, `cargo clean` (debug test
  binaries pile up to tens of GB).
- Never read `server/tests/it/group_archive.rs` whole (~2,750 lines). Search it (`grep -n`)
  or read only the lines you need (Read with offset/limit).
