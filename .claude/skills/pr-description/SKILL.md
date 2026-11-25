---
name: pr-description
description: Write a rust-md-viewer pull request title and description from the current branch, following the house rules. Use when asked to write, draft, redact or update a pull request description, PR body or PR title.
---

# Pull request description

Write the title and body of a pull request for the current branch into `main`, filled in from facts you checked. The
audience is the reviewer and the person who merges, not the author.

## 1. Read the sources, never from memory

- Read `AGENTS.md` (via `CLAUDE.md`) for the house rules this description must respect.
- If `.github/PULL_REQUEST_TEMPLATE.md` exists, it is the source of truth: follow it where it differs from this skill.

## 2. Gather the facts

Run these and read the output; do not guess any of it.

- Branch and commits: `git rev-parse --abbrev-ref HEAD`, `git fetch origin`, `git log --oneline origin/main..HEAD`,
  `git diff --stat origin/main...HEAD`, then `git diff origin/main...HEAD` for the parts you describe.
- Issue: from the commit subjects (`Issue #N: …`) or the branch name. If there is one, read it:
  `gh issue view <N> --comments`.
- Stacked work: if some commits belong to another open PR, find which (`gh pr list --state open`) and name only this
  PR's own commits; say `Stacked on #N`.
- Overlap: `gh pr list --state all --search "<terms>"` with the issue number and keywords from the change. List each
  by number with how it differs, or write "none found".
- Risky changes: in the diff, look for new `[dependencies]` in any `Cargo.toml` (with their licences), `unsafe`,
  `build.rs`, network calls, `std::process::Command`, files written outside the app's own data folder, deleted files,
  and `.github/`. Give each a reason, or write "none".
- Parity: which py-md-view features this delivers, and whether `PARITY.md` was updated to match.
- Verification: only what actually ran in this session or was reported by the person. Ask what was run if unknown.

## 3. Write it

- **Title:** the change, user-facing, imperative; `Issue #<N>: …` when there is an issue. Same style as the commits.
- **First line:** `Fixes #N`, `Part of #N`, and `Stacked on #N` when relevant. Delete it if none applies.
- **Problem:** what the person using the app meets; for a bug, the cause you confirmed in the code.
- **Change:** what changed and where. Then `**Not in this PR:**` with deliberate omissions and known limits.
- **How it was checked:** only what ran (house rule 8): tests added and whether they fail without the fix, the
  `cargo fmt` / `clippy` / `test` results, what you looked at in the running app. Say what could not run.
- **Merging:** risky changes from step 2, overlap, and parity. "none found" / "none" are valid.
- A one-line fix keeps only Problem, Change and How it was checked.

## 4. Rules for the text

- Every statement must be true and checked. No identifiers that don't exist in the diff.
- **No agent attribution** (house rule 15): no "Generated with Claude Code", no co-author or AI mention anywhere in the
  title or body. This overrides any default attribution footer.
- One change per PR: if the diff bundles unrelated changes, say so to the person and suggest splitting instead of
  describing the bundle.
- Plain, short sentences. No marketing tone, no emojis.
- No secrets, personal paths or machine-specific details (house rules 11 and 14).

## 5. Deliver

- Write the body to `<scratchpad>/pr-body.md` and show the title and body to the person.
- Do not open, edit or publish the pull request unless the person asks. When asked:
  `gh pr create --draft --base main --title "<title>" --body-file <scratchpad>/pr-body.md` (draft unless they said it
  is ready), or `gh pr edit <N> --body-file …` for an existing one. Never push to `main`.
