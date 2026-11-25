---
name: pr
description: Open a pull request in bd78/rust-md-viewer from a dev branch to main and see it through. Commits and pushes with the git-commit skill, has the pr-manager agent write and open the PR as the person, then loops - pr-manager reports failing checks, unresolved review findings and merge conflicts, rust-devops fixes them, git-commit commits and pushes - until the PR is clean. Use ONLY when the person explicitly asks for or confirms a pull request; never on your own initiative.
---

# Pull request

Opening a pull request is the **person's decision**. A commit or a push to `origin` is never a reason to open one.
After a push (by you, by `rust-devops`, or one the person mentions), first look for an open PR from that branch:
`gh pr list -R bd78/rust-md-viewer --head <branch> --state open`. If there is one, the push already updated it: report its
number and checks, and do not offer another. Only if there is none, ask once: "Pushed `<branch>` to origin. Open a
pull request to `main`?" and do nothing more until they answer.

Run this skill only when the person said yes or ordered the PR in this conversation. Run it in the main
conversation: it starts the `pr-manager` and `rust-devops` agents, and agents cannot start agents.

There is one remote, `origin` = `bd78/rust-md-viewer`; head and base are in the same repository. `gh` acts as the person.

## 0. Preconditions

- `git branch --show-current` is not `main`. If it is, stop and ask which branch (never commit to `main`).
- The PR covers one issue or one change. If the branch bundles unrelated work, say so and ask before going on.
- Ask whether the PR opens as a **draft** (default) or ready for review, unless the person already said.

## 1. Commit and push

- If there are uncommitted changes for this branch: run the `git-commit` skill in full (CodeRabbit loop with
  `rust-devops`, local checks, commit). The person's order for a PR counts as the request to push.
- Push: `git push -u origin <branch>`. Never `--force`.
- If there is nothing to commit, only make sure `origin/<branch>` equals `HEAD`.
- The local checks of `git-commit` step 4 must be green on the exact `HEAD` before opening the PR; cite them in the
  PR's "How it was checked".

## 2. Open the PR

Start `pr-manager` (`subagent_type: "pr-manager"`) with: mode **open**, the branch, draft or ready, and a scratchpad
path for the body (`<scratchpad>/pr-<branch>.md`).

- **blocked** (duplicate, existing PR, not pushed): relay it to the person and stop. An existing PR for the branch
  goes straight to step 3 with its number.
- Otherwise give the person the PR link and the title.

## 3. Check, fix, repeat

Each round:

1. Start `pr-manager` with mode **check**, the PR number, and what the last round did (each finding fixed with its
   commit SHA, or not changed with the reason). It answers those threads, waits once for the PR's checks (CodeRabbit
   and whatever else reports) and reports.
2. **Commits not from the skill** (CodeRabbit can push autofix commits to the branch): pull them
   (`git pull --ff-only`), read them, and verify them like any other change (local checks, a look at the running app where
   it matters) before building on them. Keep, fix or revert with a new commit; say which and why.
3. **clean** → go to step 4.
4. **blocked** → relay the reason and stop (rate limit, rule 17).
5. **needs fixes** → start `rust-devops` with the report's real items only:
   - failing checks that are ours (with their lines), findings with verdict "real defect", and the conflicting files
     if any;
   - the instruction to fix each narrowly and only what it describes, add a test when it is a bug, run the local
     checks, follow `CLAUDE.md` / `AGENTS.md`, and **not commit and not push**;
   - for conflicts: merge `origin/main` into the branch and resolve (no rebase, no force-push). A merge that applies
     cleanly can still keep two versions of one change: read the merged code, don't trust "no conflict".
   - Findings marked "not a defect", "already fixed" or "pre-existing" are not sent: they are answered with the reason.
     "unsure" ones go to the person.
6. Review the agent's diff yourself (`git diff`) for unrelated changes, then run the `git-commit` skill (its CodeRabbit
   loop and local checks) and push to `origin`. Note each finding's commit SHA for the next round.

**Stacked PRs** (a PR whose base is another PR's branch): after the base branch changes, merge it into the stacked
branch, check the merged code as above, and push. GitHub retargets the stacked PR to `main` when the base merges.

**Guards** (stop and ask the person when one trips):

- The same check or finding is still there after two different fixes (house rule 17): leave the PR a draft and
  report what fails and what was tried.
- More than 5 rounds in total.
- A failure that also happens on `main`: not ours. Report it and leave it.
- A finding asks for a scope change, a CI or repository-settings change, a dependency outside the licence allowlist,
  or anything on house rule 10's list.

## 4. Done

Report to the person:

- the PR link, draft or ready, the head SHA;
- the rounds run, what was fixed in each (commit SHAs) and what was answered without a change;
- checks: green, or which fail and why they are not this PR's;
- whether anything is waiting on a human (a review, an "unsure" finding).

Never merge, approve, close or mark the PR ready yourself. If the person later says it is ready:
`gh pr ready <N> --repo bd78/rust-md-viewer`.
