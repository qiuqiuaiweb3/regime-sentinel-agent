import { describe, expect, it } from 'vitest';

import { snapshotToDashboardView, type DashboardSnapshot } from './dashboard';

const snapshot: DashboardSnapshot = {
  regime: {
    state: 'EARLY_RISK',
    confidence: 'Normal',
    updated_at_ms: 1_769_000_000_750
  },
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
    enabled: false,
    generated_at_ms: null,
    summary: 'Gemini summaries are disabled by default.'
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
  });
});
