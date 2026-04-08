---
name: wrapup
description: Reflect on task completion and record lessons learned. Use when the user says "wrapup", "wrap up", "wrap it up", "close out", or "record lessons learned". Distills what went well, what was tricky, and updates project knowledge to improve future sessions.
---

# Wrapup

Reflect on the session and record durable lessons — things that would have helped at the start, or would help next time.

## Process

### Step 1 — Recall what happened

Briefly summarize:
- What was the task or change?
- What was the main challenge or insight?
- What required user correction or guidance?

### Step 2 — Identify what is worth recording

Ask: *Would knowing this at the start of a future session have saved time or prevented a mistake?*

Candidates:
- Patterns, idioms, or constraints that aren't obvious from the code
- Decisions made (and why) that aren't captured in code comments or git messages
- Guidelines about where certain kinds of knowledge should live
- Anything the user had to correct or clarify

Skip:
- Things already in CLAUDE.md or docs
- One-off debugging steps unlikely to recur
- Implementation details that live in the code itself

### Step 3 — Choose the right destination

| What | Where |
|------|-------|
| Project-wide workflow or coding conventions | `CLAUDE.md` — keep it concise, high-signal |
| Architectural concepts, design rationale, "why" | `docs/` or `docs/bs/` — high-level, no impl detail |
| How to use a skill or when to invoke it | The skill's `SKILL.md` |
| User preferences, collaboration style | Auto-memory (`~/.claude/projects/.../memory/`) |
| Corrected behavior for future Claude sessions | Auto-memory feedback entries |

### Step 4 — Write the updates

- **CLAUDE.md**: Add a bullet or short paragraph. Keep the file small and scannable. No restating what's obvious from the code.
- **docs/**: Focus on "what" and "why" at a conceptual level. Avoid function names, parameter lists, exact APIs — these go stale. Brief "how" is fine if it illuminates the design.
- **Skills**: Tighten trigger descriptions, add guidance about edge cases, or clarify scope.
- **Memory**: Add feedback entries for behavioral corrections (see auto-memory guidelines).

### Step 5 — Optionally, refine this skill

If the wrapup process itself revealed a gap (e.g. a destination type is missing, guidance is ambiguous), improve this `SKILL.md` before finishing.

## Output

Summarize what was updated and why, so the user can review and push.
