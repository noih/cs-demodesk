import { useEffect, useRef } from 'react';
import * as echarts from 'echarts/core';
import { BarChart, LineChart, RadarChart, ScatterChart } from 'echarts/charts';
import { GridComponent, LegendComponent, TooltipComponent, RadarComponent } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import type { EChartsCoreOption, ECharts } from 'echarts/core';

echarts.use([BarChart, LineChart, RadarChart, ScatterChart, GridComponent, LegendComponent, TooltipComponent, RadarComponent, CanvasRenderer]);

/** Shared look for every chart so they read as one system in the dark theme. */
export const CHART_THEME = {
  teamA: '#6cb6ff',
  teamB: '#ffb347',
  accent: '#f0a500',
  text: '#c8ccd2',
  muted: '#8b93a1',
  grid: 'rgba(255,255,255,0.08)',
};

/** Ten distinct colours, one per player in a match (team A takes the first five). */
export const PLAYER_COLORS = ['#5b9cf6', '#f59e0b', '#22c55e', '#ef4444', '#a855f7', '#2dd4bf', '#facc15', '#f472b6', '#e5e7eb', '#a3e635'];

/** steamid → colour, stable for one parsed demo (team A first, then B, in roster order). */
export function playerColors(stats: Array<{ steamid: string; team: 'A' | 'B' }>): Map<string, string> {
  const ordered = [...stats.filter((p) => p.team === 'A'), ...stats.filter((p) => p.team === 'B')];
  return new Map(ordered.map((p, i) => [p.steamid, PLAYER_COLORS[i % PLAYER_COLORS.length]!]));
}

export function EChart({ option, height = 320 }: { option: EChartsCoreOption; height?: number }) {
  const ref = useRef<HTMLDivElement>(null);
  const chart = useRef<ECharts | undefined>(undefined);

  useEffect(() => {
    if (!ref.current) return;
    chart.current = echarts.init(ref.current, undefined, { renderer: 'canvas' });
    const ro = new ResizeObserver(() => chart.current?.resize());
    ro.observe(ref.current);
    return () => {
      ro.disconnect();
      chart.current?.dispose();
    };
  }, []);

  useEffect(() => {
    chart.current?.setOption({ backgroundColor: 'transparent', textStyle: { color: CHART_THEME.text }, ...option }, true);
  }, [option]);

  return <div ref={ref} className="chart" style={{ height }} />;
}
