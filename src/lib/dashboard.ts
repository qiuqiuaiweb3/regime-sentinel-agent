import type { LineData, SeriesMarker, UTCTimestamp } from 'lightweight-charts';

export type DashboardRegime = {
  state: string;
  confidence: string;
  updated_at_ms: number;
};

export type DashboardPricePoint = {
  timestamp_ms: number;
  p_mid: number;
  p_fair: number;
};

export type DashboardAlert = {
  timestamp_ms: number;
  state: string;
  lead_time_ms: number;
  score: number;
};

export type DashboardGeminiSummary = {
  enabled: boolean;
  generated_at_ms: number | null;
  coverage: string;
  summary: string;
};

export type DashboardSimilarWindow = {
  slug: string;
  window_ts_ms: number;
  score: number;
  fair_gap: number;
  ofi_1s: number;
  depth_imbalance: number;
};

export type DashboardValidationHorizon = {
  horizon_ms: number;
  pr_auc: number;
};

export type DashboardValidation = {
  median_lead_time_ms: number | null;
  p75_lead_time_ms: number | null;
  precision: number;
  recall: number;
  degraded_confidence: boolean;
  reason: string;
  horizons: DashboardValidationHorizon[];
};

export type DashboardSnapshot = {
  mode: 'live' | 'replay';
  regime: DashboardRegime;
  price_points: DashboardPricePoint[];
  alerts: DashboardAlert[];
  gemini_summary: DashboardGeminiSummary;
  similar_windows: DashboardSimilarWindow[];
  validation: DashboardValidation;
};

type PartialDashboardSnapshot = {
  mode?: 'live' | 'replay';
  regime: DashboardRegime;
  price_points: DashboardPricePoint[];
  alerts: DashboardAlert[];
  gemini_summary: {
    enabled: boolean;
    generated_at_ms: number | null;
    coverage?: string;
    summary: string;
  };
  similar_windows?: DashboardSimilarWindow[];
  validation?: Partial<DashboardValidation> & {
    horizons?: DashboardValidationHorizon[];
  };
};

export type AlertRow = {
  time: string;
  state: string;
  lead: string;
  score: string;
};

export type SimilarWindowRow = {
  time: string;
  slug: string;
  score: string;
  fairGap: string;
  orderFlow: string;
  depth: string;
};

export type ValidationRow = {
  horizon: string;
  prAuc: string;
};

export type DashboardView = {
  priceData: LineData<UTCTimestamp>[];
  fairData: LineData<UTCTimestamp>[];
  markers: SeriesMarker<UTCTimestamp>[];
  alertRows: AlertRow[];
  similarRows: SimilarWindowRow[];
  validationRows: ValidationRow[];
  state: string;
  confidence: string;
  currentLead: string;
  geminiEnabled: boolean;
  geminiSummary: string;
  geminiGeneratedAt: string;
  geminiCoverage: string;
  degradedConfidence: boolean;
  validationReason: string;
  validationSummary: string;
};

export const fallbackDashboardSnapshot: DashboardSnapshot = {
  mode: 'live',
  regime: {
    state: 'EARLY_RISK',
    confidence: 'Normal',
    updated_at_ms: 1_769_000_000_750
  },
  price_points: [
    { timestamp_ms: 1_769_000_000_000, p_mid: 0.5, p_fair: 0.49 },
    { timestamp_ms: 1_769_000_000_750, p_mid: 0.54, p_fair: 0.49 },
    { timestamp_ms: 1_769_000_001_000, p_mid: 0.62, p_fair: 0.51 }
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
    summary:
      'Cached demo summary: early risk increased because fair-gap velocity, order flow, and depth imbalance moved in the same direction.'
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
      { horizon_ms: 5000, pr_auc: 0 },
      { horizon_ms: 30000, pr_auc: 0 }
    ]
  }
};

export async function fetchDashboardSnapshot(
  fetcher: typeof fetch = fetch,
  mode: 'live' | 'replay' = 'live'
): Promise<DashboardSnapshot> {
  const response = await fetcher(`/api/dashboard/snapshot?mode=${mode}`);
  if (!response.ok) {
    throw new Error(`dashboard snapshot request failed: ${response.status}`);
  }
  return normalizeDashboardSnapshot((await response.json()) as PartialDashboardSnapshot);
}

export function normalizeDashboardSnapshot(snapshot: PartialDashboardSnapshot): DashboardSnapshot {
  return {
    mode: snapshot.mode ?? 'live',
    regime: snapshot.regime,
    price_points: snapshot.price_points,
    alerts: snapshot.alerts,
    gemini_summary: {
      enabled: snapshot.gemini_summary.enabled,
      generated_at_ms: snapshot.gemini_summary.generated_at_ms,
      coverage: snapshot.gemini_summary.coverage ?? 'not generated',
      summary: snapshot.gemini_summary.summary
    },
    similar_windows: snapshot.similar_windows ?? [],
    validation: {
      median_lead_time_ms: snapshot.validation?.median_lead_time_ms ?? null,
      p75_lead_time_ms: snapshot.validation?.p75_lead_time_ms ?? null,
      precision: snapshot.validation?.precision ?? 0,
      recall: snapshot.validation?.recall ?? 0,
      degraded_confidence: snapshot.validation?.degraded_confidence ?? false,
      reason: snapshot.validation?.reason ?? 'No validation report is available.',
      horizons: snapshot.validation?.horizons ?? []
    }
  };
}

export function snapshotToDashboardView(snapshot: DashboardSnapshot): DashboardView {
  const baseTimestamp = snapshot.price_points[0]?.timestamp_ms ?? snapshot.regime.updated_at_ms;

  return {
    priceData: snapshot.price_points.map((point) => ({
      time: toChartTime(point.timestamp_ms),
      value: point.p_mid
    })),
    fairData: snapshot.price_points.map((point) => ({
      time: toChartTime(point.timestamp_ms),
      value: point.p_fair
    })),
    markers: snapshot.alerts.map((alert) => ({
      time: toChartTime(alert.timestamp_ms),
      position: 'aboveBar',
      color: '#b91c1c',
      shape: 'arrowDown',
      text: `${alert.state} ${formatLeadTime(alert.lead_time_ms, false)}`
    })),
    alertRows: snapshot.alerts.map((alert) => ({
      time: formatRelativeTime(alert.timestamp_ms, baseTimestamp),
      state: alert.state,
      lead: formatLeadTime(alert.lead_time_ms, true),
      score: alert.score.toFixed(2)
    })),
    similarRows: snapshot.similar_windows.map((window) => ({
      time: formatRelativeTime(window.window_ts_ms, baseTimestamp),
      slug: window.slug,
      score: window.score.toFixed(2),
      fairGap: window.fair_gap.toFixed(3),
      orderFlow: window.ofi_1s.toFixed(3),
      depth: window.depth_imbalance.toFixed(3)
    })),
    validationRows: snapshot.validation.horizons.map((horizon) => ({
      horizon: formatHorizon(horizon.horizon_ms),
      prAuc: horizon.pr_auc.toFixed(3)
    })),
    state: snapshot.regime.state,
    confidence: snapshot.regime.confidence,
    currentLead:
      snapshot.alerts.length > 0 ? formatLeadTime(snapshot.alerts[0].lead_time_ms, true) : 'pending',
    geminiEnabled: snapshot.gemini_summary.enabled,
    geminiSummary: snapshot.gemini_summary.summary,
    geminiGeneratedAt:
      snapshot.gemini_summary.generated_at_ms === null
        ? 'not generated'
        : formatRelativeTime(snapshot.gemini_summary.generated_at_ms, baseTimestamp),
    geminiCoverage: snapshot.gemini_summary.coverage,
    degradedConfidence: snapshot.validation.degraded_confidence,
    validationReason: snapshot.validation.reason,
    validationSummary: `median ${formatOptionalLeadTime(
      snapshot.validation.median_lead_time_ms
    )} · p75 ${formatOptionalLeadTime(snapshot.validation.p75_lead_time_ms)} · precision ${snapshot.validation.precision.toFixed(
      3
    )} · recall ${snapshot.validation.recall.toFixed(3)}`
  };
}

function toChartTime(timestampMs: number): UTCTimestamp {
  return (timestampMs / 1000) as UTCTimestamp;
}

function formatRelativeTime(timestampMs: number, baseTimestampMs: number): string {
  const deltaSeconds = (timestampMs - baseTimestampMs) / 1000;
  const sign = deltaSeconds >= 0 ? '+' : '-';
  return `${sign}${Math.abs(deltaSeconds).toFixed(3)}s`;
}

function formatLeadTime(leadTimeMs: number, withSpace: boolean): string {
  const sign = leadTimeMs >= 0 ? '+' : '';
  const separator = withSpace ? ' ' : '';
  return `${sign}${leadTimeMs}${separator}ms`;
}

function formatOptionalLeadTime(leadTimeMs: number | null): string {
  if (leadTimeMs === null) {
    return 'pending';
  }
  return formatLeadTime(leadTimeMs, true);
}

function formatHorizon(horizonMs: number): string {
  if (horizonMs % 1000 === 0) {
    return `${horizonMs / 1000}s`;
  }
  return `${horizonMs}ms`;
}
