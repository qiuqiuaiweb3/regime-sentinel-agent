import { describe, expect, it } from 'vitest';

import {
  fetchDashboardSnapshot,
  normalizeDashboardSnapshot,
  requestManualExplain,
  snapshotToDashboardView,
  type DashboardSnapshot
} from './dashboard';

const snapshot: DashboardSnapshot = {
  regime: {
    state: 'EARLY_RISK',
    confidence: 'Normal',
    updated_at_ms: 1_769_000_000_750
  },
  mode: 'live',
  price_points: [
    { timestamp_ms: 1_769_000_000_000, p_mid: 0.5, p_fair: 0.49 },
    { timestamp_ms: 1_769_000_000_750, p_mid: 0.54, p_fair: 0.49 }
  ],
  alerts: [
    {
      timestamp_ms: 1_769_000_000_750,
      state: 'EARLY_RISK',
      lead_time_ms: 250,
      score: 1.94
    }
  ],
  gemini_summary: {
    enabled: true,
    generated_at_ms: 1_769_000_001_000,
    coverage: 'last 30 minutes',
    summary: 'Cached Gemini summary.'
  },
  similar_windows: [
    {
      slug: 'btc-updown-5m-1768999700',
      window_ts_ms: 1_768_999_700_750,
      score: 0.98,
      fair_gap: 0.05,
      ofi_1s: 0.42,
      depth_imbalance: 0.31
    }
  ],
  validation: {
    median_lead_time_ms: 250,
    p75_lead_time_ms: 250,
    precision: 1,
    recall: 0.333,
    degraded_confidence: true,
    reason: '5s and 30s horizons need more live evidence.',
    horizons: [
      { horizon_ms: 1000, pr_auc: 1 },
      { horizon_ms: 5000, pr_auc: 0 }
    ]
  }
};

describe('snapshotToDashboardView', () => {
  it('converts the service snapshot into chart series and alert rows', () => {
    const view = snapshotToDashboardView(snapshot);

    expect(view.priceData).toEqual([
      { time: 1_769_000_000, value: 0.5 },
      { time: 1_769_000_000.75, value: 0.54 }
    ]);
    expect(view.fairData).toEqual([
      { time: 1_769_000_000, value: 0.49 },
      { time: 1_769_000_000.75, value: 0.49 }
    ]);
    expect(view.markers).toEqual([
      {
        time: 1_769_000_000.75,
        position: 'aboveBar',
        color: '#b91c1c',
        shape: 'arrowDown',
        text: 'EARLY_RISK +250ms'
      }
    ]);
    expect(view.alertRows).toEqual([
      {
        time: '+0.750s',
        state: 'EARLY_RISK',
        lead: '+250 ms',
        score: '1.94'
      }
    ]);
    expect(view.currentLead).toBe('+250 ms');
    expect(view.geminiGeneratedAt).toBe('+1.000s');
    expect(view.geminiCoverage).toBe('last 30 minutes');
    expect(view.similarRows).toEqual([
      {
        time: '-299.250s',
        slug: 'btc-updown-5m-1768999700',
        score: '0.98',
        fairGap: '0.050',
        orderFlow: '0.420',
        depth: '0.310'
      }
    ]);
    expect(view.validationRows).toEqual([
      { horizon: '1s', prAuc: '1.000' },
      { horizon: '5s', prAuc: '0.000' }
    ]);
    expect(view.degradedConfidence).toBe(true);
    expect(view.validationSummary).toBe('median +250 ms · p75 +250 ms · precision 1.000 · recall 0.333');
  });

  it('normalizes older dashboard payloads without optional dashboard extensions', () => {
    const legacySnapshot = normalizeDashboardSnapshot({
      regime: snapshot.regime,
      price_points: snapshot.price_points,
      alerts: [],
      gemini_summary: {
        enabled: false,
        generated_at_ms: null,
        summary: 'Gemini summaries are disabled by default.'
      }
    });

    const view = snapshotToDashboardView(legacySnapshot);

    expect(legacySnapshot.mode).toBe('live');
    expect(view.geminiGeneratedAt).toBe('not generated');
    expect(view.geminiCoverage).toBe('not generated');
    expect(view.similarRows).toEqual([]);
    expect(view.validationRows).toEqual([]);
    expect(view.validationSummary).toBe('median pending · p75 pending · precision 0.000 · recall 0.000');
  });

  it('fetches replay snapshots with the replay mode query parameter', async () => {
    const requests: string[] = [];
    const fetcher = async (url: string) => {
      requests.push(url);
      return {
        ok: true,
        json: async () => snapshot
      } as Response;
    };

    await fetchDashboardSnapshot(fetcher as typeof fetch, 'replay');

    expect(requests).toEqual(['/api/dashboard/snapshot?mode=replay']);
  });

  it('posts manual explain requests without sending market data', async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = [];
    const fetcher = async (url: string, init?: RequestInit) => {
      requests.push({ url, init });
      return {
        ok: true,
        json: async () => ({
          status: 'generated',
          generated_now: true,
          cooldown_seconds: 300
        })
      } as Response;
    };

    const result = await requestManualExplain(fetcher as typeof fetch);

    expect(result.status).toBe('generated');
    expect(requests).toEqual([
      {
        url: '/api/agent/explain-now',
        init: {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: '{}'
        }
      }
    ]);
  });
});
