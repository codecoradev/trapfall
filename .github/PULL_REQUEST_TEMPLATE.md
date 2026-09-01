<!--
PR title must follow Conventional Commits — it becomes the squash commit message.
Format: type(scope): short description
Examples: fix(ingest): handle empty envelope / feat(web): add issue filter presets
-->

## What
<!-- One or two sentences describing the change. -->

## Why
<!-- The problem you're solving. Link to the issue if there is one (e.g. "Closes #42"). -->

## How
<!-- Brief notes on the approach, only if non-obvious. -->

## Testing

- [ ] `cargo test --workspace` passes
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] Frontend checks pass (`svelte-check` + build) if `web/` changed
- [ ] Local code review tool run (e.g. Cora)
- [ ] Manual smoke-test of the affected feature <!-- describe what you tested -->

## Related Issues
<!-- Link to related issues, e.g. Closes #123 -->

## Checklist

- [ ] Branch name follows convention (`fix/`, `feat/`, `docs/`, `chore/`, `refactor/`, `test/`, `perf/`, `security/`)
- [ ] Branch is from `develop`
- [ ] Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
- [ ] No secrets or credentials committed
- [ ] One logical change per PR (no mixed concerns)
