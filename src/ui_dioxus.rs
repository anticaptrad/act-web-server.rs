//! Optional Dioxus server-rendered surface.

use axum::response::Html;
use dioxus::prelude::*;

pub async fn index() -> Html<String> {
    let body = dioxus_ssr::render_element(rsx! {
        main {
            "data-framework": "dioxus",
            p { class: "eyebrow", "OPTIONAL SSR ADAPTER" }
            h1 { "AntiCapTrad · Dioxus" }
            p {
                class: "lede",
                "A component-oriented Rust UI surface mounted inside the existing Axum service."
            }
            section {
                h2 { "Runtime boundary" }
                ul {
                    li { strong { "Router: " } "Axum" }
                    li { strong { "Renderer: " } "Dioxus SSR" }
                    li { strong { "Activation: " } "ui-dioxus Cargo feature" }
                }
            }
            nav {
                "aria-label": "UI runtime links",
                a { href: "/", "Maud operator UI" }
                " · "
                a { href: "/ui/leptos", "Leptos adapter" }
            }
        }
    });

    Html(document("Dioxus", &body))
}

fn document(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{title} · act-web-server</title><style>{STYLES}</style></head><body>{body}</body></html>"#
    )
}

const STYLES: &str = r#"
  :root { color-scheme: dark; font-family: ui-sans-serif, system-ui, sans-serif; background: #090b10; color: #f2f5f7; }
  body { margin: 0; padding: clamp(1.5rem, 5vw, 5rem); }
  main { max-width: 52rem; margin: auto; }
  .eyebrow { color: #b69cff; letter-spacing: .16em; font-size: .75rem; font-weight: 800; }
  h1 { font-size: clamp(2rem, 7vw, 4.5rem); margin: .25rem 0 1rem; }
  .lede { color: #aeb8c3; font-size: 1.15rem; max-width: 40rem; }
  section { background: #11151d; border: 1px solid #2a3240; border-radius: 1rem; padding: 1.25rem; margin: 2rem 0; }
  li { margin: .6rem 0; }
  a { color: #66d9ff; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn identifies_the_dioxus_adapter() {
        let page = index().await.0;
        assert!(page.contains("data-framework=\"dioxus\""));
        assert!(page.contains("ui-dioxus Cargo feature"));
    }
}
