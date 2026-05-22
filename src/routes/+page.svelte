<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Activity,
    AlertTriangle,
    BarChart3,
    Database,
    Play,
    RefreshCw,
    Sparkles
  } from '@lucide/svelte';
  import {
    CrosshairMode,
    LineStyle,
    createChart,
    type IChartApi,
    type UTCTimestamp
  } from 'lightweight-charts';

  type AlertRow = {
    time: string;
    state: string;
    lead: string;
    score: string;
  };

  const priceData = [
    { time: 1769000000 as UTCTimestamp, value: 0.5 },
    { time: 1769000075 as UTCTimestamp, value: 0.54 },
    { time: 1769000100 as UTCTimestamp, value: 0.62 },
    { time: 1769000400 as UTCTimestamp, value: 0.61 }
  ];

  const fairData = [
    { time: 1769000000 as UTCTimestamp, value: 0.49 },
    { time: 1769000075 as UTCTimestamp, value: 0.49 },
    { time: 1769000100 as UTCTimestamp, value: 0.51 },
    { time: 1769000400 as UTCTimestamp, value: 0.52 }
  ];

  const alertRows: AlertRow[] = [
    { time: '00:01.750', state: 'EARLY_RISK', lead: '+250 ms', score: '1.94' },
    { time: '00:05.000', state: 'WATCH', lead: 'pending', score: '0.68' }
  ];

  let chartElement: HTMLDivElement;
  let chart: IChartApi | undefined;
  let horizon = '1s';
  let geminiEnabled = false;
  let runState = 'ready';

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

    const priceSeries = chart.addLineSeries({
      color: '#1d4ed8',
      lineWidth: 2,
      priceLineVisible: false
    });
    const fairSeries = chart.addLineSeries({
      color: '#0f766e',
      lineWidth: 2,
      lineStyle: LineStyle.Dashed,
      priceLineVisible: false
    });

    priceSeries.setData(priceData);
    fairSeries.setData(fairData);
    priceSeries.setMarkers([
      {
        time: 1769000075 as UTCTimestamp,
        position: 'aboveBar',
        color: '#b91c1c',
        shape: 'arrowDown',
        text: 'EARLY_RISK +250ms'
      }
    ]);
    chart.timeScale().fitContent();

    const resize = () => chart?.applyOptions({ width: chartElement.clientWidth });
    resize();
    window.addEventListener('resize', resize);

    return () => {
      window.removeEventListener('resize', resize);
      chart?.remove();
    };
  });

  function replay() {
    runState = 'replayed';
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
          <p class="mt-2 text-xl font-semibold text-red-700">EARLY_RISK</p>
        </div>
        <div class="rounded-md border border-slate-200 bg-white p-4">
          <div class="flex items-center justify-between text-xs uppercase text-slate-500">
            <span>Lead Time</span>
            <BarChart3 size={16} class="text-blue-700" />
          </div>
          <p class="mt-2 text-xl font-semibold text-slate-950">+250 ms</p>
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
        </div>
      </div>

      <div class="rounded-md border border-slate-200 bg-white">
        <div class="flex flex-col gap-3 border-b border-slate-200 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h2 class="text-sm font-semibold text-slate-950">Replay Window</h2>
            <p class="text-xs text-slate-500">mid price, fair probability, and generated alert marker</p>
          </div>
          <div class="flex items-center gap-2">
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
          {#each alertRows as row}
            <div class="grid grid-cols-[1fr_auto] gap-3 px-4 py-3">
              <div>
                <p class="text-sm font-medium text-slate-950">{row.state}</p>
                <p class="text-xs text-slate-500">{row.time} · score {row.score}</p>
              </div>
              <span class="self-center rounded-md bg-red-50 px-2 py-1 text-xs font-semibold text-red-700">
                {row.lead}
              </span>
            </div>
          {/each}
        </div>
      </section>

      <section class="rounded-md border border-slate-200 bg-white p-4">
        <div class="flex items-center justify-between gap-3">
          <div>
            <h2 class="text-sm font-semibold text-slate-950">Gemini Summary</h2>
            <p class="text-xs text-slate-500">interval floor 15 minutes</p>
          </div>
          <label class="inline-flex items-center gap-2 text-sm text-slate-700">
            <input bind:checked={geminiEnabled} class="h-4 w-4" type="checkbox" />
            Enabled
          </label>
        </div>
        <p class="mt-4 text-sm leading-6 text-slate-700">
          Fair gap and OFI rose before the full price move. Historical windows with similar pressure
          usually resolved within the next 1s to 5s horizon.
        </p>
      </section>

      <section class="rounded-md border border-slate-200 bg-white p-4">
        <h2 class="text-sm font-semibold text-slate-950">Run State</h2>
        <dl class="mt-3 grid grid-cols-2 gap-3 text-sm">
          <div>
            <dt class="text-xs text-slate-500">Mode</dt>
            <dd class="font-medium text-slate-950">{runState}</dd>
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
            <dd class="font-medium text-slate-950">3</dd>
          </div>
        </dl>
      </section>
    </aside>
  </div>
</main>
