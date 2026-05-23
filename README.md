# Regime Sentinel Agent

Regime Sentinel Agent is a Google Cloud Rapid Agent Hackathon / MongoDB Track project.
It monitors Polymarket BTC 5 minute Up/Down markets, emits deterministic regime-shift
warnings on the hot path, stores market memory in MongoDB, and uses Agent Builder plus
Gemini for low-frequency explanations.

## Current Status

This repository is the independent hackathon implementation. It is not a copy of
`poly-market-analysis` or `poly-tx`; prior work is used only as interface and design
experience.

Current implementation status:

- Rust workspace with `regime-core`, `regime-service`, and `regime-replay`.
- Google Cloud defaults target Seoul `asia-northeast3`.
- MongoDB and Gemini configuration are provided through environment variables.
- MongoDB MCP read-only integration is documented in `docs/mongodb-mcp.md`, with
  a checked-in client config template under `mcp/`.
- Replay validation can consume prebuilt alerts or generate alerts from feature windows.
- Strict replay/API validation can also consume `fair_probability_feature_windows`,
  where `p_fair` is computed from reference price, strike, time remaining,
  realized volatility, and feed lag instead of accepting caller-provided `p_fair`.
- Generated alerts use slug-isolated previous-window tracking plus default
  onset-bucket deduplication and cooldown.
- MongoDB collection/index bootstrap is available through an explicit CLI.
- MongoDB document writers exist for market ticks, feature windows, regime states, alerts,
  agent summaries, and backtest runs.
- Axum serves the SvelteKit dashboard, REST API, SSE stream, and OpenAPI tool spec.
- Live collector code supports Polymarket CLOB market data, Chainlink BTC/USD reference prices,
  stale-data downgrade, and NDJSON fallback.
- Gemini calls are disabled by default and cost-limited when enabled.
- Agent Builder read tools expose current regime, recent alerts, similar windows,
  backtest metrics, and stored market summaries.

## Architecture

- Hot path: market events -> feature engine -> warning engine -> MongoDB -> REST/SSE dashboard.
- Cold path: MongoDB memory -> Agent Builder tools / MongoDB MCP -> Gemini summary -> MongoDB.
- Dashboard: SvelteKit static client with TradingView Lightweight Charts, served by Axum.

## Local Setup

```bash
cp .env.example .env
npm ci
npm run build
cargo build
cargo test
npm test -- --run
npm run check
```

Do not commit `.env` or local secret files.

Initialize MongoDB collections and indexes explicitly after `.env` is configured:

```bash
cargo run -p regime-service --bin init_mongodb
```

Seed and verify demo documents in the six acceptance collections:

```bash
cargo run -p regime-service --bin seed_demo_mongodb
cargo run -p regime-service --bin verify_demo_mongodb
```

The seed command writes an ignored `.regime-demo-seed.json` file with the latest
`demo_run_id`; the verify command uses that id and exits nonzero if any collection
has no matching document.

## Environment

Required later for live/cloud runs:

- `GOOGLE_CLOUD_PROJECT`
- `GOOGLE_CLOUD_REGION`
- `MONGODB_URI`
- `MONGODB_DB`
- `GEMINI_ENABLED`
- `GEMINI_PROVIDER`
- `GEMINI_LOCATION`
- `GEMINI_SUMMARY_INTERVAL_MINUTES`
- `GEMINI_MAX_CALLS_PER_HOUR`
- `GEMINI_MANUAL_COOLDOWN_SECONDS`
- `MDB_MCP_CONNECTION_STRING`

Gemini is intentionally disabled locally by default. The default provider is Vertex AI.
For a local one-off summary, set:

```bash
GEMINI_ENABLED=true
GEMINI_PROVIDER=vertex
GEMINI_LOCATION=asia-northeast3
GEMINI_MODEL=gemini-3-flash-preview
GEMINI_ACCESS_TOKEN="$(gcloud auth print-access-token)"
GEMINI_SUMMARY_INTERVAL_MINUTES=30
GEMINI_MAX_CALLS_PER_HOUR=2
GEMINI_MANUAL_COOLDOWN_SECONDS=300
cargo run -p regime-service --bin gemini_summary_once
```

`GEMINI_PROVIDER=developer_api` with `GEMINI_API_KEY` is kept as a fallback.
When `MONGODB_URI` and `MONGODB_DB` are set, `gemini_summary_once` persists the
summary to `agent_summaries`; otherwise it writes NDJSON fallback.

Live collector is intentionally disabled by default. To enable it for one market:

```bash
LIVE_COLLECTOR_ENABLED=true
LIVE_MARKET_SLUG=btc-updown-5m-...
LIVE_MARKET_SERIES=btc-updown-5m
POLYMARKET_ASSET_IDS=yes-token-id,no-token-id
POLYMARKET_OUTCOMES=UP,DOWN
REFERENCE_PRICE_SYMBOL=btc/usd
```

## Run Locally

Serve the production dashboard through Axum:

```bash
npm run build
REGIME_STATIC_DIR=build cargo run -p regime-service --bin regime-service
```

Useful local endpoints:

- `http://127.0.0.1:8080/`
- `http://127.0.0.1:8080/health`
- `http://127.0.0.1:8080/api/dashboard/snapshot`
- `http://127.0.0.1:8080/api/dashboard/snapshot?mode=replay`
- `http://127.0.0.1:8080/api/dashboard/events`
- `http://127.0.0.1:8080/api/agent/current-regime`
- `http://127.0.0.1:8080/api/agent/recent-alerts`
- `http://127.0.0.1:8080/api/agent/backtest-metrics`
- `http://127.0.0.1:8080/api/agent/market-summary`
- `http://127.0.0.1:8080/api/openapi.json`

## MongoDB MCP

The partner MCP integration uses the official MongoDB MCP Server in read-only
mode. The checked-in template is:

```text
mcp/mongodb.readonly.example.json
```

Create an ignored local copy for MCP-capable clients:

```bash
cp mcp/mongodb.readonly.example.json mcp/mongodb.local.json
```

Then replace the placeholder connection string or run the server directly with:

```bash
MDB_MCP_CONNECTION_STRING="${MONGODB_URI}" \
  npx -y mongodb-mcp-server@1.11.0 --readOnly
```

See `docs/mongodb-mcp.md` for boundaries and verification evidence.

## Replay Demo Artifacts

The fixed high-volatility replay window is stored at:

```text
demo/replay/high-volatility-btc-window.json
```

Regenerate the JSON validation report:

```bash
cargo run -q -p regime-replay -- \
  --input demo/replay/high-volatility-btc-window.json | jq . \
  > demo/reports/validation-report.json
```

Regenerate the CSV alert timing report:

```bash
cargo run -q -p regime-replay -- \
  --input demo/replay/high-volatility-btc-window.json \
  --format csv \
  > demo/reports/validation-report.csv
```

The checked-in sample `backtest_runs` document is:

```text
demo/reports/backtest-run.sample.json
```

The current sample emits one early alert at `750ms` for a shift onset at `1000ms`,
with `250ms` lead time.

The strict acceptance replay fixture is:

```text
demo/replay/acceptance-lead-time-window.json
```

It records deterministic 1s, 5s, and 30s horizon alerts with `1000ms`, `5000ms`,
and `10000ms` lead time. Its checked-in reports are:

```text
demo/reports/acceptance-lead-time-report.json
demo/reports/acceptance-lead-time-report.csv
```

Run the hot-path latency probe:

```bash
cargo run -q -p regime-replay --bin latency-probe -- \
  --input demo/replay/latency-probe-window.json \
  --samples 256
```

The checked-in latency artifact is `demo/reports/latency-probe-report.json`.

## Google Cloud Run

The deployment target is Seoul:

```bash
gcloud config set project poly-market-analysis
gcloud config set compute/region asia-northeast3
gcloud config set run/region asia-northeast3
gcloud builds submit --region asia-northeast3 --config cloudbuild.yaml
```

The Cloud Build file creates an Artifact Registry Docker repository in
`asia-northeast3`, builds the SvelteKit static frontend and Rust service image,
pushes the image, and deploys Cloud Run with `LIVE_COLLECTOR_ENABLED=false` and
`GEMINI_ENABLED=true`. Scheduled Gemini summaries run every 30 minutes and
replace the previous cached response in `agent_summaries`.

Cloud Run resource settings are explicit in `cloudbuild.yaml`:

- region: `asia-northeast3`
- service account: `998092298764-compute@developer.gserviceaccount.com`
- CPU / memory: `1` vCPU / `1Gi`
- instances: min `1`, max `1`
- concurrency: `80`
- request timeout: `3600s`
- Secret Manager injection: `mongodb-uri`, `mongodb-db`

Current hosted URL:

```text
https://regime-sentinel-agent-998092298764.asia-northeast3.run.app
```

One-time IAM grants used by the deployed service:

```bash
PROJECT_NUMBER=$(gcloud projects describe poly-market-analysis --format='value(projectNumber)')
SA="${PROJECT_NUMBER}-compute@developer.gserviceaccount.com"

gcloud secrets add-iam-policy-binding mongodb-uri \
  --member="serviceAccount:${SA}" \
  --role='roles/secretmanager.secretAccessor'

gcloud secrets add-iam-policy-binding mongodb-db \
  --member="serviceAccount:${SA}" \
  --role='roles/secretmanager.secretAccessor'

gcloud beta run services add-iam-policy-binding regime-sentinel-agent \
  --region asia-northeast3 \
  --member=allUsers \
  --role=roles/run.invoker
```

Expected existing Secret Manager secrets:

- `mongodb-uri`
- `mongodb-db`

Optional Gemini secret/env can be added later after a real API key is available:

```bash
gcloud run services update regime-sentinel-agent \
  --region asia-northeast3 \
  --set-env-vars GEMINI_ENABLED=true,GEMINI_SUMMARY_INTERVAL_MINUTES=30,GEMINI_MAX_CALLS_PER_HOUR=2 \
  --set-secrets GEMINI_API_KEY=gemini-api-key:latest
```

## Agent Builder

The deployed OpenAPI spec is served from:

```text
https://regime-sentinel-agent-998092298764.asia-northeast3.run.app/api/openapi.json
```

It exposes read/demo-safe operations for health, dashboard snapshot, replay
validation, current regime, recent alerts, similar windows, backtest metrics,
and stored market summaries. The hosted spec uses OpenAPI `3.0.3`, includes the
Cloud Run server URL, and declares the replay validation JSON request body.

Configured Google Cloud resources:

- Agent Builder resources should be created in `asia-northeast3`; Tokyo
  `asia-northeast1` Agent Builder resources are no longer the target.

## Live Smoke

Run the GCP-network live smoke from Cloud Build:

```bash
gcloud builds submit --region asia-northeast3 --config cloudbuild.live-smoke.yaml
```

The smoke discovers three current `btc-updown-5m-{epoch}` events, subscribes to
their Polymarket CLOB market streams plus Chainlink `btc/usd`, and writes JSON
artifacts to `gs://poly-market-analysis-asia-northeast3-cloudbuild/live-smoke/$BUILD_ID/`.

Run the four-hour Seoul-network collection volume soak from Cloud Build:

```bash
MONGODB_URI='...' MONGODB_DB=regime_sentinel \
  cargo run -q -p regime-service --bin mongodb_storage_report \
  > /tmp/regime-storage-before.json

gcloud builds submit --region asia-northeast3 --config cloudbuild.live-soak.yaml

MONGODB_URI='...' MONGODB_DB=regime_sentinel \
  cargo run -q -p regime-service --bin mongodb_storage_report \
  > /tmp/regime-storage-after.json
```

The soak rotates through active BTC 5 minute markets, records Polymarket CLOB
ticks plus Chainlink `btc/usd`, and keeps only the most recent 12 market windows
locally before uploading to:

```text
gs://poly-market-analysis-asia-northeast3-cloudbuild/live-soak/latest/
```

`summary.json` reports duration, market ticks, reference ticks, stale states,
and NDJSON bytes for the retained one-hour window only.
The `mongodb_storage_report` before/after files provide the MongoDB Atlas
`dataSize`, `storageSize`, index size, and collection counts. MongoDB ingest is
disabled by default for the soak because the raw CLOB tick rate can overload the
512MB Atlas tier; use `_INGEST_MONGODB=true` only for a deliberately bounded test.
When ingest is enabled, `ROLLING_MARKET_LIMIT` defaults to 12 and old market
slugs are deleted from `market_ticks`, `feature_windows`, `alerts`, and
`regime_states` after each completed market window.

Latest passing run:

- build id: `631222a7-e26a-4971-8376-6198059b5c44`
- artifact prefix:
  `gs://poly-market-analysis_cloudbuild/live-smoke/631222a7-e26a-4971-8376-6198059b5c44/`
- windows: `btc-updown-5m-1779483000`, `btc-updown-5m-1779483300`,
  `btc-updown-5m-1779483600`
- result: `summary.json` had `passed: true`

## Validation Gates

The project is accepted only when these gates are backed by artifacts:

- Hosted web app is reachable.
- MongoDB Atlas collections are written and queryable.
- Agent Builder is configured with the hosted OpenAPI tool and playbook.
- MongoDB MCP read-only integration template is present and verified locally.
- Strict fair-probability replay/API input path is covered by tests; raw
  `feature_windows[].p_fair` is kept only for legacy replay fixtures.
- Generated alerts are deduplicated by market/direction/onset bucket and use
  cooldown by default on the main alert-generation path.
- Gemini is actually used for summaries.
- Replay mode can reproduce a fixed high-volatility window.
- Strict replay acceptance covers 1s, 5s, and 30s horizons with early alerts and
  median lead time at or above 5s.
- Hot-path alert generation p95 latency is below 500ms.
- Validation report includes lead time, false alerts, precision, recall, horizon PR-AUC, and ablation.

See `docs/acceptance.md` for the current gate status and known external blockers.

## License

MIT
