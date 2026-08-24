//! The default, dependency-free operator page rendered with Maud.
//!
//! Stable `data-testid` attributes preserve the browser contract. Status and
//! identity requests remain same-origin and the page requires no writable
//! filesystem or JavaScript dependency chain.

use maud::{DOCTYPE, Markup, PreEscaped, html};

pub async fn index() -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "act-web-server" }
                style { (PreEscaped(STYLES)) }
            }
            body {
                h1 data-testid="title" { "act-web-server" }

                section {
                    h2 { "Service status" }
                    dl {
                        dt { "Liveness" }
                        dd data-testid="health-status" { "checking…" }
                        dt { "Readiness" }
                        dd data-testid="ready-status" { "checking…" }
                        dt { "Database" }
                        dd data-testid="database-status" { "checking…" }
                    }
                }

                section {
                    h2 { "Verify a token" }
                    form data-testid="identity-form" autocomplete="off" {
                        label for="token" { "Supabase access token" }
                        input
                            id="token"
                            name="token"
                            data-testid="token-input"
                            placeholder="eyJhbGciOiJIUzI1NiIs…"
                            spellcheck="false";
                        button type="submit" data-testid="verify-button" { "Verify" }
                    }
                    p class="bad" data-testid="identity-error" hidden {}
                    pre data-testid="identity-result" hidden {}
                }

                script { (PreEscaped(SCRIPT)) }
            }
        }
    }
}

const STYLES: &str = r#"
  :root { color-scheme: light dark; --line: #8883; --bad: #b3261e; --good: #1b6b3a; }
  body { font: 16px/1.5 system-ui, sans-serif; margin: 0; padding: 2rem; max-width: 46rem; }
  h1 { font-size: 1.4rem; margin: 0 0 1.5rem; }
  section { border: 1px solid var(--line); border-radius: 8px; padding: 1rem 1.25rem; margin-bottom: 1.5rem; }
  h2 { font-size: 1rem; margin: 0 0 .75rem; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: .35rem 1rem; margin: 0; }
  dt { color: #7a7a7a; }
  dd { margin: 0; font-variant-numeric: tabular-nums; }
  label { display: block; margin-bottom: .35rem; }
  input, button { font: inherit; padding: .5rem .65rem; border-radius: 6px; border: 1px solid var(--line); }
  input { width: 100%; box-sizing: border-box; }
  button { cursor: pointer; margin-top: .75rem; }
  pre { background: #8881; padding: .75rem; border-radius: 6px; overflow-x: auto; margin: .75rem 0 0; }
  .bad { color: var(--bad); }
  .good { color: var(--good); }
  [hidden] { display: none !important; }
"#;

const SCRIPT: &str = r#"
  const $ = (id) => document.querySelector(`[data-testid="${id}"]`);

  async function loadStatus() {
    try {
      const health = await fetch('/health').then((response) => response.json());
      $('health-status').textContent = health.status === 'ok' ? 'ok' : 'unexpected';
      $('health-status').className = health.status === 'ok' ? 'good' : 'bad';
    } catch {
      $('health-status').textContent = 'unreachable';
      $('health-status').className = 'bad';
    }
    try {
      const ready = await fetch('/ready').then((response) => response.json());
      $('ready-status').textContent = ready.ready ? 'ready' : 'not ready';
      $('ready-status').className = ready.ready ? 'good' : 'bad';
      $('database-status').textContent = ready.database_connected ? 'connected' : 'not configured';
    } catch {
      $('ready-status').textContent = 'unreachable';
      $('ready-status').className = 'bad';
      $('database-status').textContent = 'unknown';
    }
  }

  $('identity-form').addEventListener('submit', async (event) => {
    event.preventDefault();
    const token = $('token-input').value.trim();
    const error = $('identity-error');
    const result = $('identity-result');
    error.hidden = true;
    result.hidden = true;

    if (!token) {
      error.textContent = 'Enter a token first.';
      error.hidden = false;
      return;
    }

    try {
      const response = await fetch('/api/me', {
        headers: { Authorization: `Bearer ${token}` }
      });
      if (response.ok) {
        result.textContent = JSON.stringify(await response.json(), null, 2);
        result.hidden = false;
      } else if (response.status === 401) {
        error.textContent = 'Token rejected (401).';
        error.hidden = false;
      } else if (response.status === 503) {
        error.textContent = 'Verification unavailable: no signing secret configured (503).';
        error.hidden = false;
      } else {
        error.textContent = `Unexpected response (${response.status}).`;
        error.hidden = false;
      }
    } catch {
      error.textContent = 'Request failed.';
      error.hidden = false;
    }
  });

  loadStatus();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preserves_browser_contract() {
        let page = index().await.into_string();
        for test_id in [
            "title",
            "health-status",
            "ready-status",
            "database-status",
            "identity-form",
            "token-input",
            "verify-button",
            "identity-error",
            "identity-result",
        ] {
            assert!(page.contains(&format!(r#"data-testid="{test_id}""#)));
        }
    }
}
