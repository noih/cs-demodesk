import type { DemoMeta } from './api.ts';

export function demoDate(demo: DemoMeta): number {
  return (demo.status === 'parsed' ? demo.matchTimeMs : undefined) ?? demo.createdMs ?? demo.mtimeMs;
}

export function compareDemoDates(a: DemoMeta, b: DemoMeta): number {
  return demoDate(b) - demoDate(a) || a.id.localeCompare(b.id);
}
