# Acceptance Gates

This file records what can be verified locally now and what still requires hosted
infrastructure or external network access.

## Implemented And Locally Verified

- Rust hot-path core: alert scoring, shift labels, feature windows, replay validation.
- MongoDB schema/index specs and Atlas bootstrap CLI.
- MongoDB document writers for `market_ticks`, `feature_windows`, `regime_states`,
  `alerts`, `agent_summaries`, and `backtest_runs`.
- Axum REST/SSE API:
  - `GET /health`
  - `GET /api/dashboard/snapshot`
  - `GET /api/dashboard/events`
  - `POST /api/replay/validate`
  - `GET /api/openapi.json`
- SvelteKit dashboard served by Axum static fallback.
- Polymarket CLOB market collector core, Coinbase BTC reference collector core,
  stale-data downgrade, and NDJSON fallback.
- Gemini summary request builder/parser/scheduler, disabled by default and gated by
  `GEMINI_ENABLED=true` plus `GEMINI_API_KEY`.

## Verification Commands

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
npm test -- --run
npm run check
npm run build
npm audit --omit=dev
```

## External Gates Not Yet Closed

- Hosted Cloud Run URL reachable from the public internet.
- Agent Builder configured in Google Cloud with `/api/openapi.json` as an OpenAPI tool.
- Gemini summary observed in `agent_summaries` or NDJSON fallback with a real API key.
- Live Polymarket smoke test for three real 5 minute market windows.
- Final demo video and Devpost submission.

Current local blocker: this machine cannot connect to `docs.polymarket.com`,
`clob.polymarket.com`, or `gamma-api.polymarket.com` with `curl`, so the live
Polymarket smoke test must be retried from Cloud Run or another network.
