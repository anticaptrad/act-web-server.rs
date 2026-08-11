# Security Policy

## Reporting

Report security issues privately through GitHub's security advisory flow for this repository or the organization. Do not open a public issue containing credentials, JWTs, database URLs, protected user data, production request/response bodies, or exploit details that would materially increase risk before a fix is available.

Include the smallest reproducible description possible: affected commit, route or component, expected security boundary, observed behavior, and redacted reproduction steps. Replace secrets and personal data with synthetic values.

## Sensitive boundaries

Security reports should treat the following as sensitive:

- Supabase JWT signing material and bearer tokens;
- PostgreSQL/Supabase connection strings;
- authenticated identity payloads;
- operator browser state;
- telemetry that could expose credentials or protected data;
- container/runtime configuration that could weaken read-only or same-origin assumptions.

The public `/`, `/health`, and `/ready` surfaces must not become an alternate path to protected account data. Protected `/api/*` routes must continue to fail closed when verification configuration is absent or invalid.

## Validation expectations

Security fixes should preserve or strengthen the repository's required validation:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --release --locked
```

Do not weaken authentication, token handling, readiness semantics, telemetry redaction, or read-only-root compatibility merely to make a test pass.
