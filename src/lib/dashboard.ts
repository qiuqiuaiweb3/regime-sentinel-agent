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
  summary: string;
};

export type DashboardSnapshot = {
  regime: DashboardRegime;
  price_points: DashboardPricePoint[];
  alerts: DashboardAlert[];
  gemini_summary: DashboardGeminiSummary;
};

export type AlertRow = {
  time: string;
  state: string;
  lead: string;
  score: string;
};

export type DashboardView = {
  priceData: LineData<UTCTimestamp>[];
  fairData: LineData<UTCTimestamp>[];
  markers: SeriesMarker<UTCTimestamp>[];
  alertRows: AlertRow[];
  state: string;
  confidence: string;
  currentLead: string;
  geminiEnabled: boolean;
  geminiSummary: string;
};

export const fallbackDashboardSnapshot: DashboardSnapshot = {
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
    enabled: false,
    generated_at_ms: null,
    summary: 'Gemini summaries are disabled by default.'
  }
};

export async function fetchDashboardSnapshot(fetcher: typeof fetch = fetch): Promise<DashboardSnapshot> {
  const response = await fetcher('/api/dashboard/snapshot');
  if (!response.ok) {
    throw new Error(`dashboard snapshot request failed: ${response.status}`);
  }
  return (await response.json()) as DashboardSnapshot;
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
    state: snapshot.regime.state,
    confidence: snapshot.regime.confidence,
    currentLead:
      snapshot.alerts.length > 0 ? formatLeadTime(snapshot.alerts[0].lead_time_ms, true) : 'pending',
    geminiEnabled: snapshot.gemini_summary.enabled,
    geminiSummary: snapshot.gemini_summary.summary
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
