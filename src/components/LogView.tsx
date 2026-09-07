import { useLayoutEffect, useRef } from 'react';

/** Scrolling log box that sticks to the bottom until the user scrolls up, and
 * sticks again once they scroll back down. */
export function LogView({ lines, empty }: { lines: string[]; empty: string }) {
  const ref = useRef<HTMLPreElement>(null);
  const stick = useRef(true);
  useLayoutEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [lines]);
  return (
    <pre
      ref={ref}
      className="log mono"
      style={{ marginTop: 12, maxHeight: '60vh' }}
      onScroll={(e) => {
        const el = e.currentTarget;
        stick.current = el.scrollTop + el.clientHeight >= el.scrollHeight - 4;
      }}
    >
      {lines.length ? lines.join('\n') : empty}
    </pre>
  );
}
