<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Chart,
    LineController,
    LineElement,
    PointElement,
    CategoryScale,
    LinearScale,
    Tooltip,
    Filler,
  } from 'chart.js';

  Chart.register(LineController, LineElement, PointElement, CategoryScale, LinearScale, Tooltip, Filler);

  interface Props {
    labels: string[];
    data: number[];
    label?: string;
    color?: string;
  }

  let { labels, data, label = '', color = '#C9718A' }: Props = $props();

  let canvas: HTMLCanvasElement;
  let chart: Chart | undefined;

  function render() {
    if (chart) chart.destroy();
    if (!canvas) return;

    chart = new Chart(canvas, {
      type: 'line',
      data: {
        labels,
        datasets: [
          {
            label,
            data,
            borderColor: color,
            backgroundColor: color + '20',
            borderWidth: 2,
            pointRadius: 3,
            pointBackgroundColor: color,
            fill: true,
            tension: 0.3,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: { tooltip: { mode: 'index', intersect: false } },
        scales: {
          x: { grid: { display: false } },
          y: { beginAtZero: true },
        },
      },
    });
  }

  onMount(() => {
    render();
    return () => chart?.destroy();
  });

  $effect(() => {
    // Re-render when data changes
    void labels;
    void data;
    render();
  });
</script>

<div class="chart-container">
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .chart-container {
    position: relative;
    width: 100%;
    height: 240px;
  }
</style>
