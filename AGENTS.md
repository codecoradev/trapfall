# AGENTS.md — Rules for AI agents working on TrapFall

## Git Commits

- **NO `Co-authored-by` trailer** in any commit message. Ever.
- If a tool auto-injects `Co-authored-by`, strip it before committing.
- Verify with: `git log -1 --format="%B" | grep -i co-author` — must be empty.

## Git Workflow

- Default branch: `develop`
- Never push to `main` directly
- Always PR to `develop`
- Conventional commit format: `feat:`, `fix:`, `chore:`, `docs:`
- No co-author, no signed-off-by unless explicitly requested

## Style

- Follow CLAUDE.md conventions
- No `unwrap()` in production code
- `tracing` for all logging
- Tests alongside source files
