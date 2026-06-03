<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Chart,
    DoughnutController,
    ArcElement,
    Tooltip,
    Legend,
  } from 'chart.js';

  Chart.register(DoughnutController, ArcElement, Tooltip, Legend);

  interface Props {
    labels: string[];
    data: number[];
    colors?: string[];
  }

  const defaultColors = ['#C9718A', '#9E4E66', '#F5DCE4', '#7A6670', '#2C1F24'];

  let { labels, data, colors = defaultColors }: Props = $props();

  let canvas: HTMLCanvasElement;
  let chart: Chart | undefined;

  function render() {
    if (chart) chart.destroy();
    if (!canvas) return;

    chart = new Chart(canvas, {
      type: 'doughnut',
      data: {
        labels,
        datasets: [
          {
            data,
            backgroundColor: colors.slice(0, data.length),
            borderWidth: 2,
            borderColor: '#FFFFFF',
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: {
          legend: { position: 'bottom', labels: { padding: 16 } },
          tooltip: { mode: 'index' },
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
