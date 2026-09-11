# act-web-server.rs

Server-rendered Anticaptrad operator UI and fail-closed web/API gateway. The
public page contains no protected data; browser-provided access tokens are sent
only in the `Authorization` header and are never persisted, logged, or placed in
URLs.

## Authentication and telemetry

Protected routes use the official `shared-auth-client` pinned to immutable
commit `cc57a85b276bee81ad94decc87df2f48d49cab9f`. The web tier acts as a BFF for
the API resource, so the user's delegated product bearer is introspected for
the exact `act-api` audience and `youtube:admin` scope before the same bearer is
forwarded on synchronous API modes. The independent service credential is used
only as the caller credential for protected introspection.
The former local Supabase HS256/JWT implementation and its issuer/secret knobs
have been removed. Product data remains actor-scoped by Anticaptrad itself.

Ores structured logging is pinned to
`ca176fb6768a9750d262a536952268625ffd3a8a` and bridged into tracing/OTLP. Logs
contain metadata only: no bearer, service credential, email, subject, payload,
database URL, NATS URL, or private topology.

## Four interaction modes

`GET /api/data/:mode` accepts one of these explicit modes after Shared Auth:

1. `direct_read_only_database` uses a separately provisioned `*_web_ro` role,
   verifies the actual Postgres role, begins a read-only transaction, runs one
   fixed parameterized actor-scoped `SELECT`, caps results at 100 rows, and
   applies a two-second deadline. It exposes no raw SeaORM connection.
2. `stateless_http` calls the API over HTTPS (loopback HTTP only for local
   development), disables redirects, applies connection/request deadlines, and
   streams at most 256 KiB before parsing JSON.
3. `stateful_mtls_tcp` keeps one mutually authenticated TLS connection, uses
   strict four-byte length framing capped at 64 KiB, bounds concurrency, and
   sends a fresh Shared Auth bearer/subject binding on every logical operation.
4. `jet_stream_async` verifies the browser bearer through Shared Auth, then
   replaces it with an HMAC over the exact strict, versioned operation envelope.
   The broker and durable database therefore never receive the browser bearer.
   Publishing uses a stable operation/deduplication ID and waits for the
   JetStream acknowledgement. The API consumer at exact commit
   `f22ed47258c07556ab5bd1375efa3bc8ad56df29` verifies the operation-bound HMAC
   before any journal write and owns the durable consumer, explicit
   ack/redelivery policy, SHA-256 inbox dedupe, owner-scoped queryable status,
   and transactional result outbox. The browser polls
   `GET /api/operations/:operation_id`; this web route forwards to the API over
   the bounded stateless HTTPS client.

NATS/broker or database unavailability does not take down the public web
process. The affected transport remains unavailable and fails closed. Async
mode also requires `ACT_API_URL`; an accepted operation without a bounded,
owner-scoped status path is rejected as an invalid startup configuration.

## Configuration

Configuration comes only from process environment injection. Never add
`dotenv`, plaintext credentials, or private keys to the repository.

| Variable | Purpose |
| --- | --- |
| `PORT` | Public HTTP listener; default `8080` |
| `OTEL_SERVICE_NAME` | OTel/Ores service name |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Optional OTLP collector |
| `SHARED_AUTH_URL` | HTTPS Shared Auth authority |
| `SHARED_AUTH_SERVICE_CREDENTIAL` | Independent introspection caller credential |
| `ACT_READONLY_DATABASE_URL` | Direct-read-only Postgres connection |
| `ACT_READONLY_DATABASE_ROLE` | Exact role name ending in `_web_ro` |
| `ACT_API_URL` | HTTPS API cluster base URL |
| `ACT_API_MTLS_ADDR` | API mTLS IP socket |
| `ACT_API_TLS_SERVER_NAME` | Exact TLS server name |
| `ACT_WEB_CLIENT_CERT_FILE` | Runtime-injected client certificate chain |
| `ACT_WEB_CLIENT_KEY_FILE` | Runtime-injected client private key |
| `ACT_API_CA_FILE` | Runtime-injected API CA bundle |
| `ACT_NATS_URL` | TLS NATS/JetStream endpoint; cleartext only on loopback |
| `ACT_NATS_OPERATION_HMAC_KEY` | Runtime-injected, minimum 32-byte web/API operation-attestation key; required with NATS and never logged or persisted |

## Validation

```text
cargo fmt --all --check
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo audit
```

The `.zpkg.toml` manifest is the zed-pkg package/dependency intent. Cargo Git
dependencies remain immutable source pins; no `.zpkg.lock` is fabricated.
