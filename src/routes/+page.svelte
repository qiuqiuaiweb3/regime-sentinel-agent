<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Activity,
    AlertTriangle,
    BarChart3,
    Database,
    Play,
    RefreshCw,
    Sparkles,
    ToggleLeft
  } from '@lucide/svelte';
  import {
    CrosshairMode,
    LineStyle,
    createChart,
    type IChartApi,
    type ISeriesApi
  } from 'lightweight-charts';
  import {
    fallbackDashboardSnapshot,
    fetchDashboardSnapshot,
    normalizeDashboardSnapshot,
    snapshotToDashboardView,
    type DashboardSnapshot
  } from '$lib/dashboard';

  let chartElement: HTMLDivElement;
  let chart: IChartApi | undefined;
  let priceSeries: ISeriesApi<'Line'> | undefined;
  let fairSeries: ISeriesApi<'Line'> | undefined;
  let snapshot: DashboardSnapshot = fallbackDashboardSnapshot;
  let dashboardView = snapshotToDashboardView(snapshot);
  let horizon = '1s';
  let mode: 'live' | 'replay' = 'live';
  let geminiEnabled = dashboardView.geminiEnabled;
  let runState = 'fallback';

  function applySnapshot(nextSnapshot: DashboardSnapshot) {
    snapshot = nextSnapshot;
    dashboardView = snapshotToDashboardView(snapshot);
    geminiEnabled = dashboardView.geminiEnabled;
    priceSeries?.setData(dashboardView.priceData);
    fairSeries?.setData(dashboardView.fairData);
    priceSeries?.setMarkers(dashboardView.markers);
    chart?.timeScale().fitContent();
  }

  async function refreshDashboard() {
    runState = 'loading';
    try {
      applySnapshot(await fetchDashboardSnapshot(fetch, mode));
      runState = 'snapshot';
    } catch {
      runState = 'fallback';
    }
  }

  function setMode(nextMode: 'live' | 'replay') {
    mode = nextMode;
    runState = nextMode;
    void refreshDashboard();
  }

  onMount(() => {
    chart = createChart(chartElement, {
      layout: {
        background: { color: '#ffffff' },
        textColor: '#26364f'
      },
      grid: {
        vertLines: { color: '#e4e9f2' },
        horzLines: { color: '#e4e9f2' }
      },
      rightPriceScale: {
        borderColor: '#cbd5e1'
      },
      timeScale: {
        borderColor: '#cbd5e1',
        timeVisible: true,
        secondsVisible: true
      },
      crosshair: {
        mode: CrosshairMode.Normal
      }
    });

    priceSeries = chart.addLineSeries({
      color: '#1d4ed8',
      lineWidth: 2,
      priceLineVisible: false
    });
    fairSeries = chart.addLineSeries({
      color: '#0f766e',
      lineWidth: 2,
      lineStyle: LineStyle.Dashed,
      priceLineVisible: false
    });

    applySnapshot(snapshot);

    const resize = () => chart?.applyOptions({ width: chartElement.clientWidth });
    resize();
    window.addEventListener('resize', resize);
    void refreshDashboard();

    const events = new EventSource('/api/dashboard/events');
    events.addEventListener('snapshot', (event) => {
      try {
        if (mode === 'live') {
          applySnapshot(normalizeDashboardSnapshot(JSON.parse((event as MessageEvent).data)));
          runState = 'streaming';
        }
      } catch {
        runState = 'stream-error';
      }
    });
    events.onerror = () => {
      if (runState !== 'streaming') {
        runState = 'fallback';
      }
    };

    return () => {
      window.removeEventListener('resize', resize);
      events.close();
      chart?.remove();
    };
  });

  function replay() {
    mode = 'replay';
    runState = 'replayed';
    void refreshDashboard();
  }
</script>

<svelte:head>
  <title>Regime Sentinel Agent</title>
</svelte:head>

<main class="min-h-screen bg-[#f5f7fb] text-[#172033]">
  <header class="border-b border-slate-200 bg-white">
    <div class="mx-auto flex max-w-7xl items-center justify-between px-4 py-3 sm:px-6">
      <div class="flex items-center gap-3">
        <div class="flex h-9 w-9 items-center justify-center rounded-md bg-[#16324f] text-white">
          <Activity size={20} />
        </div>
        <div>
          <h1 class="text-base font-semibold tracking-normal text-slate-950">Regime Sentinel Agent</h1>
          <p class="text-xs text-slate-500">BTC 5m Up/Down market monitor</p>
        </div>
      </div>
      <div class="flex items-center gap-2">
        <button
          class="inline-flex h-9 items-center gap-2 rounded-md border border-slate-300 bg-white px-3 text-sm font-medium text-slate-700 hover:bg-slate-50"
          type="button"
          on:click={refreshDashboard}
          aria-label="Refresh data"
        >
          <RefreshCw size={16} />
          Refresh
        </button>
        <button
          class="inline-flex h-9 items-center gap-2 rounded-md bg-[#0f766e] px-3 text-sm font-medium text-white hover:bg-[#0b5f59]"
          type="button"
          on:click={replay}
          aria-label="Run replay"
        >
          <Play size={16} />
          Replay
        </button>
      </div>
    </div>
  </header>

  <div class="mx-auto grid max-w-7xl gap-4 px-4 py-4 sm:px-6 lg:grid-cols-[minmax(0,1fr)_340px]">
    <section class="space-y-4">
      <div class="grid gap-3 sm:grid-cols-4">
        <div class="rounded-md border border-slate-200 bg-white p-4">
          <div class="flex items-center justify-between text-xs uppercase text-slate-500">
            <span>State</span>
            <AlertTriangle size={16} class="text-red-700" />
          </div>
          <p class="mt-2 text-xl font-semibold text-red-700">{dashboardView.state}</p>
          <p class="mt-1 text-xs text-slate-500">{dashboardView.confidence}</p>
        </div>
        <div class="rounded-md border border-slate-200 bg-white p-4">
          <div class="flex items-center justify-between text-xs uppercase text-slate-500">
            <span>Lead Time</span>
            <BarChart3 size={16} class="text-blue-700" />
          </div>
          <p class="mt-2 text-xl font-semibold text-slate-950">{dashboardView.currentLead}</p>
        </div>
        <div class="rounded-md border border-slate-200 bg-white p-4">
          <div class="flex items-center justify-between text-xs uppercase text-slate-500">
            <span>MongoDB</span>
            <Database size={16} class="text-emerald-700" />
          </div>
          <p class="mt-2 text-xl font-semibold text-slate-950">6 collections</p>
        </div>
        <div class="rounded-md border border-slate-200 bg-white p-4">
          <div class="flex items-center justify-between text-xs uppercase text-slate-500">
            <span>Gemini</span>
            <Sparkles size={16} class="text-violet-700" />
          </div>
          <p class="mt-2 text-xl font-semibold text-slate-950">{geminiEnabled ? '15 min' : 'Off'}</p>
          <p class="mt-1 text-xs text-slate-500">{dashboardView.geminiGeneratedAt}</p>
        </div>
      </div>

      <div class="rounded-md border border-slate-200 bg-white">
        <div class="flex flex-col gap-3 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 class="text-sm font-semibold text-slate-950">Market Window</h2>
            <p class="text-xs text-slate-500">mid price, fair probability, and generated alert marker</p>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <div class="inline-flex h-8 rounded-md border border-slate-300 bg-white p-0.5" aria-label="Run mode">
              {#each ['live', 'replay'] as item}
                <button
                  class={`inline-flex h-7 items-center gap-1 rounded px-2 text-sm ${
                    mode === item
                      ? 'bg-[#16324f] text-white'
                      : 'text-slate-700 hover:bg-slate-50'
                  }`}
                  type="button"
                  on:click={() => setMode(item as 'live' | 'replay')}
                  aria-label={`${item} mode`}
                >
                  <ToggleLeft size={14} />
                  {item}
                </button>
              {/each}
            </div>
            {#each ['1s', '5s', '30s'] as item}
              <button
                class={`h-8 rounded-md border px-3 text-sm ${
                  horizon === item
                    ? 'border-[#16324f] bg-[#16324f] text-white'
                    : 'border-slate-300 bg-white text-slate-700 hover:bg-slate-50'
                }`}
                type="button"
                on:click={() => (horizon = item)}
              >
                {item}
              </button>
            {/each}
          </div>
        </div>
        <div bind:this={chartElement} class="chart-surface w-full"></div>
      </div>
    </section>

    <aside class="space-y-4">
      <section class="rounded-md border border-slate-200 bg-white">
        <div class="border-b border-slate-200 px-4 py-3">
          <h2 class="text-sm font-semibold text-slate-950">Alerts</h2>
        </div>
        <div class="divide-y divide-slate-200">
          {#each dashboardView.alertRows as row}
            <div class="grid grid-cols-[1fr_auto] gap-3 px-4 py-3">
              <div>
                <p class="text-sm font-medium text-slate-950">{row.state}</p>
                <p class="text-xs text-slate-500">{row.time} · score {row.score}</p>
              </div>
              <span class="self-center rounded-md bg-red-50 px-2 py-1 text-xs font-semibold text-red-700">
                {row.lead}
              </span>
            </div>
          {:else}
            <p class="px-4 py-3 text-sm text-slate-500">No alerts</p>
          {/each}
        </div>
      </section>

      <section class="rounded-md border border-slate-200 bg-white">
        <div class="border-b border-slate-200 px-4 py-3">
          <h2 class="text-sm font-semibold text-slate-950">Similar History</h2>
        </div>
        <div class="divide-y divide-slate-200">
          {#each dashboardView.similarRows as row}
            <div class="px-4 py-3">
              <div class="flex items-center justify-between gap-3">
                <p class="min-w-0 truncate text-sm font-medium text-slate-950">{row.slug}</p>
                <span class="rounded-md bg-emerald-50 px-2 py-1 text-xs font-semibold text-emerald-700">
                  {row.score}
                </span>
              </div>
              <p class="mt-1 break-words text-xs text-slate-500">
                {row.time}
                <span class="mx-1">·</span>
                gap {row.fairGap}
                <span class="mx-1">·</span>
                flow {row.orderFlow}
                <span class="mx-1">·</span>
                depth {row.depth}
              </p>
            </div>
          {:else}
            <p class="px-4 py-3 text-sm text-slate-500">No similar windows</p>
          {/each}
        </div>
      </section>

      <section class="rounded-md border border-slate-200 bg-white p-4">
        <div class="flex items-center justify-between gap-3">
          <div>
            <h2 class="text-sm font-semibold text-slate-950">Gemini Summary</h2>
            <p class="text-xs text-slate-500">{dashboardView.geminiCoverage}</p>
          </div>
          <label class="inline-flex items-center gap-2 text-sm text-slate-700">
            <input bind:checked={geminiEnabled} class="h-4 w-4" type="checkbox" />
            Enabled
          </label>
        </div>
        <p class="mt-4 text-sm leading-6 text-slate-700">
          {dashboardView.geminiSummary}
        </p>
        <p class="mt-3 text-xs text-slate-500">Generated {dashboardView.geminiGeneratedAt}</p>
      </section>

      <section class="rounded-md border border-slate-200 bg-white p-4">
        <h2 class="text-sm font-semibold text-slate-950">Validation</h2>
        <p class="mt-2 text-sm text-slate-700">{dashboardView.validationSummary}</p>
        {#if dashboardView.degradedConfidence}
          <p class="mt-2 rounded-md bg-amber-50 px-3 py-2 text-xs font-medium text-amber-800">
            {dashboardView.validationReason}
          </p>
        {/if}
        <div class="mt-3 grid grid-cols-3 gap-2">
          {#each dashboardView.validationRows as row}
            <div class="rounded-md border border-slate-200 px-3 py-2">
              <p class="text-xs text-slate-500">{row.horizon}</p>
              <p class="text-sm font-semibold text-slate-950">{row.prAuc}</p>
            </div>
          {/each}
        </div>
      </section>

      <section class="rounded-md border border-slate-200 bg-white p-4">
        <h2 class="text-sm font-semibold text-slate-950">Run State</h2>
        <dl class="mt-3 grid grid-cols-2 gap-3 text-sm">
          <div>
            <dt class="text-xs text-slate-500">Mode</dt>
            <dd class="font-medium text-slate-950">{snapshot.mode} · {runState}</dd>
          </div>
          <div>
            <dt class="text-xs text-slate-500">Horizon</dt>
            <dd class="font-medium text-slate-950">{horizon}</dd>
          </div>
          <div>
            <dt class="text-xs text-slate-500">False Alerts</dt>
            <dd class="font-medium text-slate-950">0</dd>
          </div>
          <div>
            <dt class="text-xs text-slate-500">Vector k</dt>
            <dd class="font-medium text-slate-950">{dashboardView.similarRows.length}</dd>
          </div>
        </dl>
      </section>
    </aside>
  </div>
</main>
