# UI rendering runtimes

`act-web-server.rs` has one HTTP runtime—Axum—and three intentionally small
server-rendering adapters. This avoids running three competing web servers or
duplicating authentication, health, readiness, telemetry, and shutdown logic.

## Route matrix

| Route | Renderer | Cargo feature | Default build |
| --- | --- | --- | --- |
| `/` | Maud | always enabled | yes |
| `/ui/leptos` | Leptos SSR | `ui-leptos` | no |
| `/ui/dioxus` | Dioxus SSR | `ui-dioxus` | no |

The Maud operator page remains the dependency-light production default and
preserves the existing `data-testid` browser contract. The optional pages are
public demonstration/mount surfaces and contain no protected platform data.

## Builds

Default Maud server:

```sh
cargo run --locked
```

Maud plus Leptos:

```sh
cargo run --locked --features ui-leptos
```

Maud plus Dioxus:

```sh
cargo run --locked --features ui-dioxus
```

All renderers:

```sh
cargo run --locked --all-features
```

## Selection policy

- Use Maud for small server-owned pages, operational status, forms, and HTML
  where a reactive client runtime would add no value.
- Use Leptos where fine-grained Rust reactivity, islands, or a shared SSR/client
  component model materially improves a feature.
- Use Dioxus where its component model or cross-renderer ecosystem materially
  improves a feature.
- Keep Axum as the router and middleware owner in every case.
- Do not import protected state into a public SSR route. Protected data must pass
  through the same authorization boundary as JSON API data.
- Keep renderer features independently compilable; a production image should
  enable only the adapters it serves.

## Quality gates

```sh
cargo fmt --check
cargo test --locked
cargo test --all-features --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
```
