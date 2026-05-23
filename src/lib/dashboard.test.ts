import { describe, expect, it } from 'vitest';

import {
  dashboardRunStateForSnapshot,
  fallbackDashboardSnapshot,
  fetchDashboardSnapshot,
  getDashboardCopy,
  formatGeminiCountdown,
  manualExplainErrorToView,
  manualExplainResponseToView,
  normalizeDashboardSnapshot,
  requestManualExplain,
  snapshotToDashboardView,
  type DashboardSnapshot
} from './dashboard';

const snapshot: DashboardSnapshot = {
  regime: {
    state: 'EARLY_RISK',
    confidence: 'Normal',
    updated_at_ms: 1_769_000_000_750,
    description: 'Up-side pressure is rising before full repricing.'
  },
  market: {
    slug: 'btc-updown-5m-1769000000',
    series: 'btc-updown-5m',
    title: 'Bitcoin Up or Down - demo'
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
    next_update_at_ms: 1_769_001_801_000,
    interval_seconds: 1_800,
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
  regime_indicators: [
    {
      key: 'fair_gap',
      label: 'Fair gap',
      value: 0.05,
      unit: 'pp',
      status: 'elevated',
      description: 'Up midpoint minus neutral fair line.'
    },
    {
      key: 'shift_score',
      label: 'Shift score',
      value: 0.63,
      unit: '',
      status: 'watch',
      description: 'Combined live heuristic score.'
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
  it('defaults dashboard chrome copy to English and supports Chinese labels', () => {
    const defaultCopy = getDashboardCopy();
    const chineseCopy = getDashboardCopy('zh');

    expect(defaultCopy.languageLabel).toBe('Language');
    expect(defaultCopy.liveMode).toBe('LIVE');
    expect(defaultCopy.replayMode).toBe('DEMO REPLAY');
    expect(defaultCopy.currentRegimeTitle).toBe('CURRENT REGIME');
    expect(defaultCopy.refresh).toBe('Refresh View');
    expect(chineseCopy.languageLabel).toBe('语言');
    expect(chineseCopy.liveMode).toBe('实时');
    expect(chineseCopy.replayMode).toBe('演示回放');
    expect(chineseCopy.currentRegimeTitle).toBe('当前状态');
    expect(chineseCopy.refresh).toBe('刷新当前视图');
  });

  it('shows demo fallback instead of streaming when live mode receives demo replay data', () => {
    const liveState = dashboardRunStateForSnapshot(fallbackDashboardSnapshot, 'live', true);
    const replayState = dashboardRunStateForSnapshot(fallbackDashboardSnapshot, 'replay', true);
    const liveSnapshot: DashboardSnapshot = {
      ...snapshot,
      market: {
        ...snapshot.market,
        title: 'Bitcoin Up or Down - live'
      },
      regime: {
        ...snapshot.regime,
        description: 'Live collector view: chart uses Polymarket Up midpoint ticks.'
      }
    };

    expect(liveState).toBe('demo-fallback');
    expect(replayState).toBe('streaming');
    expect(dashboardRunStateForSnapshot(liveSnapshot, 'live', true)).toBe('streaming');
  });

  it('converts the service snapshot into chart series and alert rows', () => {
    const view = snapshotToDashboardView(snapshot, 1_769_000_001_000);

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
    expect(view.marketSlug).toBe('btc-updown-5m-1769000000');
    expect(view.marketTitle).toBe('Bitcoin Up or Down - demo');
    expect(view.regimeDescription).toBe('Up-side pressure is rising before full repricing.');
    expect(view.indicatorRows).toEqual([
      {
        key: 'fair_gap',
        label: 'Fair gap',
        value: '+0.050 pp',
        status: 'elevated',
        description: 'Up midpoint minus neutral fair line.'
      },
      {
        key: 'shift_score',
        label: 'Shift score',
        value: '0.630',
        status: 'watch',
        description: 'Combined live heuristic score.'
      }
    ]);
    expect(view.formulaRows).toEqual([
      { key: 'p_mid', label: 'Up midpoint', value: '0.540' },
      { key: 'fair_gap', label: 'Fair gap', value: '+0.050 pp' },
      { key: 'mid_velocity_1s', label: 'Mid velocity 1s', value: 'missing' },
      { key: 'order_flow_1s', label: 'Order flow 1s', value: 'missing' },
      { key: 'btc_velocity_1s', label: 'BTC velocity 1s', value: 'missing' },
      { key: 'shift_score', label: 'Shift score', value: '0.630' }
    ]);
    expect(view.stateFormula).toContain('shift_score = clamp');
    expect(view.stateRules[0]).toBe('SHIFT_RISK when shift_score >= 0.75');
    expect(view.geminiGeneratedAt).toBe('+1.000s');
    expect(view.geminiCoverage).toBe('last 30 minutes');
    expect(view.geminiCountdown).toBe('30:00');
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

  it('derives a watch forecast from the clamped shift score', () => {
    const view = snapshotToDashboardView(snapshot);

    expect(view.shiftScoreRaw).toBe(0.63);
    expect(view.shiftScoreClamped).toBe(0.63);
    expect(view.shiftForecastLabel).toBe('WATCH');
    expect(view.shiftScoreBar).toBe('[######----]');
    expect(view.stateTone).toBe('warn');
    expect(view.displayRegimeLabel).toBe('EARLY_RISK');
    expect(view.sourceRegimeLabel).toBe('EARLY_RISK');
  });

  it('maps high early risk to a shift risk headline without rewriting raw states', () => {
    const highRiskSnapshot: DashboardSnapshot = {
      ...snapshot,
      regime_indicators: snapshot.regime_indicators.map((indicator) =>
        indicator.key === 'shift_score' ? { ...indicator, value: 0.82, status: 'high' } : indicator
      )
    };

    const view = snapshotToDashboardView(highRiskSnapshot);

    expect(view.shiftScoreRaw).toBe(0.82);
    expect(view.shiftScoreClamped).toBe(0.82);
    expect(view.shiftForecastLabel).toBe('LIKELY SHIFT');
    expect(view.shiftScoreBar).toBe('[########--]');
    expect(view.stateTone).toBe('risk');
    expect(view.displayRegimeLabel).toBe('SHIFT_RISK');
    expect(view.sourceRegimeLabel).toBe('EARLY_RISK');
    expect(view.state).toBe('EARLY_RISK');
    expect(view.alertRows[0].state).toBe('EARLY_RISK');
    expect(view.markers[0].text).toBe('EARLY_RISK +250ms');
  });

  it('falls back safely when shift score is missing', () => {
    const noScoreSnapshot: DashboardSnapshot = {
      ...snapshot,
      regime: {
        ...snapshot.regime,
        state: 'BALANCED_LIVE'
      },
      regime_indicators: snapshot.regime_indicators.filter(
        (indicator) => indicator.key !== 'shift_score'
      )
    };

    const view = snapshotToDashboardView(noScoreSnapshot);

    expect(view.shiftScoreRaw).toBeNull();
    expect(view.shiftScoreClamped).toBeNull();
    expect(view.shiftForecastLabel).toBe('NO SIGNAL');
    expect(view.shiftScoreBar).toBe('[----------]');
    expect(view.stateTone).toBe('ok');
    expect(view.displayRegimeLabel).toBe('BALANCED_LIVE');
  });

  it('clamps shift score before deriving forecast labels and bars', () => {
    const highSnapshot: DashboardSnapshot = {
      ...snapshot,
      regime_indicators: snapshot.regime_indicators.map((indicator) =>
        indicator.key === 'shift_score' ? { ...indicator, value: 1.2 } : indicator
      )
    };
    const lowSnapshot: DashboardSnapshot = {
      ...snapshot,
      regime: {
        ...snapshot.regime,
        state: 'BALANCED_LIVE'
      },
      regime_indicators: snapshot.regime_indicators.map((indicator) =>
        indicator.key === 'shift_score' ? { ...indicator, value: -0.2 } : indicator
      )
    };

    const highView = snapshotToDashboardView(highSnapshot);
    const lowView = snapshotToDashboardView(lowSnapshot);

    expect(highView.shiftScoreRaw).toBe(1.2);
    expect(highView.shiftScoreClamped).toBe(1);
    expect(highView.shiftScoreBar).toBe('[##########]');
    expect(highView.shiftForecastLabel).toBe('LIKELY SHIFT');
    expect(lowView.shiftScoreRaw).toBe(-0.2);
    expect(lowView.shiftScoreClamped).toBe(0);
    expect(lowView.shiftScoreBar).toBe('[----------]');
    expect(lowView.shiftForecastLabel).toBe('STABLE');
  });

  it('normalizes older dashboard payloads without optional dashboard extensions', () => {
    const { description: _description, ...legacyRegime } = snapshot.regime;
    const legacySnapshot = normalizeDashboardSnapshot({
      regime: legacyRegime,
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
    expect(view.marketSlug).toBe('unknown-market');
    expect(view.regimeDescription).toBe('No live regime description is available yet.');
    expect(view.indicatorRows).toEqual([]);
    expect(view.formulaRows[0]).toEqual({ key: 'p_mid', label: 'Up midpoint', value: '0.540' });
    expect(view.geminiGeneratedAt).toBe('not generated');
    expect(view.geminiCoverage).toBe('not generated');
    expect(view.geminiCountdown).toBe('waiting');
    expect(view.similarRows).toEqual([]);
    expect(view.validationRows).toEqual([]);
    expect(view.validationSummary).toBe('median pending · p75 pending · precision 0.000 · recall 0.000');
  });

  it('formats scheduled Gemini countdowns from the next update timestamp', () => {
    expect(formatGeminiCountdown(1_769_001_801_000, 1_769_000_001_000)).toBe('30:00');
    expect(formatGeminiCountdown(1_769_000_062_000, 1_769_000_001_000)).toBe('01:01');
    expect(formatGeminiCountdown(1_769_000_001_000, 1_769_000_002_000)).toBe('due now');
    expect(formatGeminiCountdown(null, 1_769_000_002_000)).toBe('waiting');
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

  it('formats generated Gemini manual explain responses for immediate display', () => {
    const view = manualExplainResponseToView({
      status: 'generated',
      generated_now: true,
      cooldown_seconds: 900,
      summary: {
        summary: 'Gemini says Up pressure is fading.'
      }
    });

    expect(view).toEqual({
      tone: 'ok',
      title: 'Gemini response',
      message: 'Gemini says Up pressure is fading.',
      detail: 'Manual explain generated. Cooldown 900s.'
    });
  });

  it('formats cooldown responses so the button does not look inert', () => {
    const view = manualExplainErrorToView(
      Object.assign(new Error('manual explain request failed: 429'), {
        payload: {
          status: 'cooldown',
          generated_now: false,
          retry_after_seconds: 609,
          cooldown_seconds: 900
        }
      })
    );

    expect(view).toEqual({
      tone: 'warn',
      title: 'Gemini cooldown',
      message: 'Manual explain is cooling down. Try again in 609s.',
      detail: 'Configured cooldown 900s.'
    });
  });
});
