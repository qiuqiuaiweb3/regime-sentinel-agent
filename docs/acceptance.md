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
  `GEMINI_ENABLED=true`, with Vertex AI as the default provider and Developer API key
  fallback.
- Real Vertex AI Gemini summary was observed on 2026-05-23 JST with
  `gemini-3-flash-preview`; `gemini_summary_once` returned text and persisted it to
  MongoDB `agent_summaries`.
- Hosted Cloud Run URL:
  `https://regime-sentinel-agent-998092298764.asia-northeast1.run.app`
- Public GitHub repository:
  `https://github.com/qiuqiuaiweb3/regime-sentinel-agent`
- Fixed replay demo artifacts:
  - `demo/replay/high-volatility-btc-window.json`
  - `demo/reports/validation-report.json`
  - `demo/reports/validation-report.csv`
  - `demo/reports/backtest-run.sample.json`
- Current replay sample has one early alert at `750ms` for a shift onset at `1000ms`,
  with `250ms` lead time.
- MongoDB Atlas demo seed and verify CLIs are available:
  - `cargo run -p regime-service --bin seed_demo_mongodb`
  - `cargo run -p regime-service --bin verify_demo_mongodb`
- The seed CLI writes an ignored `.regime-demo-seed.json` with a fresh
  `demo_run_id`; the verify CLI uses that id and exits nonzero if any matching
  collection count is zero.
- MongoDB Atlas demo write/query was verified on 2026-05-23 JST for
  `demo-1779472674857` with count `1` in each of `market_ticks`, `feature_windows`,
  `regime_states`, `alerts`, `agent_summaries`, and `backtest_runs`.
- Cloud Run resource config is explicit in `cloudbuild.yaml`: `asia-northeast1`,
  `1` vCPU, `1Gi` memory, min `1`, max `1`, concurrency `80`, timeout `3600s`,
  service account, and Secret Manager injection.
- GCP-network Polymarket discovery smoke was verified on 2026-05-23 JST through
  Cloud Build:
  - Gamma API returned live BTC 5m slugs including `btc-updown-5m-1779474300`,
    `btc-updown-5m-1779474600`, and `btc-updown-5m-1779474900`.
  - The corresponding event payloads included CLOB token ids for `Up` and `Down`.
  - Direct local `curl` from this workstation still cannot connect to Polymarket
    domains, so live tests must run from GCP or another network.

## Verification Commands

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
npm test -- --run
npm run check
npm run build
npm audit --omit=dev
cargo run -q -p regime-replay -- --input demo/replay/high-volatility-btc-window.json | jq .
cargo run -q -p regime-replay -- --input demo/replay/high-volatility-btc-window.json --format csv
cargo run -p regime-service --bin seed_demo_mongodb
cargo run -p regime-service --bin verify_demo_mongodb
GEMINI_ENABLED=true GEMINI_PROVIDER=vertex GEMINI_LOCATION=global \
  GEMINI_MODEL=gemini-3-flash-preview \
  GEMINI_ACCESS_TOKEN="$(gcloud auth print-access-token)" \
  cargo run -p regime-service --bin gemini_summary_once
```

## External Gates Not Yet Closed

- Agent Builder configured in Google Cloud with `/api/openapi.json` as an OpenAPI tool.
- Live Polymarket smoke test for three real 5 minute market windows.
- Final demo video and Devpost submission.

Current local blocker: this machine cannot connect to `docs.polymarket.com`,
`clob.polymarket.com`, or `gamma-api.polymarket.com` with `curl`; GCP network
connectivity has been verified, but the full three-window collector smoke still
needs to run from GCP or another network.
