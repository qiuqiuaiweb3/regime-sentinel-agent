# Acceptance Gates

This file records what has been verified locally and what was verified through
hosted Google Cloud infrastructure.

## Implemented And Verified

- Rust hot-path core: alert scoring, shift labels, feature windows, replay validation,
  fair probability calculation, and alert deduplication.
- Strict fair-probability replay/API path:
  - `fair_probability_feature_windows` is accepted by both `regime-replay` and
    `POST /api/replay/validate`.
  - `p_fair` is computed from `current_price`, `strike_price`,
    `time_remaining_ms`, `realized_volatility`, and `feed_lag_ms`.
  - Raw `feature_windows[].p_fair` remains as a legacy replay compatibility path,
    not the strict acceptance path.
  - Stale or invalid fair-probability inputs downgrade confidence and apply the
    stale-data penalty when either the previous or current window is degraded.
- Alert generation through `generate_alerts_from_feature_windows` now uses
  slug-isolated previous-window tracking plus onset-bucket deduplication and
  cooldown by default, so replay/API/latency/ablation callers do not score
  adjacent windows across different markets.
- MongoDB schema/index specs and Atlas bootstrap CLI.
- MongoDB document writers for `market_ticks`, `feature_windows`, `regime_states`,
  `alerts`, `agent_summaries`, and `backtest_runs`.
- Axum REST/SSE API:
  - `GET /health`
  - `GET /api/dashboard/snapshot`
  - `GET /api/dashboard/snapshot?mode=live|replay`
  - `GET /api/dashboard/events`
  - `POST /api/replay/validate`
  - `GET /api/agent/current-regime`
  - `GET /api/agent/recent-alerts`
  - `POST /api/agent/similar-windows`
  - `GET /api/agent/backtest-metrics`
  - `GET /api/agent/market-summary`
  - `POST /api/agent/explain-now`
  - `GET /api/openapi.json`
- SvelteKit dashboard served by Axum static fallback, with live/replay mode,
  TradingView Lightweight Charts, similar-history context, validation metrics,
  Gemini summary coverage, and a cooldown-gated `Explain now` action.
- Polymarket CLOB market collector core, Coinbase BTC reference collector core,
  stale-data downgrade, and NDJSON fallback.
- Gemini summary request builder/parser/scheduler, disabled by default and gated by
  `GEMINI_ENABLED=true`, with Vertex AI as the default provider and Developer API key
  fallback.
- Manual Gemini explain is gated by `GEMINI_MANUAL_COOLDOWN_SECONDS`
  / `MANUAL_EXPLAIN_COOLDOWN_SECONDS` and a shared `GEMINI_MAX_CALLS_PER_HOUR`
  budget across automatic summaries and manual requests.
- MongoDB MCP integration is documented in `docs/mongodb-mcp.md` with a read-only
  official server template at `mcp/mongodb.readonly.example.json`.
- Local MCP package resolution was verified with
  `npx -y mongodb-mcp-server@1.11.0 --version`, which returned `1.11.0`.
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
- Replay/API tests cover fair-probability input based alert generation, default
  dedup/cooldown, interleaved-market isolation, and stale fair-probability
  confidence downgrade.
- Strict deterministic replay acceptance artifacts:
  - `demo/replay/acceptance-lead-time-window.json`
  - `demo/reports/acceptance-lead-time-report.json`
  - `demo/reports/acceptance-lead-time-report.csv`
  - The fixture covers `1s`, `5s`, and `30s` horizons with early alerts at
    `1000ms`, `5000ms`, and `10000ms` lead time.
  - The report records `median_lead_time_ms=5000.0`,
    `p75_lead_time_ms=10000.0`, `precision=1.0`, `recall=1.0`, and PR-AUC
    `1.0` for all three horizons.
  - This is deterministic replay proof for the demo acceptance gate, not a claim
    of statistically validated live forecasting skill.
- Hot-path latency probe:
  - binary: `cargo run -q -p regime-replay --bin latency-probe`
  - input: `demo/replay/latency-probe-window.json`
  - report: `demo/reports/latency-probe-report.json`
  - Latest local run used `256` samples and recorded p95 hot-path alert
    generation latency at `240ns` / `1us`, below the `500ms` gate.
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
  service account, Secret Manager injection, and
  `AGENT_TOOL_MONGODB_TIMEOUT_MS=1500`.
- Cloud Run deployment was verified on 2026-05-23 JST:
  - build id: `43e9b928-8195-41c6-a438-2adb4ad67014`
  - revision: `regime-sentinel-agent-00008-vxp`
  - image:
    `asia-northeast1-docker.pkg.dev/poly-market-analysis/regime-sentinel/regime-service:43e9b928-8195-41c6-a438-2adb4ad67014`
  - `/health` returned `{"status":"ok","service":"regime-service"}`.
  - `/api/openapi.json` returned OpenAPI `3.0.3`, the hosted Cloud Run server URL,
    a JSON request schema for `POST /api/replay/validate`, and Agent Builder
    operations including `getCurrentRegime`, `findSimilarWindows`, and
    `explainNow`.
  - Hosted Agent tool endpoints returned HTTP `200` with sample fallback when
    MongoDB was unavailable from Cloud Run:
    - `GET /api/agent/current-regime`
    - `GET /api/agent/recent-alerts`
    - `GET /api/agent/backtest-metrics`
    - `GET /api/agent/market-summary`
    - `POST /api/agent/similar-windows`
- Agent Builder / Conversational Agents was configured on 2026-05-23 JST:
  - Dialogflow API: enabled for `poly-market-analysis`.
  - agent:
    `projects/poly-market-analysis/locations/asia-northeast1/agents/3e5926c5-ed12-40de-a944-b66fae7fe1e0`
  - OpenAPI tool:
    `projects/poly-market-analysis/locations/asia-northeast1/agents/3e5926c5-ed12-40de-a944-b66fae7fe1e0/tools/83dd74d9-d114-433d-b46d-6dd4a055bc48`
  - playbook:
    `projects/poly-market-analysis/locations/asia-northeast1/agents/3e5926c5-ed12-40de-a944-b66fae7fe1e0/playbooks/c186ebd4-adcb-4ea6-b400-893768e30ff4`
  - The playbook references the Regime Sentinel Dashboard API tool and instructs
    it to call `getDashboardSnapshot` before summarizing current regime state.
  - The OpenAPI tool was refreshed from the hosted `/api/openapi.json` on
    2026-05-23 JST and verified to include `/api/agent/current-regime`,
    `/api/agent/market-summary`, and `/api/agent/similar-windows`.
- GCP-network Polymarket discovery smoke was verified on 2026-05-23 JST through
  Cloud Build:
  - Gamma API returned live BTC 5m slugs including `btc-updown-5m-1779474300`,
    `btc-updown-5m-1779474600`, and `btc-updown-5m-1779474900`.
  - The corresponding event payloads included CLOB token ids for `Up` and `Down`.
  - Direct local `curl` from this workstation still cannot connect to Polymarket
    domains, so live tests must run from GCP or another network.
- Three-window live Polymarket smoke was verified on 2026-05-23 JST through
  Cloud Build:
  - build id: `631222a7-e26a-4971-8376-6198059b5c44`
  - artifact prefix:
    `gs://poly-market-analysis_cloudbuild/live-smoke/631222a7-e26a-4971-8376-6198059b5c44/`
  - `summary.json` had `passed: true`.
  - Windows:
    - `btc-updown-5m-1779483000`: `market_ticks=34276`, `reference_ticks=101`.
    - `btc-updown-5m-1779483300`: `market_ticks=362`, `reference_ticks=145`.
    - `btc-updown-5m-1779483600`: `market_ticks=6`, `reference_ticks=189`.
  - Each smoke ran for `30` seconds and observed `BTC-USD`, `UP`, and `DOWN`
    outcomes.
  - The live-smoke Cloud Build compiles `live_smoke` before market discovery and
    skips the current window when it is within the last 90 seconds, so discovered
    slugs do not expire during Rust compilation.

## Verification Commands

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
npm test -- --run
npm run check
npm run build
npm audit --omit=dev
npx -y mongodb-mcp-server@1.11.0 --version
node -e "JSON.parse(require('fs').readFileSync('mcp/mongodb.readonly.example.json','utf8'))"
cargo run -q -p regime-replay --bin regime-replay -- \
  --input demo/replay/high-volatility-btc-window.json | jq .
cargo run -q -p regime-replay --bin regime-replay -- \
  --input demo/replay/high-volatility-btc-window.json --format csv
cargo run -q -p regime-replay --bin regime-replay -- \
  --input demo/replay/acceptance-lead-time-window.json | jq .
cargo run -q -p regime-replay --bin regime-replay -- \
  --input demo/replay/acceptance-lead-time-window.json --format csv
cargo run -q -p regime-replay --bin latency-probe -- \
  --input demo/replay/latency-probe-window.json --samples 256
cargo run -p regime-service --bin seed_demo_mongodb
cargo run -p regime-service --bin verify_demo_mongodb
GEMINI_ENABLED=true GEMINI_PROVIDER=vertex GEMINI_LOCATION=global \
  GEMINI_MODEL=gemini-3-flash-preview \
  GEMINI_ACCESS_TOKEN="$(gcloud auth print-access-token)" \
  cargo run -p regime-service --bin gemini_summary_once
gcloud builds submit --config cloudbuild.live-smoke.yaml
```

## External Gates Not Yet Closed

- Final demo video and Devpost submission.

Current local blocker: this machine cannot connect to `docs.polymarket.com`,
`clob.polymarket.com`, or `gamma-api.polymarket.com` with `curl`; GCP network
connectivity has been verified and the full three-window collector smoke passed
from Cloud Build.
