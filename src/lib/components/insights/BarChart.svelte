<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Chart,
    BarController,
    BarElement,
    CategoryScale,
    LinearScale,
    Tooltip,
  } from 'chart.js';

  Chart.register(BarController, BarElement, CategoryScale, LinearScale, Tooltip);

  interface Props {
    labels: string[];
    data: number[];
    label?: string;
    color?: string;
    horizontal?: boolean;
  }

  let { labels, data, label = '', color = '#C9718A', horizontal = false }: Props = $props();

  let canvas: HTMLCanvasElement;
  let chart: Chart | undefined;

  function render() {
    if (chart) chart.destroy();
    if (!canvas) return;

    chart = new Chart(canvas, {
      type: 'bar',
      data: {
        labels,
        datasets: [
          {
            label,
            data,
            backgroundColor: color + 'CC',
            borderColor: color,
            borderWidth: 1,
            borderRadius: 4,
          },
        ],
      },
      options: {
        indexAxis: horizontal ? 'y' : 'x',
        responsive: true,
        maintainAspectRatio: false,
        plugins: { tooltip: { mode: 'index', intersect: false } },
        scales: {
          x: { grid: { display: horizontal } },
          y: { beginAtZero: true, grid: { display: !horizontal } },
        },
      },
    });
  }

  onMount(() => {
    render();
    return () => chart?.destroy();
  });

  $effect(() => {
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
