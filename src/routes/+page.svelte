<script lang="ts">
  import { onMount } from 'svelte';
  import uPlot from 'uplot';
  import 'uplot/dist/uPlot.min.css';
  import {
    LIVE_DASHBOARD_REFRESH_MS,
    dashboardChartDataFromView,
    dashboardRunStateForSnapshot,
    fallbackDashboardSnapshot,
    fetchDashboardSnapshot,
    getDashboardCopy,
    normalizeDashboardSnapshot,
    snapshotToDashboardView,
    type DashboardLanguage,
    type DashboardRunState,
    type DashboardView,
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
    waiting: '等待',
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
  let chart: uPlot | undefined;
  let snapshot: DashboardSnapshot = fallbackDashboardSnapshot;
  let nowMs = Date.now();
  let dashboardView = snapshotToDashboardView(snapshot, nowMs);
  let updatedUtc = formatUtcTime(snapshot.regime.updated_at_ms);
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
  let dashboardRefreshTimer: ReturnType<typeof setInterval> | undefined;

  $: copy = getDashboardCopy(language);
  $: proofItems = proofItemsByLanguage[language];
  $: updatedUtc = formatUtcTime(snapshot.regime.updated_at_ms);

  function applySnapshot(nextSnapshot: DashboardSnapshot) {
    if (
      nextSnapshot.mode === snapshot.mode &&
      nextSnapshot.regime.updated_at_ms < snapshot.regime.updated_at_ms
    ) {
      return false;
    }
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
    updateChartData();
    return true;
  }

  function refreshCountdown() {
    nowMs = Date.now();
    dashboardView = snapshotToDashboardView(snapshot, nowMs);
  }

  async function refreshDashboard(showLoading = true) {
    if (showLoading) {
      runState = 'loading';
    }
    try {
      const nextSnapshot = await fetchDashboardSnapshot(fetch, mode);
      const applied = applySnapshot(nextSnapshot);
      if (applied && (showLoading || runState !== 'streaming')) {
        runState = dashboardRunStateForSnapshot(nextSnapshot, mode, false);
      }
    } catch {
      if (showLoading) {
        runState = 'waiting-live-data';
      }
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

  function formatUtcTime(timestampMs: number) {
    return new Date(timestampMs).toISOString().slice(11, 19);
  }

  function chartDataFromView(view: DashboardView): uPlot.AlignedData {
    return dashboardChartDataFromView(view) as uPlot.AlignedData;
  }

  function updateChartData() {
    chart?.setData(chartDataFromView(dashboardView));
    applyChartRange();
  }

  function applyChartRange() {
    if (!chart) {
      return;
    }

    if (dashboardView.marketWindow) {
      chart.setScale('x', {
        min: dashboardView.marketWindow.startTime,
        max: dashboardView.marketWindow.endTime
      });
      return;
    }

    if (dashboardView.priceData.length === 0) {
      const currentTime = Date.now() / 1000;
      chart.setScale('x', {
        min: currentTime - 150,
        max: currentTime + 150
      });
      return;
    }

    if (dashboardView.priceData.length !== 1) {
      return;
    }

    const pointTime = dashboardView.priceData[0].time;
    chart.setScale('x', {
      min: pointTime - 150,
      max: pointTime + 150
    });
  }

  function chartSize() {
    return {
      width: chartElement.clientWidth || 800,
      height: chartElement.clientHeight || 220
    };
  }

  function createChartOptions(): uPlot.Options {
    const size = chartSize();
    return {
      ...size,
      padding: [12, 10, 0, 38],
      legend: {
        show: false
      },
      cursor: {
        x: true,
        y: true,
        drag: {
          x: false,
          y: false
        }
      },
      scales: {
        x: {
          time: true,
          range: (_self, min, max) => {
            if (min === max) {
              return [min - 150, max + 150];
            }
            return [min, max];
          }
        },
        y: {
          range: [0, 1]
        }
      },
      axes: [
        {
          stroke: '#7e9098',
          grid: {
            stroke: 'rgba(120,140,150,0.22)',
            width: 1
          },
          ticks: {
            stroke: 'rgba(120,140,150,0.35)'
          },
          values: (_self, values) =>
            values.map((value) => new Date(value * 1000).toISOString().slice(11, 19))
        },
        {
          side: 1,
          stroke: '#7e9098',
          grid: {
            stroke: 'rgba(120,140,150,0.22)',
            width: 1
          },
          ticks: {
            stroke: 'rgba(120,140,150,0.35)'
          },
          values: (_self, values) => values.map((value) => value.toFixed(2))
        }
      ],
      series: [
        {},
        {
          label: 'p_mid',
          stroke: '#00e5ff',
          width: 2,
          points: {
            show: true,
            size: 5,
            stroke: '#00e5ff',
            fill: '#020405'
          }
        },
        {
          label: 'p_fair',
          stroke: '#2dff55',
          width: 2,
          dash: [8, 6],
          points: {
            show: false
          }
        }
      ],
      plugins: [alertMarkersPlugin()]
    };
  }

  function alertMarkersPlugin(): uPlot.Plugin {
    return {
      hooks: {
        draw: [
          (plot) => {
            if (dashboardView.markers.length === 0) {
              return;
            }

            const { ctx, bbox } = plot;
            const leftBound = bbox.left;
            const rightBound = bbox.left + bbox.width;
            ctx.save();
            ctx.strokeStyle = 'rgba(255,48,48,0.72)';
            ctx.fillStyle = '#ff3030';
            ctx.font = '12px ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace';
            ctx.textBaseline = 'top';
            for (const marker of dashboardView.markers) {
              const x = plot.valToPos(marker.time, 'x', true);
              if (x < leftBound || x > rightBound) {
                continue;
              }
              ctx.beginPath();
              ctx.moveTo(x, bbox.top);
              ctx.lineTo(x, bbox.top + bbox.height);
              ctx.stroke();
              ctx.fillText(marker.text, Math.min(x + 5, rightBound - 120), bbox.top + 6);
            }
            ctx.restore();
          }
        ]
      }
    };
  }

  onMount(() => {
    chart = new uPlot(createChartOptions(), chartDataFromView(dashboardView), chartElement);
    applyChartRange();

    const resizeObserver = new ResizeObserver(() => {
      chart?.setSize(chartSize());
    });
    resizeObserver.observe(chartElement);
    void refreshDashboard();
    countdownTimer = setInterval(refreshCountdown, 1_000);
    dashboardRefreshTimer = setInterval(() => {
      if (runState !== 'streaming') {
        void refreshDashboard(false);
      }
    }, LIVE_DASHBOARD_REFRESH_MS);

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
      if (dashboardRefreshTimer) {
        clearInterval(dashboardRefreshTimer);
      }
      chart?.destroy();
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
    </div>
    <div class="market-strip">
      <span class="break-anywhere">{dashboardView.headerMarketLabel}</span>
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
      <button class="tui-button" type="button" on:click={() => refreshDashboard(true)}>{copy.refresh}</button>
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
            <dd>{updatedUtc}</dd>
          </div>
          <div>
            <dt>{copy.sourceState}</dt>
            <dd class="break-anywhere">{dashboardView.sourceRegimeLabel}</dd>
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
                      class:waiting={row.status === 'waiting'}
                      class:ok={row.status !== 'high' &&
                        row.status !== 'elevated' &&
                        row.status !== 'watch' &&
                        row.status !== 'waiting'}
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
          <span class="green">{dashboardView.fairLineLabel}</span>
          <span class="red">{copy.alertsTitle.toLowerCase()}</span>
        </div>
      </div>
      <div class="chart-shell">
        <div bind:this={chartElement} class="chart-surface"></div>
        {#if !dashboardView.hasMidpointData}
          <div class="chart-empty-state">
            <strong>{dashboardView.marketWindowStatusTitle}</strong>
            <span>{dashboardView.marketWindowStatusDetail}</span>
          </div>
        {/if}
      </div>
      <div class:waiting={!dashboardView.hasMidpointData} class="chart-sample-status">
        <strong>{dashboardView.marketWindowStatusTitle}</strong>
        <span>{dashboardView.marketWindowStatusDetail}</span>
      </div>
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
