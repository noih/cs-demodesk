import { useEffect, useRef } from 'react';
import { useAppTheme } from '../AppTheme.tsx';
import * as echarts from 'echarts/core';
import { BarChart, LineChart, RadarChart, ScatterChart } from 'echarts/charts';
import { DataZoomInsideComponent, GridComponent, LegendComponent, TooltipComponent, RadarComponent } from 'echarts/components';
import { LabelLayout } from 'echarts/features';
import { CanvasRenderer } from 'echarts/renderers';
import type { EChartsCoreOption, ECharts } from 'echarts/core';

echarts.use([BarChart, LineChart, RadarChart, ScatterChart, DataZoomInsideComponent, GridComponent, LegendComponent, TooltipComponent, RadarComponent, CanvasRenderer, LabelLayout]);

/** Ten distinct colours, one per player in a match (team A takes the first five). */
export const PLAYER_COLORS = ['#5b9cf6', '#f59e0b', '#22c55e', '#ef4444', '#a855f7', '#2dd4bf', '#facc15', '#f472b6', '#e5e7eb', '#a3e635'];

/** steamid → colour, stable for one parsed demo (team A first, then B, in roster order). */
export function playerColors(stats: Array<{ steamid: string; team: 'A' | 'B' }>, palette: string[] = PLAYER_COLORS): Map<string, string> {
  const ordered = [...stats.filter((p) => p.team === 'A'), ...stats.filter((p) => p.team === 'B')];
  return new Map(ordered.map((p, i) => [p.steamid, palette[i % palette.length]!]));
}

export function EChart({ option, height = 320 }: { option: EChartsCoreOption; height?: number | string }) {
  const { colors, typography } = useAppTheme();
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
    const axisText = (value: unknown): unknown => {
      if (!value) return value;
      if (Array.isArray(value)) return value.map(axisText);
      const axis = value as Record<string, unknown>;
      return { ...axis, axisLabel: { ...(axis.axisLabel as object), fontSize: typography[3] }, nameTextStyle: { ...(axis.nameTextStyle as object), fontSize: typography[3] } };
    };
    const radar = option.radar as Record<string, unknown> | undefined;
    chart.current?.setOption({ backgroundColor: 'transparent', textStyle: { color: colors.text, fontSize: typography[3] }, ...option, animation: false, animationDuration: 0, animationDurationUpdate: 0,
      xAxis: axisText(option.xAxis), yAxis: axisText(option.yAxis),
      ...(radar ? { radar: { ...radar, axisName: { ...(radar.axisName as object), fontSize: typography[3] } } } : {}),
      tooltip: { backgroundColor: colors.panel, borderColor: colors.border, textStyle: { color: colors.text, fontSize: typography[3] }, ...(option.tooltip as object), transitionDuration: 0 } }, true);
  }, [option, colors, typography]);

  return <div ref={ref} className="chart" style={{ height }} />;
}
