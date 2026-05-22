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
- Google Cloud defaults target `asia-northeast1`.
- MongoDB and Gemini configuration are provided through environment variables.
- Replay validation can consume prebuilt alerts or generate alerts from feature windows.
- MongoDB collection/index bootstrap is available through an explicit CLI.
- MongoDB document writers exist for market ticks, feature windows, regime states, alerts,
  agent summaries, and backtest runs.
- Axum serves the SvelteKit dashboard, REST API, SSE stream, and OpenAPI tool spec.
- Live collector code supports Polymarket CLOB market data, Coinbase BTC reference prices,
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

Gemini is intentionally disabled by default. The default provider is Vertex AI.
For a local one-off summary, set:

```bash
GEMINI_ENABLED=true
GEMINI_PROVIDER=vertex
GEMINI_LOCATION=global
GEMINI_MODEL=gemini-3-flash-preview
GEMINI_ACCESS_TOKEN="$(gcloud auth print-access-token)"
GEMINI_SUMMARY_INTERVAL_MINUTES=30
GEMINI_MAX_CALLS_PER_HOUR=2
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
REFERENCE_PRICE_PRODUCT_ID=BTC-USD
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

The deployment target is Tokyo:

```bash
gcloud config set project poly-market-analysis
gcloud builds submit --config cloudbuild.yaml
```

The Cloud Build file creates an Artifact Registry Docker repository in
`asia-northeast1`, builds the SvelteKit static frontend and Rust service image,
pushes the image, and deploys Cloud Run with `LIVE_COLLECTOR_ENABLED=false` and
`GEMINI_ENABLED=false`.

Cloud Run resource settings are explicit in `cloudbuild.yaml`:

- region: `asia-northeast1`
- service account: `998092298764-compute@developer.gserviceaccount.com`
- CPU / memory: `1` vCPU / `1Gi`
- instances: min `1`, max `1`
- concurrency: `80`
- request timeout: `3600s`
- Secret Manager injection: `mongodb-uri`, `mongodb-db`

Current hosted URL:

```text
https://regime-sentinel-agent-998092298764.asia-northeast1.run.app
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
  --region asia-northeast1 \
  --member=allUsers \
  --role=roles/run.invoker
```

Expected existing Secret Manager secrets:

- `mongodb-uri`
- `mongodb-db`

Optional Gemini secret/env can be added later after a real API key is available:

```bash
gcloud run services update regime-sentinel-agent \
  --region asia-northeast1 \
  --set-env-vars GEMINI_ENABLED=true,GEMINI_SUMMARY_INTERVAL_MINUTES=30,GEMINI_MAX_CALLS_PER_HOUR=2 \
  --set-secrets GEMINI_API_KEY=gemini-api-key:latest
```

## Agent Builder

The deployed OpenAPI spec is served from:

```text
https://regime-sentinel-agent-998092298764.asia-northeast1.run.app/api/openapi.json
```

It exposes read/demo-safe operations for health, dashboard snapshot, replay
validation, current regime, recent alerts, similar windows, backtest metrics,
and stored market summaries. The hosted spec uses OpenAPI `3.0.3`, includes the
Cloud Run server URL, and declares the replay validation JSON request body.

Configured Google Cloud resources:

- agent:
  `projects/poly-market-analysis/locations/asia-northeast1/agents/3e5926c5-ed12-40de-a944-b66fae7fe1e0`
- OpenAPI tool:
  `projects/poly-market-analysis/locations/asia-northeast1/agents/3e5926c5-ed12-40de-a944-b66fae7fe1e0/tools/83dd74d9-d114-433d-b46d-6dd4a055bc48`
- playbook:
  `projects/poly-market-analysis/locations/asia-northeast1/agents/3e5926c5-ed12-40de-a944-b66fae7fe1e0/playbooks/c186ebd4-adcb-4ea6-b400-893768e30ff4`

## Live Smoke

Run the GCP-network live smoke from Cloud Build:

```bash
gcloud builds submit --config cloudbuild.live-smoke.yaml
```

The smoke discovers three current `btc-updown-5m-{epoch}` events, subscribes to
their Polymarket CLOB market streams plus Coinbase `BTC-USD`, and writes JSON
artifacts to `gs://poly-market-analysis_cloudbuild/live-smoke/$BUILD_ID/`.

Latest passing run:

- build id: `e5936042-2ad1-4afc-8a19-4fcbcf445cb4`
- artifact prefix:
  `gs://poly-market-analysis_cloudbuild/live-smoke/e5936042-2ad1-4afc-8a19-4fcbcf445cb4/`
- windows: `btc-updown-5m-1779477300`, `btc-updown-5m-1779477600`,
  `btc-updown-5m-1779477900`
- result: `summary.json` had `passed: true`

## Validation Gates

The project is accepted only when these gates are backed by artifacts:

- Hosted web app is reachable.
- MongoDB Atlas collections are written and queryable.
- Agent Builder is configured with the hosted OpenAPI tool and playbook.
- Gemini is actually used for summaries.
- Replay mode can reproduce a fixed high-volatility window.
- Strict replay acceptance covers 1s, 5s, and 30s horizons with early alerts and
  median lead time at or above 5s.
- Hot-path alert generation p95 latency is below 500ms.
- Validation report includes lead time, false alerts, precision, recall, horizon PR-AUC, and ablation.

See `docs/acceptance.md` for the current gate status and known external blockers.

## License

MIT
