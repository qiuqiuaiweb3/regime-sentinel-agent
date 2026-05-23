import type { LineData, SeriesMarker, UTCTimestamp } from 'lightweight-charts';

export type DashboardRegime = {
  state: string;
  confidence: string;
  updated_at_ms: number;
  description: string;
};

export type DashboardMarket = {
  slug: string;
  series: string;
  title: string;
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
  next_update_at_ms: number | null;
  interval_seconds: number | null;
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

export type DashboardRegimeIndicator = {
  key: string;
  label: string;
  value: number;
  unit: string;
  status: string;
  description: string;
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

export type ManualExplainResponse = {
  status: string;
  generated_now: boolean;
  source?: string;
  cooldown_seconds?: number;
  retry_after_seconds?: number;
  max_calls_per_hour?: number;
  reason?: string;
  error?: string;
  summary?: unknown;
};

export type DashboardSnapshot = {
  mode: 'live' | 'replay';
  market: DashboardMarket;
  regime: DashboardRegime;
  price_points: DashboardPricePoint[];
  alerts: DashboardAlert[];
  gemini_summary: DashboardGeminiSummary;
  similar_windows: DashboardSimilarWindow[];
  regime_indicators: DashboardRegimeIndicator[];
  validation: DashboardValidation;
};

export type DashboardLanguage = 'en' | 'zh';
export type DashboardRunState =
  | 'waiting-live-data'
  | 'demo-fallback'
  | 'loading'
  | 'snapshot'
  | 'streaming'
  | 'stream-error';

export type DashboardCopy = {
  languageLabel: string;
  englishLanguage: string;
  chineseLanguage: string;
  runModeLabel: string;
  liveMode: string;
  replayMode: string;
  refresh: string;
  currentRegimeTitle: string;
  shiftWatch: string;
  confidence: string;
  updatedUtc: string;
  sourceState: string;
  marketSlug: string;
  forecastTitle: string;
  shiftScore: string;
  scoreValue: string;
  noSignal: string;
  threshold: string;
  replayLeadEvidence: string;
  evidenceHorizon: string;
  liveModeCaveat: string;
  replayValidationCaveat: string;
  evidenceSignalsTitle: string;
  signal: string;
  value: string;
  status: string;
  noIndicators: string;
  marketWindowTitle: string;
  marketWindowDescription: string;
  alertsTitle: string;
  state: string;
  relTime: string;
  score: string;
  lead: string;
  noAlerts: string;
  mongoMemoryTitle: string;
  slug: string;
  time: string;
  gap: string;
  flow: string;
  depth: string;
  noSimilarWindows: string;
  geminiTitle: string;
  geminiAvailable: string;
  nextGeminiUpdate: string;
  generated: string;
  validationTitle: string;
  horizon: string;
  source: string;
  deterministicReplay: string;
  noReplayRows: string;
  systemProofTitle: string;
};

const dashboardCopy: Record<DashboardLanguage, DashboardCopy> = {
  en: {
    languageLabel: 'Language',
    englishLanguage: 'EN',
    chineseLanguage: '中文',
    runModeLabel: 'Run mode',
    liveMode: 'LIVE',
    replayMode: 'DEMO REPLAY',
    refresh: 'Refresh View',
    currentRegimeTitle: 'CURRENT REGIME',
    shiftWatch: 'SHIFT WATCH',
    confidence: 'Confidence',
    updatedUtc: 'Updated UTC',
    sourceState: 'Source state',
    marketSlug: 'Market slug',
    forecastTitle: 'REGIME SHIFT FORECAST',
    shiftScore: 'Shift score',
    scoreValue: 'Score value',
    noSignal: 'no signal',
    threshold: 'threshold',
    replayLeadEvidence: 'Replay lead evidence',
    evidenceHorizon: 'EVIDENCE HORIZON',
    liveModeCaveat: 'Live mode = heuristic warning.',
    replayValidationCaveat:
      'Replay validation = deterministic acceptance evidence, not statistically validated live forecasting.',
    evidenceSignalsTitle: 'EVIDENCE SIGNALS',
    signal: 'Signal',
    value: 'Value',
    status: 'Status',
    noIndicators: 'No live regime indicators yet',
    marketWindowTitle: 'MARKET WINDOW',
    marketWindowDescription: 'p_mid (Polymarket Up midpoint) vs p_fair (fair line)',
    alertsTitle: 'ALERTS',
    state: 'State',
    relTime: 'Rel time',
    score: 'Score',
    lead: 'Lead',
    noAlerts: 'No alerts',
    mongoMemoryTitle: 'MONGODB MARKET MEMORY',
    slug: 'Slug',
    time: 'Time',
    gap: 'Gap',
    flow: 'Flow',
    depth: 'Depth',
    noSimilarWindows: 'No similar windows',
    geminiTitle: 'GEMINI EXPLAIN',
    geminiAvailable: 'Gemini available',
    nextGeminiUpdate: 'Next Gemini update',
    generated: 'generated',
    validationTitle: 'REPLAY ACCEPTANCE EVIDENCE',
    horizon: 'Horizon',
    source: 'Source',
    deterministicReplay: 'deterministic replay',
    noReplayRows: 'No replay validation rows',
    systemProofTitle: 'SYSTEM PROOF - STATIC ACCEPTANCE EVIDENCE'
  },
  zh: {
    languageLabel: '语言',
    englishLanguage: 'EN',
    chineseLanguage: '中文',
    runModeLabel: '运行模式',
    liveMode: '实时',
    replayMode: '演示回放',
    refresh: '刷新当前视图',
    currentRegimeTitle: '当前状态',
    shiftWatch: '转向监控',
    confidence: '置信度',
    updatedUtc: '更新时间 UTC',
    sourceState: '源状态',
    marketSlug: '市场标识',
    forecastTitle: '状态转向预测',
    shiftScore: '转向评分',
    scoreValue: '评分值',
    noSignal: '无信号',
    threshold: '阈值',
    replayLeadEvidence: '回放提前量证据',
    evidenceHorizon: '证据周期',
    liveModeCaveat: '实时模式 = 启发式预警。',
    replayValidationCaveat: '回放验证 = 确定性验收证据，不等于已完成统计验证的实时预测。',
    evidenceSignalsTitle: '证据信号',
    signal: '信号',
    value: '数值',
    status: '状态',
    noIndicators: '暂无实时状态指标',
    marketWindowTitle: '市场窗口',
    marketWindowDescription: 'p_mid（Polymarket Up 中间价） vs p_fair（公允线）',
    alertsTitle: '警报',
    state: '状态',
    relTime: '相对时间',
    score: '评分',
    lead: '提前量',
    noAlerts: '暂无警报',
    mongoMemoryTitle: 'MongoDB 市场记忆',
    slug: '标识',
    time: '时间',
    gap: '差值',
    flow: '流向',
    depth: '深度',
    noSimilarWindows: '暂无相似窗口',
    geminiTitle: 'Gemini 解释',
    geminiAvailable: 'Gemini 可用',
    nextGeminiUpdate: '下次 Gemini 更新',
    generated: '生成时间',
    validationTitle: '回放验收证据',
    horizon: '周期',
    source: '来源',
    deterministicReplay: '确定性回放',
    noReplayRows: '暂无回放验证行',
    systemProofTitle: '系统证明 - 静态验收证据'
  }
};

export function getDashboardCopy(language: DashboardLanguage = 'en'): DashboardCopy {
  return dashboardCopy[language];
}

export function dashboardRunStateForSnapshot(
  snapshot: DashboardSnapshot,
  mode: 'live' | 'replay',
  streaming: boolean
): DashboardRunState {
  if (mode === 'live' && isDemoFallbackSnapshot(snapshot)) {
    return 'demo-fallback';
  }

  return streaming ? 'streaming' : 'snapshot';
}

function isDemoFallbackSnapshot(snapshot: DashboardSnapshot) {
  const title = snapshot.market.title.toLowerCase();
  const description = snapshot.regime.description.toLowerCase();
  return title.includes('demo replay') || description.startsWith('demo replay');
}

type PartialDashboardSnapshot = {
  mode?: 'live' | 'replay';
  market?: DashboardMarket;
  regime: Omit<DashboardRegime, 'description'> & { description?: string };
  price_points: DashboardPricePoint[];
  alerts: DashboardAlert[];
  gemini_summary: {
    enabled: boolean;
    generated_at_ms: number | null;
    next_update_at_ms?: number | null;
    interval_seconds?: number | null;
    coverage?: string;
    summary: string;
  };
  similar_windows?: DashboardSimilarWindow[];
  regime_indicators?: DashboardRegimeIndicator[];
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

export type IndicatorRow = {
  key: string;
  label: string;
  value: string;
  status: string;
  description: string;
};

export type FormulaRow = {
  key: string;
  label: string;
  value: string;
};

export type ManualExplainView = {
  tone: 'info' | 'ok' | 'warn' | 'error';
  title: string;
  message: string;
  detail: string;
};

export type ShiftForecastLabel = 'LIKELY SHIFT' | 'WATCH' | 'STABLE' | 'NO SIGNAL';
export type DashboardStateTone = 'risk' | 'warn' | 'ok' | 'neutral';

export type DashboardView = {
  priceData: LineData<UTCTimestamp>[];
  fairData: LineData<UTCTimestamp>[];
  markers: SeriesMarker<UTCTimestamp>[];
  alertRows: AlertRow[];
  similarRows: SimilarWindowRow[];
  indicatorRows: IndicatorRow[];
  formulaRows: FormulaRow[];
  stateFormula: string;
  stateRules: string[];
  validationRows: ValidationRow[];
  marketSlug: string;
  marketSeries: string;
  marketTitle: string;
  state: string;
  confidence: string;
  regimeDescription: string;
  shiftScoreRaw: number | null;
  shiftScoreClamped: number | null;
  shiftForecastLabel: ShiftForecastLabel;
  shiftScoreBar: string;
  stateTone: DashboardStateTone;
  displayRegimeLabel: string;
  sourceRegimeLabel: string;
  currentLead: string;
  geminiEnabled: boolean;
  geminiSummary: string;
  geminiGeneratedAt: string;
  geminiCoverage: string;
  geminiCountdown: string;
  degradedConfidence: boolean;
  validationReason: string;
  validationSummary: string;
};

export const manualExplainIdleView: ManualExplainView = {
  tone: 'info',
  title: 'Scheduled Gemini',
  message: 'Gemini summaries are refreshed on the fixed half-hour schedule.',
  detail: 'The dashboard displays the latest cached response.'
};

export const fallbackDashboardSnapshot: DashboardSnapshot = {
  mode: 'live',
  regime: {
    state: 'EARLY_RISK',
    confidence: 'Normal',
    updated_at_ms: 1_769_000_000_750,
    description:
      'Demo replay regime: Up-side pressure increased before the generated alert marker.'
  },
  market: {
    slug: 'btc-updown-5m-1768999700',
    series: 'btc-updown-5m',
    title: 'Bitcoin Up or Down - demo replay'
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
    next_update_at_ms: 1_769_001_801_000,
    interval_seconds: 1_800,
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
  regime_indicators: [
    {
      key: 'fair_gap',
      label: 'Fair gap',
      value: 0.05,
      unit: 'pp',
      status: 'elevated',
      description: 'Demo fair probability gap used by the replay alert.'
    },
    {
      key: 'mid_velocity_1s',
      label: 'Mid velocity 1s',
      value: 0.08,
      unit: 'pp/s',
      status: 'high',
      description: 'Demo one-second midpoint repricing velocity.'
    },
    {
      key: 'order_flow_1s',
      label: 'Order flow 1s',
      value: 0.42,
      unit: '',
      status: 'elevated',
      description: 'Demo signed flow proxy used by the replay alert.'
    },
    {
      key: 'shift_score',
      label: 'Shift score',
      value: 0.82,
      unit: '',
      status: 'high',
      description: 'Demo combined regime-shift heuristic score.'
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

export async function requestManualExplain(
  fetcher: typeof fetch = fetch
): Promise<ManualExplainResponse> {
  const response = await fetcher('/api/agent/explain-now', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: '{}'
  });
  const payload = (await response.json()) as ManualExplainResponse;
  if (!response.ok) {
    throw Object.assign(new Error(`manual explain request failed: ${response.status}`), {
      payload
    });
  }
  return payload;
}

export function normalizeDashboardSnapshot(snapshot: PartialDashboardSnapshot): DashboardSnapshot {
  return {
    mode: snapshot.mode ?? 'live',
    market: snapshot.market ?? {
      slug: 'unknown-market',
      series: 'unknown-series',
      title: 'Unknown market'
    },
    regime: {
      ...snapshot.regime,
      description:
        snapshot.regime.description ?? 'No live regime description is available yet.'
    },
    price_points: snapshot.price_points,
    alerts: snapshot.alerts,
    gemini_summary: {
      enabled: snapshot.gemini_summary.enabled,
      generated_at_ms: snapshot.gemini_summary.generated_at_ms,
      next_update_at_ms: snapshot.gemini_summary.next_update_at_ms ?? null,
      interval_seconds: snapshot.gemini_summary.interval_seconds ?? null,
      coverage: snapshot.gemini_summary.coverage ?? 'not generated',
      summary: snapshot.gemini_summary.summary
    },
    similar_windows: snapshot.similar_windows ?? [],
    regime_indicators: snapshot.regime_indicators ?? [],
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

export function snapshotToDashboardView(
  snapshot: DashboardSnapshot,
  nowMs: number = Date.now()
): DashboardView {
  const baseTimestamp = snapshot.price_points[0]?.timestamp_ms ?? snapshot.regime.updated_at_ms;
  const shiftScoreRaw =
    snapshot.regime_indicators.find((indicator) => indicator.key === 'shift_score')?.value ?? null;
  const shiftScoreClamped =
    shiftScoreRaw === null ? null : Math.min(Math.max(shiftScoreRaw, 0), 1);
  const shiftForecastLabel = forecastLabelFromScore(shiftScoreClamped);
  const stateTone = stateToneFromSnapshot(snapshot.regime.state, shiftScoreClamped);
  const sourceRegimeLabel = snapshot.regime.state;
  const displayRegimeLabel =
    sourceRegimeLabel === 'EARLY_RISK' && shiftScoreClamped !== null && shiftScoreClamped >= 0.75
      ? 'SHIFT_RISK'
      : sourceRegimeLabel;

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
    indicatorRows: snapshot.regime_indicators.map((indicator) => ({
      key: indicator.key,
      label: indicator.label,
      value: formatIndicatorValue(indicator.value, indicator.unit),
      status: indicator.status,
      description: indicator.description
    })),
    formulaRows: formulaRows(snapshot),
    stateFormula:
      'shift_score = clamp(|fair_gap| * 2 + |mid_velocity_1s| * 4 + |order_flow_1s| * 0.4 + min(|btc_velocity_1s| / 100, 0.3), 0, 1)',
    stateRules: [
      'SHIFT_RISK when shift_score >= 0.75',
      'UP_PRESSURE when p_mid >= 0.56 or (mid_velocity_1s > 0.02 and order_flow_1s > 0.20)',
      'DOWN_PRESSURE when p_mid <= 0.44 or (mid_velocity_1s < -0.02 and order_flow_1s < -0.20)',
      'BALANCED_LIVE otherwise'
    ],
    validationRows: snapshot.validation.horizons.map((horizon) => ({
      horizon: formatHorizon(horizon.horizon_ms),
      prAuc: horizon.pr_auc.toFixed(3)
    })),
    marketSlug: snapshot.market.slug,
    marketSeries: snapshot.market.series,
    marketTitle: snapshot.market.title,
    state: snapshot.regime.state,
    confidence: snapshot.regime.confidence,
    regimeDescription: snapshot.regime.description,
    shiftScoreRaw,
    shiftScoreClamped,
    shiftForecastLabel,
    shiftScoreBar: formatShiftScoreBar(shiftScoreClamped),
    stateTone,
    displayRegimeLabel,
    sourceRegimeLabel,
    currentLead:
      snapshot.alerts.length > 0 ? formatLeadTime(snapshot.alerts[0].lead_time_ms, true) : 'pending',
    geminiEnabled: snapshot.gemini_summary.enabled,
    geminiSummary: snapshot.gemini_summary.summary,
    geminiGeneratedAt:
      snapshot.gemini_summary.generated_at_ms === null
        ? 'not generated'
        : formatRelativeTime(snapshot.gemini_summary.generated_at_ms, baseTimestamp),
    geminiCoverage: snapshot.gemini_summary.coverage,
    geminiCountdown: formatGeminiCountdown(snapshot.gemini_summary.next_update_at_ms, nowMs),
    degradedConfidence: snapshot.validation.degraded_confidence,
    validationReason: snapshot.validation.reason,
    validationSummary: `median ${formatOptionalLeadTime(
      snapshot.validation.median_lead_time_ms
    )} · p75 ${formatOptionalLeadTime(snapshot.validation.p75_lead_time_ms)} · precision ${snapshot.validation.precision.toFixed(
      3
    )} · recall ${snapshot.validation.recall.toFixed(3)}`
  };
}

export function formatGeminiCountdown(nextUpdateAtMs: number | null, nowMs: number): string {
  if (nextUpdateAtMs === null) {
    return 'waiting';
  }

  const remainingSeconds = Math.ceil((nextUpdateAtMs - nowMs) / 1_000);
  if (remainingSeconds <= 0) {
    return 'due now';
  }

  const minutes = Math.floor(remainingSeconds / 60);
  const seconds = remainingSeconds % 60;
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
}

function forecastLabelFromScore(score: number | null): ShiftForecastLabel {
  if (score === null) {
    return 'NO SIGNAL';
  }
  if (score >= 0.75) {
    return 'LIKELY SHIFT';
  }
  if (score >= 0.45) {
    return 'WATCH';
  }
  return 'STABLE';
}

function stateToneFromSnapshot(state: string, score: number | null): DashboardStateTone {
  if (score !== null) {
    if (score >= 0.75) {
      return 'risk';
    }
    if (score >= 0.45) {
      return 'warn';
    }
    return 'ok';
  }

  if (state === 'SHIFT_RISK' || state === 'EARLY_RISK') {
    return 'risk';
  }
  if (state === 'UP_PRESSURE' || state === 'DOWN_PRESSURE') {
    return 'warn';
  }
  if (state === 'BALANCED_LIVE') {
    return 'ok';
  }
  return 'neutral';
}

function formatShiftScoreBar(score: number | null): string {
  if (score === null) {
    return '[----------]';
  }
  const filled = Math.round(score * 10);
  return `[${'#'.repeat(filled)}${'-'.repeat(10 - filled)}]`;
}

export function manualExplainResponseToView(response: ManualExplainResponse): ManualExplainView {
  if (response.status === 'generated') {
    return {
      tone: 'ok',
      title: 'Gemini response',
      message: manualExplainSummaryText(response.summary),
      detail: `Manual explain generated. Cooldown ${response.cooldown_seconds ?? 'unknown'}s.`
    };
  }

  if (response.status === 'cooldown') {
    return {
      tone: 'warn',
      title: 'Gemini cooldown',
      message: `Manual explain is cooling down. Try again in ${
        response.retry_after_seconds ?? 'unknown'
      }s.`,
      detail: `Configured cooldown ${response.cooldown_seconds ?? 'unknown'}s.`
    };
  }

  if (response.status === 'rate_limited') {
    return {
      tone: 'warn',
      title: 'Gemini rate limit',
      message: 'Manual explain is rate limited by the hourly Gemini call budget.',
      detail:
        response.max_calls_per_hour === undefined
          ? 'Hourly cap is active.'
          : `Hourly cap ${response.max_calls_per_hour} calls.`
    };
  }

  if (response.status === 'disabled') {
    return {
      tone: 'warn',
      title: 'Gemini disabled',
      message: 'Gemini is disabled in the current service configuration.',
      detail: response.reason ?? 'Enable Gemini before requesting an explanation.'
    };
  }

  if (response.status === 'failed') {
    return {
      tone: 'error',
      title: 'Gemini request failed',
      message: response.error ?? response.reason ?? 'Gemini request failed.',
      detail: response.reason ?? 'Check service logs for the upstream error.'
    };
  }

  return {
    tone: 'info',
    title: 'Gemini status',
    message: response.reason ?? response.status,
    detail: response.generated_now ? 'Generated now.' : 'No new explanation was generated.'
  };
}

export function manualExplainErrorToView(error: unknown): ManualExplainView {
  const payload = (error as Error & { payload?: ManualExplainResponse }).payload;
  if (payload) {
    return manualExplainResponseToView(payload);
  }

  return {
    tone: 'error',
    title: 'Gemini request failed',
    message: error instanceof Error ? error.message : 'Manual explain request failed.',
    detail: 'The request did not return a structured response.'
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

function formatIndicatorValue(value: number, unit: string): string {
  const sign = value > 0 && unit !== '' ? '+' : '';
  const decimals = unit === '$/s' ? 2 : 3;
  const suffix = unit === '' ? '' : ` ${unit}`;
  return `${sign}${value.toFixed(decimals)}${suffix}`;
}

function formulaRows(snapshot: DashboardSnapshot): FormulaRow[] {
  const latestPoint = snapshot.price_points.at(-1);
  return [
    {
      key: 'p_mid',
      label: 'Up midpoint',
      value: latestPoint === undefined ? 'missing' : latestPoint.p_mid.toFixed(3)
    },
    formulaRowFromIndicator(snapshot, 'fair_gap', 'Fair gap'),
    formulaRowFromIndicator(snapshot, 'mid_velocity_1s', 'Mid velocity 1s'),
    formulaRowFromIndicator(snapshot, 'order_flow_1s', 'Order flow 1s'),
    formulaRowFromIndicator(snapshot, 'btc_velocity_1s', 'BTC velocity 1s'),
    formulaRowFromIndicator(snapshot, 'shift_score', 'Shift score')
  ];
}

function formulaRowFromIndicator(
  snapshot: DashboardSnapshot,
  key: string,
  label: string
): FormulaRow {
  const indicator = snapshot.regime_indicators.find((candidate) => candidate.key === key);
  return {
    key,
    label,
    value:
      indicator === undefined ? 'missing' : formatIndicatorValue(indicator.value, indicator.unit)
  };
}

function manualExplainSummaryText(summary: unknown): string {
  if (typeof summary === 'string' && summary.trim() !== '') {
    return summary;
  }

  if (summary && typeof summary === 'object' && 'summary' in summary) {
    const value = (summary as { summary?: unknown }).summary;
    if (typeof value === 'string' && value.trim() !== '') {
      return value;
    }
  }

  return 'Gemini generated an explanation, but the response text was empty.';
}

function formatHorizon(horizonMs: number): string {
  if (horizonMs % 1000 === 0) {
    return `${horizonMs / 1000}s`;
  }
  return `${horizonMs}ms`;
}
