# Changelog

## 2026-05-23

### Added

- Added live dashboard regime context:
  - current market title and event slug
  - current regime description
  - live regime indicator panel for fair gap, 1s/5s midpoint velocity,
    1s order flow, BTC reference velocity, and shift score
  - state formula and threshold rules below the chart
- Added visual emphasis for `STATE` changes so regime transitions are easier to
  notice during the demo.
- Added visible manual Gemini response handling in the dashboard. `Explain now`
  now shows loading, generated text, cooldown, rate-limit, disabled, and failure
  states instead of failing silently.
- Added live-dashboard support for local NDJSON fallback files so the UI can keep
  rendering live collector output when MongoDB is unavailable.
- Added SOCKS5 WebSocket support for the Polymarket/Coinbase live collector. The
  local demo uses `SOCKS5_PROXY=socks5://127.0.0.1:10808` for market-data
  WebSockets without forcing Gemini HTTP traffic through `ALL_PROXY`.
- Added Gemini request regression coverage for Vertex AI regional path handling.

### Fixed

- Fixed `Explain now` returning `502 Bad Gateway` when Gemini 3 Flash was called
  through the regional Vertex host. Korea/`asia-northeast3` is still the selected
  Gemini location; the request uses the global Vertex host
  `https://aiplatform.googleapis.com/v1` while preserving
  `locations/asia-northeast3` in the resource path.
- Kept Gemini 3 request bodies using `thinkingConfig.thinkingLevel`, while
  omitting that field for Gemini 2.5 models that reject it.
- Prevented process-wide proxy settings from breaking Gemini `reqwest` calls
  during local demos.

### Verified

- Real Vertex AI call verified:
  - `GEMINI_MODEL=gemini-3-flash-preview`
  - `GEMINI_LOCATION=asia-northeast3`
  - `POST /api/agent/explain-now` returned HTTP `200` with generated Gemini text.
- Live local dashboard verified against a current BTC 5m market:
  - market slug and title rendered
  - `price_points` populated with live points
  - regime state changed between `BALANCED_LIVE` and `UP_PRESSURE`
  - six live regime indicators were returned by `/api/dashboard/snapshot`
- Verification commands run:
  - `npm test -- --run`
  - `npm run check`
  - `npm run build`
  - `cargo fmt --check`
  - `cargo test -p regime-service`
  - `cargo clippy -p regime-service --all-targets -- -D warnings`
  - `cargo build -p regime-service --bin regime-service`

### Operations

- Local project service was safely stopped after testing.
- `127.0.0.1:8080` is no longer listening.
- GCP read-only check found no ongoing Cloud Build jobs, no Compute Engine VM,
  no Cloud Run Jobs, and disabled Cloud SQL/GKE APIs.
- Existing Cloud Run service, Artifact Registry repository, Secret Manager
  secrets, Cloud Build bucket, Vertex AI/Gemini API, and MongoDB Atlas are safe
  to keep for the hackathon demo. MongoDB Atlas cost still depends on the Atlas
  tier selected outside GCP.
