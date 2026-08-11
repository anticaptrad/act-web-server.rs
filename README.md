# Anticaptrad Web Server

Rust/Axum operator web surface for Anticaptrad. The service exposes public liveness/readiness probes and a minimal public operator page, while protected API routes require a verified Supabase JWT.

## Current HTTP surface

- `GET /` — public operator UI; the page itself contains no protected account data.
- `GET /health` — liveness probe.
- `GET /ready` — readiness response including whether optional PostgreSQL connectivity is available.
- `GET /api/me` — authenticated identity projection from the verified Supabase JWT.

Protected routes fail closed when JWT verification is not configured. Operator tokens are supplied in the browser only for authenticated calls, are sent in the `Authorization` header, and must not be placed in URLs, persisted, or logged.

## Configuration

Configuration comes only from the process environment; `.env`/`dotenv` loading is intentionally unsupported.

- `PORT` — listen port, default `8080`.
- `OTEL_SERVICE_NAME` — OpenTelemetry service name, default `act-web-server`.
- `DATABASE_URL` — optional PostgreSQL/Supabase connection string.
- `SUPABASE_JWT_SECRET` — HS256 verification secret for protected routes. If absent, protected access must fail closed.
- `SUPABASE_JWT_AUD` — expected JWT audience, default `authenticated`.
- `SUPABASE_JWT_ISS` — optional expected issuer.
- `SUPABASE_JWT_LEEWAY_SECS` — clock-skew tolerance, default `5`.

Do not commit environment values, tokens, database URLs, JWT secrets, or production payloads.

## Local validation

```sh
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --release --locked
```

The repository includes the SeaORM migration workspace member, so workspace-wide validation is required rather than checking only the web binary.

## Security and trust boundaries

This repository owns the web/operator boundary, not the Anticaptrad YouTube/GAS administrative control plane. It must preserve:

- verified Supabase authentication on protected routes;
- a credential-free public page and probes;
- distinct liveness, readiness, and persistence signals;
- same-origin operation and stable browser-E2E selectors;
- read-only-root-filesystem compatibility;
- explicit telemetry without credential or protected-payload fields.

See `SECURITY.md` for vulnerability reporting and handling expectations.
