---
name: address-review
description: Download unresolved GitHub PR review comments and create a structured plan for addressing them. Use when the user wants to work through PR feedback.
allowed-tools: Bash(${CLAUDE_SKILL_DIR}/fetch_comments.py), Bash(gh pr view *), Bash(gh repo view *)
---

## Unresolved Review Comments

!`${CLAUDE_SKILL_DIR}/fetch_comments.py`

---

## Your Task

Analyze the unresolved comments above and produce a structured implementation plan. Follow this process carefully:

### Step 1 — Categorize comments into coherent tasks

Group related comments into tasks. A task may cover:
- A **regression** (something that used to work but was broken by this PR)
- A **bug or correctness issue**
- A **code quality / cleanup** item (rename, remove redundancy, simplify)
- A **refactor** (structural change affecting multiple files)
- A **test addition or fix**
- A **documentation update**
- A **GitHub issue to file** (feature request or future work not done in this PR)

Each comment should belong to exactly one task. If a comment is too minor to warrant its own task, group it with the nearest related task under a sub-item.

### Step 2 — For each task, gather just enough context

Use read-only exploration (Glob, Grep, Read, Bash for `git log`/`gh`) to identify:
- The **function or struct name** most relevant to the change (not a line number — those go stale)
- The **file path(s)** to modify
- Any **existing utilities or patterns** that should be reused

Do **not** do deep implementation research at this stage — just enough to point accurately to the right location.

### Step 3 — Write the plan

Write the final plan to the plan file. Structure:

- **Context section** at the top: what this PR does and why these comments need addressing
- One **section per task**, titled clearly (e.g. `T1 — Regression: empty lambda params`)
- Each task section includes:
  - The **problem** (from the comment — preserve all details)
  - The **comment ID(s)** it corresponds to (all relevant comment IDs)
  - The **file(s) and function/struct name(s)** to modify (no raw line numbers)
  - **Complete context and constraints** from the comment(s): exact error messages, code examples, suggestions, workarounds mentioned, why the change matters
- A **suggested implementation order** (regressions and bugs first, then cleanup, then docs, then issues to file)
- A **verification section** with the exact commands to run after implementation

### Guidelines

- **Never use raw line numbers** as location pointers — use function/struct/impl names instead, since line numbers drift as files are edited.
- **Preserve all information from comments**: Include exact error messages, code snippets, workarounds, alternative approaches, and rationale. The plan should be complete enough to implement without re-reading the PR comments.
- Note which comments reference items **already addressed** in the current codebase (do not re-do them).
- If a comment asks to "file a GitHub issue", that is a task of its own (use `gh issue create`).
- Include multiple comment IDs if a single task addresses several related comments.
- Keep the plan scannable but comprehensive — every detail from the comments must appear somewhere in the plan, organized logically by task.
