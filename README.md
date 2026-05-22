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

## Environment

Required later for live/cloud runs:

- `GOOGLE_CLOUD_PROJECT`
- `GOOGLE_CLOUD_REGION`
- `MONGODB_URI`
- `MONGODB_DB`
- `GEMINI_ENABLED`
- `GEMINI_SUMMARY_INTERVAL_MINUTES`
- `GEMINI_MAX_CALLS_PER_HOUR`

Gemini is intentionally disabled by default. To enable summaries, set:

```bash
GEMINI_ENABLED=true
GEMINI_API_KEY=...
GEMINI_SUMMARY_INTERVAL_MINUTES=30
GEMINI_MAX_CALLS_PER_HOUR=2
```

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
- `http://127.0.0.1:8080/api/dashboard/events`
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

Use the hosted Cloud Run URL plus `/api/openapi.json` as the OpenAPI tool source.
The OpenAPI spec exposes read/demo-safe operations for health, dashboard snapshot,
and replay validation.

## Validation Gates

The project is accepted only when these gates are backed by artifacts:

- Hosted web app is reachable.
- MongoDB Atlas collections are written and queryable.
- Agent Builder and Gemini are actually used for summaries.
- Replay mode can reproduce a fixed high-volatility window.
- At least one replay alert has `alert_time < shift_onset_time`.
- Validation report includes lead time, false alerts, precision, recall, horizon PR-AUC, and ablation.

See `docs/acceptance.md` for the current gate status and known external blockers.

## License

MIT
