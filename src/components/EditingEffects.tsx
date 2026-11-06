import { useEffect, useId, useRef, useState } from 'react';
import { Badge, Button, Checkbox, Flex, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, clock, errorText, type EditingAnalysis, type EditingCandidate, type Highlight, type RenderOptions } from '../api.ts';

export interface EditingChoice {
  context: string;
  analysis?: EditingAnalysis;
  selected: string[];
  busy: boolean;
}

export function EditingEffects({ context, demoId, highlights, tickRate, options, onChange }: {
  context: string; demoId: string; highlights: Highlight[]; tickRate: number; options: RenderOptions;
  onChange: (choice: EditingChoice) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const request = useRef(0);
  const [input] = useState(() => ({ demoId, ids: highlights.map(h => h.id), options }));
  const [analysis, setAnalysis] = useState<EditingAnalysis>();
  const [selected, setSelected] = useState<string[]>([]);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string>();
  const [expanded, setExpanded] = useState<string[]>(highlights.slice(0, 1).map(h => h.id));
  useEffect(() => {
    const current = ++request.current;
    onChange({ context, selected: [], busy: true });
    api.cachedEditingAnalysis(input.demoId, input.ids, input.options).then(result => {
      if (current !== request.current) return;
      const picks = result?.candidates.filter(c => c.recommended).map(c => c.id) ?? [];
      setAnalysis(result ?? undefined); setSelected(picks); setBusy(false);
      onChange({ context, analysis: result ?? undefined, selected: picks, busy: false });
    }).catch(e => {
      if (current !== request.current) return;
      setError(errorText(e)); setBusy(false);
      onChange({ context, selected: [], busy: false });
    });
    return () => { request.current++; };
  }, [context, input, onChange]);
  const publish = (result: EditingAnalysis | undefined, picks: string[], pending: boolean) => {
    setAnalysis(result); setSelected(picks); setBusy(pending);
    onChange({ context, analysis: result, selected: picks, busy: pending });
  };
  const analyze = async (force = false) => {
    if (busy) return;
    const current = ++request.current;
    setError(undefined); publish(undefined, [], true);
    try {
      const result = await api.analyzeEditing(demoId, highlights.map(h => h.id), options, force);
      if (current === request.current) publish(result, result.candidates.filter(c => c.recommended).map(c => c.id), false);
    } catch (e) {
      if (current === request.current) { setError(errorText(e)); publish(undefined, [], false); }
    }
  };
  const candidateRow = (candidate: EditingCandidate) => {
    const title = t(`editingEffects.${candidate.kind}.title`);
    const checked = selected.includes(candidate.id);
    return <label key={candidate.id} className="editing-option-row" data-enabled={checked}>
      <Checkbox size="2" checked={checked} disabled={busy}
        aria-label={`${title}${candidate.target ? ` · ${candidate.target}` : ''} ${candidate.tick}`}
        onCheckedChange={value => publish(analysis, value
          ? [...selected.filter(pick => !candidate.conflicts.includes(pick)), candidate.id]
          : selected.filter(pick => pick !== candidate.id), false)} />
      <span className="editing-option-copy">
        <Text as="span" size="2" weight="medium">{candidate.kind === 'combat' ? candidate.target : title}</Text>
        <Text as="span" size="1" color="gray">{candidate.kind === 'combat'
          ? clock(candidate.tick, tickRate)
          : t(`editingEffects.${candidate.kind}.hint`)}</Text>
      </span>
      {candidate.recommended && <Badge size="1" color="gray">{t('editingEffects.recommended')}</Badge>}
    </label>;
  };
  return <section className="editing-effects" aria-labelledby={`${id}-title`} aria-busy={busy}>
    <Flex justify="between" align="center" gap="2" wrap="wrap" mb="2">
      <Text id={`${id}-title`} weight="medium">{t('editingEffects.title')}</Text>
      <Button size="1" variant="soft" disabled={busy} onClick={() => void analyze(!!analysis)}>
        {busy ? t('editingEffects.analyzing') : analysis ? t('editingEffects.reanalyze') : t('editingEffects.analyze')}
      </Button>
    </Flex>
    {error && <Text as="p" size="1" color="red" role="alert">{error}</Text>}
    {analysis?.warnings.map(w => <Text as="p" key={w} size="1" color="amber" role="status">{t('editingEffects.pushUnavailable')}</Text>)}
    {analysis && <>
      {analysis.candidates.length === 0 ? <Text as="p" size="2">{t('editingEffects.empty')}</Text> : <>
        {selected.length > 0 && <Text as="p" role="status" size="1" color={options.maxSizeMb != null ? 'amber' : 'gray'} mt="0" mb="2">{t(options.maxSizeMb != null ? 'editingEffects.sizeLimitWarning' : 'editingEffects.durationHint')}</Text>}
        {highlights.map((h, index) => {
          const candidates = analysis.candidates.filter(c => c.highlightId === h.id);
          const replays = candidates.filter(c => c.kind === 'combat');
          const other = candidates.filter(c => c.kind !== 'combat');
          const open = candidates.length > 0 && expanded.includes(h.id);
          const groupId = `${id}-highlight-${index}`;
          return <section key={h.id} className="editing-highlight" aria-labelledby={`${groupId}-title`}>
            <button type="button" className="editing-highlight-header" aria-expanded={open}
              aria-controls={groupId} disabled={candidates.length === 0}
              onClick={() => setExpanded(previous => open ? previous.filter(value => value !== h.id) : [...previous, h.id])}>
              <i className={`bi bi-chevron-${open ? 'down' : 'right'}`} aria-hidden="true" />
              <Text id={`${groupId}-title`} as="span" size="2" weight="medium" className="editing-highlight-title">{h.title}</Text>
              <Text as="span" size="1" color="gray" className="editing-highlight-count">{candidates.length > 0
                ? t('editingEffects.selected', { count: candidates.filter(c => selected.includes(c.id)).length, total: candidates.length })
                : t('editingEffects.groupEmpty')}</Text>
            </button>
            <div id={groupId} hidden={!open} className="editing-highlight-options">
              {other.map(candidateRow)}
              {replays.length > 0 && <fieldset className="editing-combat-options">
                <legend><Text size="2" weight="medium">{t('editingEffects.combat.title')}</Text></legend>
                {replays.map(candidateRow)}
              </fieldset>}
            </div>
          </section>;
        })}
      </>}
    </>}
  </section>;
}
