# Regime Sentinel Agent

Regime Sentinel Agent is a Google Cloud Rapid Agent Hackathon / MongoDB Track project.
It monitors Polymarket BTC 5 minute Up/Down markets, emits deterministic regime-shift
warnings on the hot path, stores market memory in MongoDB, and uses Agent Builder plus
Gemini for low-frequency explanations.

## Current Status

This repository is the independent hackathon implementation. It is not a copy of
`poly-market-analysis` or `poly-tx`; prior work is used only as interface and design
experience.

Phase 0 scaffold is in progress:

- Rust workspace with `regime-core`, `regime-service`, and `regime-replay`.
- Google Cloud defaults target `asia-northeast1`.
- MongoDB and Gemini configuration are provided through environment variables.
- Gemini calls are cost-limited by default.

## Architecture

- Hot path: market events -> feature engine -> warning engine -> MongoDB -> REST/SSE dashboard.
- Cold path: MongoDB memory -> Agent Builder tools / MongoDB MCP -> Gemini summary -> MongoDB.
- Dashboard: SvelteKit static client with TradingView Lightweight Charts, served by Axum.

## Local Setup

```bash
cp .env.example .env
cargo build
cargo test
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

## Validation Gates

The project is accepted only when these gates are backed by artifacts:

- Hosted web app is reachable.
- MongoDB Atlas collections are written and queryable.
- Agent Builder and Gemini are actually used for summaries.
- Replay mode can reproduce a fixed high-volatility window.
- At least one replay alert has `alert_time < shift_onset_time`.
- Validation report includes lead time, false alerts, precision, recall, horizon PR-AUC, and ablation.

## License

MIT
