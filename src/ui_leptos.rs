//! Optional Leptos server-rendered surface.

use axum::response::Html;
use leptos::prelude::*;

pub async fn index() -> Html<String> {
    let body = view! {
        <main data-framework="leptos">
            <p class="eyebrow">"OPTIONAL SSR ADAPTER"</p>
            <h1>"AntiCapTrad · Leptos"</h1>
            <p class="lede">
                "A fine-grained Rust UI surface mounted inside the existing Axum service."
            </p>
            <section>
                <h2>"Runtime boundary"</h2>
                <ul>
                    <li><strong>"Router: "</strong>"Axum"</li>
                    <li><strong>"Renderer: "</strong>"Leptos SSR"</li>
                    <li><strong>"Activation: "</strong>"ui-leptos Cargo feature"</li>
                </ul>
            </section>
            <nav aria-label="UI runtime links">
                <a href="/">"Maud operator UI"</a>
                " · "
                <a href="/ui/dioxus">"Dioxus adapter"</a>
            </nav>
        </main>
    }
    .to_html();

    Html(document("Leptos", &body))
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
  .eyebrow { color: #adff2f; letter-spacing: .16em; font-size: .75rem; font-weight: 800; }
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
    async fn identifies_the_leptos_adapter() {
        let page = index().await.0;
        assert!(page.contains("data-framework=\"leptos\""));
        assert!(page.contains("ui-leptos Cargo feature"));
    }
}
