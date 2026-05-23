<script lang="ts">
  import { onMount } from 'svelte';
  import {
    CrosshairMode,
    LineStyle,
    createChart,
    type IChartApi,
    type ISeriesApi
  } from 'lightweight-charts';
  import {
    dashboardRunStateForSnapshot,
    fallbackDashboardSnapshot,
    fetchDashboardSnapshot,
    getDashboardCopy,
    normalizeDashboardSnapshot,
    snapshotToDashboardView,
    type DashboardLanguage,
    type DashboardRunState,
    type DashboardSnapshot
  } from '$lib/dashboard';

  const horizons = ['1s', '5s', '30s'];
  const languages: DashboardLanguage[] = ['en', 'zh'];
  const proofItemsByLanguage = {
    en: [
      { label: 'Cloud Run hosted', detail: 'asia-northeast3', status: 'Seoul target' },
      { label: 'MongoDB Atlas', detail: '6 collections', status: 'seed and verify' },
      { label: 'MongoDB MCP', detail: 'read-only config', status: 'partner proof' },
      { label: 'Agent Builder', detail: 'OpenAPI tool', status: 'configured' },
      { label: 'Gemini', detail: '30m scheduled', status: 'latest response only' },
      { label: 'Replay validation', detail: 'deterministic', status: '1s / 5s / 30s' },
      { label: 'Live smoke', detail: 'verified via GCP', status: 'acceptance evidence' }
    ],
    zh: [
      { label: 'Cloud Run 托管', detail: 'asia-northeast3', status: '首尔目标区域' },
      { label: 'MongoDB Atlas', detail: '6 个集合', status: '已种子和校验' },
      { label: 'MongoDB MCP', detail: '只读配置', status: '协作证明' },
      { label: 'Agent Builder', detail: 'OpenAPI 工具', status: '已配置' },
      { label: 'Gemini', detail: '30 分钟定时', status: '只保留最新回复' },
      { label: '回放验证', detail: '确定性', status: '1s / 5s / 30s' },
      { label: '实时 smoke', detail: '已通过 GCP 验证', status: '验收证据' }
    ]
  };
  const zhForecastLabels: Record<string, string> = {
    'LIKELY SHIFT': '可能转向',
    WATCH: '观察',
    STABLE: '稳定',
    'NO SIGNAL': '无信号'
  };
  const zhStatusLabels: Record<string, string> = {
    high: '高',
    elevated: '升高',
    watch: '观察',
    normal: '正常'
  };
  const zhIndicatorLabels: Record<string, string> = {
    fair_gap: '公允差值',
    mid_velocity_1s: '中值速度 1秒',
    mid_velocity_5s: '中值速度 5秒',
    order_flow_1s: '订单流 1秒',
    btc_velocity_1s: 'BTC 速度 1秒',
    shift_score: '转向评分'
  };
  const runStateLabels: Record<DashboardLanguage, Record<DashboardRunState, string>> = {
    en: {
      'waiting-live-data': 'WAITING LIVE DATA',
      'demo-fallback': 'DEMO FALLBACK',
      loading: 'LOADING',
      snapshot: 'SNAPSHOT',
      streaming: 'STREAMING',
      'stream-error': 'STREAM ERROR'
    },
    zh: {
      'waiting-live-data': '等待实时数据',
      'demo-fallback': '演示备用数据',
      loading: '加载中',
      snapshot: '快照',
      streaming: '推送中',
      'stream-error': '推送错误'
    }
  };

  let chartElement: HTMLDivElement;
  let chart: IChartApi | undefined;
  let priceSeries: ISeriesApi<'Line'> | undefined;
  let fairSeries: ISeriesApi<'Line'> | undefined;
  let snapshot: DashboardSnapshot = fallbackDashboardSnapshot;
  let nowMs = Date.now();
  let dashboardView = snapshotToDashboardView(snapshot, nowMs);
  let horizon = '1s';
  let mode: 'live' | 'replay' = 'live';
  let language: DashboardLanguage = 'en';
  let copy = getDashboardCopy(language);
  let proofItems = proofItemsByLanguage[language];
  let geminiUserEnabled = dashboardView.geminiEnabled;
  let geminiEnabled = dashboardView.geminiEnabled;
  let runState: DashboardRunState = 'waiting-live-data';
  let statePulse = false;
  let statePulseTimer: ReturnType<typeof setTimeout> | undefined;
  let countdownTimer: ReturnType<typeof setInterval> | undefined;
  let summaryRefreshTimer: ReturnType<typeof setInterval> | undefined;

  $: copy = getDashboardCopy(language);
  $: proofItems = proofItemsByLanguage[language];

  function applySnapshot(nextSnapshot: DashboardSnapshot) {
    nowMs = Date.now();
    const nextDashboardView = snapshotToDashboardView(nextSnapshot, nowMs);
    if (
      dashboardView.state !== nextDashboardView.state ||
      dashboardView.shiftForecastLabel !== nextDashboardView.shiftForecastLabel
    ) {
      statePulse = true;
      if (statePulseTimer) {
        clearTimeout(statePulseTimer);
      }
      statePulseTimer = setTimeout(() => {
        statePulse = false;
      }, 1_300);
    }
    snapshot = nextSnapshot;
    dashboardView = nextDashboardView;
    geminiEnabled = geminiUserEnabled || dashboardView.geminiEnabled;
    priceSeries?.setData(dashboardView.priceData);
    fairSeries?.setData(dashboardView.fairData);
    priceSeries?.setMarkers(dashboardView.markers);
    chart?.timeScale().fitContent();
  }

  function refreshCountdown() {
    nowMs = Date.now();
    dashboardView = snapshotToDashboardView(snapshot, nowMs);
  }

  async function refreshDashboard() {
    runState = 'loading';
    try {
      const nextSnapshot = await fetchDashboardSnapshot(fetch, mode);
      applySnapshot(nextSnapshot);
      runState = dashboardRunStateForSnapshot(nextSnapshot, mode, false);
    } catch {
      runState = 'waiting-live-data';
    }
  }

  function setMode(nextMode: 'live' | 'replay') {
    mode = nextMode;
    void refreshDashboard();
  }

  function setLanguage(nextLanguage: DashboardLanguage) {
    language = nextLanguage;
  }

  function languageLabel(nextLanguage: DashboardLanguage) {
    return nextLanguage === 'en' ? copy.englishLanguage : copy.chineseLanguage;
  }

  function modeLabel(nextMode: 'live' | 'replay') {
    return nextMode === 'live' ? copy.liveMode : copy.replayMode;
  }

  function forecastLabel(label: string) {
    return language === 'zh' ? (zhForecastLabels[label] ?? label) : label;
  }

  function statusLabel(status: string) {
    return language === 'zh' ? (zhStatusLabels[status] ?? status) : status.toUpperCase();
  }

  function indicatorLabel(key: string, label: string) {
    return language === 'zh' ? (zhIndicatorLabels[key] ?? label) : label;
  }

  function runStateLabel(state: DashboardRunState) {
    return runStateLabels[language][state];
  }

  function setGeminiEnabled(event: Event) {
    geminiUserEnabled = (event.currentTarget as HTMLInputElement).checked;
    geminiEnabled = geminiUserEnabled || dashboardView.geminiEnabled;
  }

  function updatedUtcTime() {
    return new Date(snapshot.regime.updated_at_ms).toISOString().slice(11, 19);
  }

  onMount(() => {
    chart = createChart(chartElement, {
      width: chartElement.clientWidth,
      height: chartElement.clientHeight,
      layout: {
        background: { color: '#020405' },
        textColor: '#b7c4c9'
      },
      grid: {
        vertLines: { color: 'rgba(120,140,150,0.22)' },
        horzLines: { color: 'rgba(120,140,150,0.22)' }
      },
      rightPriceScale: {
        borderColor: 'rgba(120,140,150,0.35)'
      },
      timeScale: {
        borderColor: 'rgba(120,140,150,0.35)',
        timeVisible: true,
        secondsVisible: true
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: { color: 'rgba(0,229,255,0.55)' },
        horzLine: { color: 'rgba(0,229,255,0.35)' }
      }
    });

    priceSeries = chart.addLineSeries({
      color: '#00e5ff',
      lineWidth: 2,
      priceLineVisible: false
    });
    fairSeries = chart.addLineSeries({
      color: '#2dff55',
      lineWidth: 2,
      lineStyle: LineStyle.Dashed,
      priceLineVisible: false
    });

    applySnapshot(snapshot);

    const resizeObserver = new ResizeObserver(() => {
      chart?.resize(chartElement.clientWidth, chartElement.clientHeight);
      chart?.timeScale().fitContent();
    });
    resizeObserver.observe(chartElement);
    void refreshDashboard();
    countdownTimer = setInterval(refreshCountdown, 1_000);
    summaryRefreshTimer = setInterval(() => {
      void refreshDashboard();
    }, 60_000);

    const events = new EventSource('/api/dashboard/events');
    events.addEventListener('snapshot', (event) => {
      try {
        if (mode === 'live') {
          const nextSnapshot = normalizeDashboardSnapshot(JSON.parse((event as MessageEvent).data));
          applySnapshot(nextSnapshot);
          runState = dashboardRunStateForSnapshot(nextSnapshot, mode, true);
        }
      } catch {
        runState = 'stream-error';
      }
    });
    events.onerror = () => {
      if (runState !== 'streaming' && runState !== 'demo-fallback') {
        runState = 'waiting-live-data';
      }
    };

    return () => {
      resizeObserver.disconnect();
      events.close();
      if (statePulseTimer) {
        clearTimeout(statePulseTimer);
      }
      if (countdownTimer) {
        clearInterval(countdownTimer);
      }
      if (summaryRefreshTimer) {
        clearInterval(summaryRefreshTimer);
      }
      chart?.remove();
    };
  });

</script>

<svelte:head>
  <title>Regime Sentinel Agent</title>
</svelte:head>

<main class="tui-shell">
  <header class="tui-command">
    <div class="brand-block">
      <h1>REGIME SENTINEL AGENT</h1>
      <p>{dashboardView.marketTitle}</p>
    </div>
    <div class="market-strip">
      <span>{dashboardView.marketSeries}</span>
      <span class="truncate">{dashboardView.marketSlug}</span>
    </div>
    <div class="command-actions">
      <div class="language-switch" aria-label={copy.languageLabel}>
        {#each languages as item}
          <button
            class:active={language === item}
            type="button"
            on:click={() => setLanguage(item)}
            aria-pressed={language === item}
          >
            {languageLabel(item)}
          </button>
        {/each}
      </div>
      <div class="mode-switch" aria-label={copy.runModeLabel}>
        {#each ['live', 'replay'] as item}
          <button
            class:active={mode === item}
            type="button"
            on:click={() => setMode(item as 'live' | 'replay')}
            aria-pressed={mode === item}
          >
            [{modeLabel(item as 'live' | 'replay')}]
          </button>
        {/each}
      </div>
      <span class="stream-state" class:error={runState === 'stream-error'}>[{runStateLabel(runState)}]</span>
      <button class="tui-button" type="button" on:click={refreshDashboard}>{copy.refresh}</button>
    </div>
  </header>

  <div class="tui-content">
    <section class="hero-grid">
      <section
        class="tui-panel current-regime tone-panel"
        class:state-pulse={statePulse}
        class:risk={dashboardView.stateTone === 'risk'}
        class:warn={dashboardView.stateTone === 'warn'}
        class:ok={dashboardView.stateTone === 'ok'}
        class:neutral={dashboardView.stateTone === 'neutral'}
      >
        <div class="panel-title">[ {copy.currentRegimeTitle} ]</div>
        <div class="regime-headline">{dashboardView.displayRegimeLabel}</div>
        <div class="shift-watch">[{copy.shiftWatch}] {forecastLabel(dashboardView.shiftForecastLabel)}</div>
        <p class="regime-description">{dashboardView.regimeDescription}</p>
        <dl class="meta-grid">
          <div>
            <dt>{copy.confidence}</dt>
            <dd>{dashboardView.confidence}</dd>
          </div>
          <div>
            <dt>{copy.updatedUtc}</dt>
            <dd>{updatedUtcTime()}</dd>
          </div>
          <div>
            <dt>{copy.sourceState}</dt>
            <dd>{dashboardView.sourceRegimeLabel}</dd>
          </div>
          <div>
            <dt>{copy.marketSlug}</dt>
            <dd class="break-anywhere">{dashboardView.marketSlug}</dd>
          </div>
        </dl>
      </section>

      <section class="tui-panel forecast-panel">
        <div class="panel-title">[ {copy.forecastTitle} ]</div>
        <div
          class="forecast-label"
          class:risk={dashboardView.stateTone === 'risk'}
          class:warn={dashboardView.stateTone === 'warn'}
          class:ok={dashboardView.stateTone === 'ok'}
          class:neutral={dashboardView.stateTone === 'neutral'}
        >
          {forecastLabel(dashboardView.shiftForecastLabel)}
        </div>
        <dl class="forecast-metrics">
          <div>
            <dt>{copy.shiftScore}</dt>
            <dd>{dashboardView.shiftScoreBar}</dd>
          </div>
          <div>
            <dt>{copy.scoreValue}</dt>
            <dd>
              {dashboardView.shiftScoreClamped === null
                ? copy.noSignal
                : dashboardView.shiftScoreClamped.toFixed(2)}
              / 0.75 {copy.threshold}
            </dd>
          </div>
          <div>
            <dt>{copy.replayLeadEvidence}</dt>
            <dd>{dashboardView.currentLead}</dd>
          </div>
        </dl>
        <div class="horizon-control">
          <p>{copy.evidenceHorizon}</p>
          <div>
            {#each horizons as item}
              <button
                class:active={horizon === item}
                type="button"
                on:click={() => (horizon = item)}
                aria-pressed={horizon === item}
              >
                [{item}]
              </button>
            {/each}
          </div>
        </div>
        <div class="forecast-caveat">
          <p>{copy.liveModeCaveat}</p>
          <p>{copy.replayValidationCaveat}</p>
        </div>
      </section>

      <section class="tui-panel evidence-panel">
        <div class="panel-title">[ {copy.evidenceSignalsTitle} ]</div>
        <div class="table-scroll">
          <table class="tui-table">
            <thead>
              <tr>
                <th>{copy.signal}</th>
                <th>{copy.value}</th>
                <th>{copy.status}</th>
              </tr>
            </thead>
            <tbody>
              {#each dashboardView.indicatorRows as row}
                <tr>
                  <td>{indicatorLabel(row.key, row.label)}</td>
                  <td>{row.value}</td>
                  <td>
                    <span
                      class="status-badge"
                      class:risk={row.status === 'high'}
                      class:warn={row.status === 'elevated' || row.status === 'watch'}
                      class:ok={row.status !== 'high' && row.status !== 'elevated' && row.status !== 'watch'}
                    >
                      {statusLabel(row.status)}
                    </span>
                  </td>
                </tr>
              {:else}
                <tr>
                  <td colspan="3">{copy.noIndicators}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <div class="formula-strip">
          <p>{dashboardView.stateFormula}</p>
          {#each dashboardView.stateRules as rule}
            <span>{rule}</span>
          {/each}
        </div>
      </section>
    </section>

    <section class="tui-panel market-panel">
        <div class="panel-row">
        <div>
          <div class="panel-title">[ {copy.marketWindowTitle} ]</div>
          <p>{copy.marketWindowDescription}</p>
        </div>
        <div class="chart-legend">
          <span class="cyan">p_mid</span>
          <span class="green">p_fair</span>
          <span class="red">{copy.alertsTitle.toLowerCase()}</span>
        </div>
      </div>
      <div bind:this={chartElement} class="chart-surface"></div>
    </section>

    <section class="lower-grid">
      <section class="tui-panel">
        <div class="panel-title">[ {copy.alertsTitle} ]</div>
        <div class="table-scroll">
          <table class="tui-table">
            <thead>
              <tr>
                <th>{copy.state}</th>
                <th>{copy.relTime}</th>
                <th>{copy.score}</th>
                <th>{copy.lead}</th>
              </tr>
            </thead>
            <tbody>
              {#each dashboardView.alertRows as row}
                <tr>
                  <td>{row.state}</td>
                  <td>{row.time}</td>
                  <td>{row.score}</td>
                  <td class="risk-text">{row.lead}</td>
                </tr>
              {:else}
                <tr>
                  <td colspan="4">{copy.noAlerts}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>

      <section class="tui-panel">
        <div class="panel-title">[ {copy.mongoMemoryTitle} ]</div>
        <div class="table-scroll">
          <table class="tui-table">
            <thead>
              <tr>
                <th>{copy.slug}</th>
                <th>{copy.time}</th>
                <th>{copy.score}</th>
                <th>{copy.gap}</th>
                <th>{copy.flow}</th>
                <th>{copy.depth}</th>
              </tr>
            </thead>
            <tbody>
              {#each dashboardView.similarRows as row}
                <tr>
                  <td class="truncate-cell">{row.slug}</td>
                  <td>{row.time}</td>
                  <td>{row.score}</td>
                  <td>{row.fairGap}</td>
                  <td>{row.orderFlow}</td>
                  <td>{row.depth}</td>
                </tr>
              {:else}
                <tr>
                  <td colspan="6">{copy.noSimilarWindows}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>

      <section class="tui-panel gemini-panel">
        <div class="panel-title">[ {copy.geminiTitle} ]</div>
        <div class="gemini-control">
          <label>
            <input
              checked={geminiEnabled}
              type="checkbox"
              on:change={setGeminiEnabled}
            />
            {copy.geminiAvailable}
          </label>
          <div class="countdown-pill">
            <span>{copy.nextGeminiUpdate}</span>
            <strong>{dashboardView.geminiCountdown}</strong>
          </div>
        </div>
        <p class="small-copy">{dashboardView.geminiCoverage} / {copy.generated} {dashboardView.geminiGeneratedAt}</p>
        <p>{dashboardView.geminiSummary}</p>
      </section>

      <section class="tui-panel validation-panel">
        <div class="panel-title">[ {copy.validationTitle} ]</div>
        <p>{dashboardView.validationSummary}</p>
        {#if dashboardView.degradedConfidence}
          <p class="warn-copy">{dashboardView.validationReason}</p>
        {/if}
        <div class="table-scroll">
          <table class="tui-table">
            <thead>
              <tr>
                <th>{copy.horizon}</th>
                <th>PR-AUC</th>
                <th>{copy.source}</th>
              </tr>
            </thead>
            <tbody>
              {#each dashboardView.validationRows as row}
                <tr class:active-row={horizon === row.horizon}>
                  <td>{row.horizon}</td>
                  <td>{row.prAuc}</td>
                  <td>{copy.deterministicReplay}</td>
                </tr>
              {:else}
                <tr>
                  <td colspan="3">{copy.noReplayRows}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>
    </section>

    <section class="tui-panel system-proof">
      <div class="panel-title">[ {copy.systemProofTitle} ]</div>
      <div class="proof-grid">
        {#each proofItems as item}
          <div>
            <strong>{item.label}</strong>
            <span>{item.detail}</span>
            <small>{item.status}</small>
          </div>
        {/each}
      </div>
    </section>
  </div>
</main>
