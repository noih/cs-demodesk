import { useState } from 'react';
import { Button, Popover, Text } from '@radix-ui/themes';
import { DayPicker } from 'react-day-picker';
import { enUS, ja, ko, ru, zhCN, zhTW } from 'date-fns/locale';
import { useTranslation } from 'react-i18next';
import { fmtDate, type Language } from '../i18n/index.ts';

const LOCALES: Record<Language, typeof enUS> = { en: enUS, 'zh-TW': zhTW, 'zh-CN': zhCN, ja, ko, ru };

/** Local calendar day as "YYYY-MM-DD"; compares and sorts as plain text. */
export function dayOf(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

const localMidnight = (day: string) => new Date(`${day}T00:00`);

/** One-day calendar picker; value is "YYYY-MM-DD" or "". Text and calendar follow the UI language, not the OS. */
export function DateField({ value, onChange, label, min, max }: { value: string; onChange: (day: string) => void; label: string; min?: string; max?: string }) {
  const { i18n } = useTranslation();
  const [open, setOpen] = useState(false);
  const selected = value ? localMidnight(value) : undefined;
  const text = selected ? fmtDate(selected.getTime()) : label;
  const disabled = [...(min ? [{ before: localMidnight(min) }] : []), ...(max ? [{ after: localMidnight(max) }] : [])];
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger>
        <Button size="2" variant="surface" color="gray" aria-label={label} style={{ flex: 1, minWidth: 0 }}>
          <i aria-hidden="true" className="bi bi-calendar3 app-icon" style={{ flexShrink: 0 }} />
          <Text truncate title={text}>{text}</Text>
        </Button>
      </Popover.Trigger>
      <Popover.Content size="1">
        <DayPicker
          mode="single"
          locale={LOCALES[i18n.language as Language] ?? enUS}
          selected={selected}
          defaultMonth={selected}
          disabled={disabled}
          onSelect={(d) => {
            onChange(d ? dayOf(d) : '');
            setOpen(false);
          }}
        />
      </Popover.Content>
    </Popover.Root>
  );
}
