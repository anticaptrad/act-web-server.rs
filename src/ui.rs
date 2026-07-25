//! A small server-rendered operator UI.
//!
//! The service is otherwise JSON-only, which leaves no way to see at a glance
//! whether it is healthy, whether persistence is attached, or whether a given
//! Supabase token actually verifies. This page answers those three questions
//! and is the surface the browser end-to-end suites drive.
//!
//! It is deliberately dependency-free: one static document with inline
//! same-origin `fetch` calls. No templating crate, no bundler, and nothing
//! written to disk, so it works under a read-only root filesystem.
//!
//! Elements carry stable `data-testid` attributes. Browser tests select on
//! those rather than on text or layout, so copy and styling can change without
//! breaking the suites.

use axum::response::Html;

pub async fn index() -> Html<&'static str> {
    Html(PAGE)
}

const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>act-web-server</title>
<style>
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
</style>
</head>
<body>
<h1 data-testid="title">act-web-server</h1>

<section>
  <h2>Service status</h2>
  <dl>
    <dt>Liveness</dt><dd data-testid="health-status">checking…</dd>
    <dt>Readiness</dt><dd data-testid="ready-status">checking…</dd>
    <dt>Database</dt><dd data-testid="database-status">checking…</dd>
  </dl>
</section>

<section>
  <h2>Verify a token</h2>
  <form data-testid="identity-form" autocomplete="off">
    <label for="token">Supabase access token</label>
    <input id="token" name="token" data-testid="token-input" placeholder="eyJhbGciOiJIUzI1NiIs…" spellcheck="false">
    <button type="submit" data-testid="verify-button">Verify</button>
  </form>
  <p class="bad" data-testid="identity-error" hidden></p>
  <pre data-testid="identity-result" hidden></pre>
</section>

<script>
  const $ = (id) => document.querySelector(`[data-testid="${id}"]`);

  async function loadStatus() {
    try {
      const health = await fetch('/health').then((r) => r.json());
      $('health-status').textContent = health.status === 'ok' ? 'ok' : 'unexpected';
      $('health-status').className = health.status === 'ok' ? 'good' : 'bad';
    } catch {
      $('health-status').textContent = 'unreachable';
      $('health-status').className = 'bad';
    }
    try {
      const ready = await fetch('/ready').then((r) => r.json());
      $('ready-status').textContent = ready.ready ? 'ready' : 'not ready';
      $('ready-status').className = ready.ready ? 'good' : 'bad';
      // Persistence is optional by design: the service serves without it.
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
      const res = await fetch('/api/me', { headers: { Authorization: `Bearer ${token}` } });
      if (res.ok) {
        result.textContent = JSON.stringify(await res.json(), null, 2);
        result.hidden = false;
      } else if (res.status === 401) {
        error.textContent = 'Token rejected (401).';
        error.hidden = false;
      } else if (res.status === 503) {
        error.textContent = 'Verification unavailable: no signing secret configured (503).';
        error.hidden = false;
      } else {
        error.textContent = `Unexpected response (${res.status}).`;
        error.hidden = false;
      }
    } catch {
      error.textContent = 'Request failed.';
      error.hidden = false;
    }
  });

  loadStatus();
</script>
</body>
</html>
"#;
