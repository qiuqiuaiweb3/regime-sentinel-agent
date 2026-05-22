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
