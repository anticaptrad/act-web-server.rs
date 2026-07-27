# act-web-server agent instructions

## Repository restrictions and invariants

- Do not run `git reset`, `git filter-repo`, or `git clean`.
- Do not run `rm` except when explicitly deleting known temporary or scratch files.
- `dotenv` is blacklisted. Do not install or use it; configuration comes from the process environment.
- Preserve verified Supabase authentication on protected routes and fail closed when verification configuration is absent.
- The public operator page must expose no protected data. Tokens are operator-supplied in the browser, sent only in the Authorization header, never placed in URLs, persisted, or logged.
- Preserve same-origin, dependency-free operation under a read-only root filesystem and stable `data-testid` selectors for browser E2E tests.
- Keep liveness, readiness, and optional persistence state semantically distinct.

## Instruction discovery

Resolve `$PWD`, walk upward through every parent directory to the filesystem root, read every readable lowercase `agents.md` on that ancestor chain, and apply them root-to-leaf. Do not search siblings. Deduplicate resolved paths/inodes, avoid symlink cycles, and report unreadable files.

## Synchronize with the remote

Before editing, inspect `git status`, current branch, remotes, and default branch. Run `git fetch --all --prune` and create the feature branch from the latest remote default branch. Fetch again before pushing and incorporate upstream changes using repository merge policy. Never discard remote commits, force-push, rewrite shared history, bypass review, or bypass required CI.

## Resolve Git conflicts semantically

Resolve conflicts by understanding and combining both sides' intent. Do not mechanically choose `ours`, `theirs`, current, or incoming changes. Produce the conceptually correct result while preserving compatible authentication, token handling, public/protected boundaries, health/readiness semantics, read-only operation, stable E2E selectors, accessibility, tests, documentation, configuration, and UI/runtime behavior. If intentions are incompatible, make the smallest explicit design decision and document it in the pull request.

After resolving, reread every affected file from the top, run formatting, linting, Rust tests/builds and browser E2E validation, then search the entire worktree for conflict markers:

```sh
grep -RInE '^(<<<<<<<|=======|>>>>>>>)' --exclude-dir=.git .
```

If any marker or suspicious partial resolution remains, repeat semantic resolution from the top and rerun validation. A conflict is resolved only when the result is conceptually coherent and verified, not merely accepted by Git.
