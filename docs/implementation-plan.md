# Implementation Plan

This repository implements the accepted plan from
`poly-market-analysis/docs/plan/2026-05-22-regime-detection-agent.md`.

## Step Reviews

Every completed step must be reviewed before the next step starts:

1. Run the verification command for that step.
2. Inspect `git diff`.
3. Record the result in the working notes or commit message.
4. Commit locally.

If implementation reality diverges from the plan, stop and ask before continuing.

## Phase Order

1. Phase 0: independent repo, license, README, Rust workspace, env example.
2. Phase 1: replay data model, feature windows, shift labels, alert scoring.
3. Phase 2: MongoDB collections and indexes.
4. Phase 3: live collector.
5. Phase 4: Agent Builder, MongoDB MCP, Gemini summary throttling.
6. Phase 5: validation report, hosted deployment, demo assets.

## Current Completion Snapshot

Updated: 2026-05-23 JST.

### Completed

- Rust workspace, Axum service, replay engine, MongoDB schema/bootstrap, static
  SvelteKit dashboard, Cloud Run deployment, Agent Builder OpenAPI tool, Gemini
  summary throttling, and live Polymarket/Chainlink collector are implemented.
- Dashboard now exposes the demo-critical live regime evidence:
  - market title and event slug
  - current regime description
  - regime indicators for fair gap, midpoint velocity, order flow, BTC velocity,
    shift score, and 5s midpoint velocity
  - state formula and threshold rules
  - animated state card on regime changes
  - half-hour Gemini countdown and latest cached response display
- Local and cloud live collection now use direct Polymarket/Chainlink WebSocket
  connections; SOCKS5/VPN proxy handling has been removed from the collector.
- Vertex AI Gemini 3 Flash manual explain is verified with:
  - `GEMINI_MODEL=gemini-3-flash-preview`
  - `GEMINI_LOCATION=asia-northeast3`
  - Korea/`asia-northeast3` remains the selected Gemini location.
  - HTTP requests use the global Vertex host `https://aiplatform.googleapis.com/v1`
    while the resource path retains `locations/asia-northeast3`.
- Local test service was stopped after the live check; no project process is
  listening on `127.0.0.1:8080`.

### Latest Verification

- Scheduled Gemini summaries are configured for a 30 minute interval and replace
  older cached summary documents.
- `/api/dashboard/snapshot` returned a current BTC 5m market, 120 chart points,
  live regime description, and six regime indicators.
- Verification commands passed:
  - `npm test -- --run`
  - `npm run check`
  - `npm run build`
  - `cargo fmt --check`
  - `cargo test -p regime-service`
  - `cargo clippy -p regime-service --all-targets -- -D warnings`
  - `cargo build -p regime-service --bin regime-service`

### Remaining Gates

- Final English demo video.
- Devpost submission.
- Confirm MongoDB Atlas tier/cost before leaving it running after the hackathon.
