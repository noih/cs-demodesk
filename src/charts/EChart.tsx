import { useEffect, useRef } from 'react';
import { useAppTheme } from '../AppTheme.tsx';
import * as echarts from 'echarts/core';
import { BarChart, LineChart, RadarChart, ScatterChart } from 'echarts/charts';
import { GridComponent, LegendComponent, TooltipComponent, RadarComponent } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import type { EChartsCoreOption, ECharts } from 'echarts/core';

echarts.use([BarChart, LineChart, RadarChart, ScatterChart, GridComponent, LegendComponent, TooltipComponent, RadarComponent, CanvasRenderer]);

/** Ten distinct colours, one per player in a match (team A takes the first five). */
export const PLAYER_COLORS = ['#5b9cf6', '#f59e0b', '#22c55e', '#ef4444', '#a855f7', '#2dd4bf', '#facc15', '#f472b6', '#e5e7eb', '#a3e635'];

/** steamid → colour, stable for one parsed demo (team A first, then B, in roster order). */
export function playerColors(stats: Array<{ steamid: string; team: 'A' | 'B' }>, palette: string[] = PLAYER_COLORS): Map<string, string> {
  const ordered = [...stats.filter((p) => p.team === 'A'), ...stats.filter((p) => p.team === 'B')];
  return new Map(ordered.map((p, i) => [p.steamid, palette[i % palette.length]!]));
}

export function EChart({ option, height = 320 }: { option: EChartsCoreOption; height?: number }) {
  const { colors } = useAppTheme();
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
    chart.current?.setOption({ backgroundColor: 'transparent', textStyle: { color: colors.text }, ...option, tooltip: { backgroundColor: colors.panel, borderColor: colors.border, textStyle: { color: colors.text }, ...(option.tooltip as object) } }, true);
  }, [option, colors]);

  return <div ref={ref} className="chart" style={{ height }} />;
}
