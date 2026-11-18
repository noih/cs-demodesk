import { useEffect, useId, useRef, useState } from 'react';
import { Badge, Button, Checkbox, Flex, IconButton, Text, Tooltip } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, clock, errorText, type EditingAnalysis, type EditingCandidate, type EditingPreviewShot, type Highlight, type RenderOptions, type ReplayLink } from '../api.ts';
import { recommendedReplayLinks, replayPairs, retainReplayLinks } from '../editingReplayLinks.ts';

export interface EditingChoice {
  context: string;
  analysis?: EditingAnalysis;
  selected: string[];
  links: ReplayLink[];
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
  const [links, setLinks] = useState<ReplayLink[]>([]);
  const [preview, setPreview] = useState<EditingPreviewShot[]>([]);
  const [previewPending, setPreviewPending] = useState(false);
  const [previewError, setPreviewError] = useState<string>();
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string>();
  const [expanded, setExpanded] = useState<string[]>(highlights.slice(0, 1).map(h => h.id));
  useEffect(() => {
    const current = ++request.current;
    api.cachedEditingAnalysis(input.demoId, input.ids, input.options).then(result => {
      if (current !== request.current) return;
      const picks = result?.candidates.filter(c => c.recommended).map(c => c.id) ?? [];
      setAnalysis(result ?? undefined); setSelected(picks); setLinks(recommendedReplayLinks(result?.candidates ?? [], picks)); setBusy(false);
    }).catch(e => {
      if (current !== request.current) return;
      setError(errorText(e)); setBusy(false);
    });
    return () => { request.current++; };
  }, [context, input]);
  useEffect(() => {
    let active = true;
    setPreviewError(undefined);
    if (busy || !analysis || selected.length === 0) {
      setPreview([]); setPreviewPending(false);
      onChange({ context, analysis, selected, links, busy });
      return;
    }
    setPreviewPending(true);
    onChange({ context, analysis, selected, links, busy: true });
    void api.previewEditingPlan(input.demoId, input.ids, {
      ...input.options, editingAnalysisId: analysis.id, editingSelections: selected, editingReplayLinks: links,
    }).then(shots => {
      if (!active) return;
      setPreview(shots); setPreviewPending(false);
      onChange({ context, analysis, selected, links, busy: false });
    }).catch(e => {
      if (!active) return;
      setPreview([]); setPreviewPending(false); setPreviewError(errorText(e));
    });
    return () => { active = false; };
  }, [context, input, analysis, selected, links, busy, onChange]);
  const publish = (result: EditingAnalysis | undefined, picks: string[], pending: boolean, nextLinks?: ReplayLink[]) => {
    const kept = nextLinks ?? retainReplayLinks(result?.candidates ?? [], picks, links);
    setAnalysis(result); setSelected(picks); setLinks(kept); setBusy(pending);
    onChange({ context, analysis: result, selected: picks, links: kept, busy: pending || picks.length > 0 });
  };
  const analyze = async (force = false) => {
    if (busy) return;
    const current = ++request.current;
    setError(undefined); publish(undefined, [], true);
    try {
      const result = await api.analyzeEditing(demoId, highlights.map(h => h.id), options, force);
      if (current === request.current) {
        const picks = result.candidates.filter(c => c.recommended).map(c => c.id);
        publish(result, picks, false, recommendedReplayLinks(result.candidates, picks));
      }
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
    {analysis?.warnings.map(w => <Text as="p" key={w} size="1" color="amber" role="status">{t(w === 'openingUnavailable' ? 'editingEffects.openingUnavailable' : 'editingEffects.pushUnavailable')}</Text>)}
    {analysis && <>
      {analysis.candidates.length === 0 ? <Text as="p" size="2">{t('editingEffects.empty')}</Text> : <>
        {selected.length > 0 && <Text as="p" role="status" size="1" color={options.maxSizeMb != null ? 'amber' : 'gray'} mt="0" mb="2">{t(options.maxSizeMb != null ? 'editingEffects.sizeLimitWarning' : 'editingEffects.durationHint')}</Text>}
        {highlights.map((h, index) => {
          const candidates = analysis.candidates.filter(c => c.highlightId === h.id);
          const replays = candidates.filter(c => c.kind === 'combat').sort((a, b) => a.tick - b.tick || a.id.localeCompare(b.id));
          const pairs = replayPairs(replays, selected);
          const sequence = preview.filter(shot => shot.highlightId === h.id);
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
                {replays.map(candidate => {
                  const pair = pairs.find(pair => pair.to === candidate.id);
                  const linked = !!pair && links.some(link => link.from === pair.from && link.to === pair.to);
                  const hint = t(linked ? 'editingEffects.replayLinked' : 'editingEffects.replayUnlinked');
                  return <div key={candidate.id} className="editing-replay-row">
                    {candidateRow(candidate)}
                    {pair ? <Tooltip delayDuration={150} content={hint}>
                      <IconButton type="button" size="1" variant={linked ? 'soft' : 'ghost'} color={linked ? 'amber' : 'gray'}
                        className="editing-replay-link" disabled={busy} aria-pressed={linked}
                        aria-label={t('editingEffects.replayLinkLabel', { target: candidate.target })}
                        onClick={() => publish(analysis, selected, false, linked
                          ? links.filter(link => link.to !== pair.to)
                          : [...links, pair])}>
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
                          <path d="M9 6H7a6 6 0 0 0 0 12h2M15 6h2a6 6 0 0 1 0 12h-2" />
                          <path d={linked ? 'M8 12h8' : 'M8 12h1m6 0h1M11 5l2-3m-2 20 2-3'} />
                        </svg>
                      </IconButton>
                    </Tooltip> : <span className="editing-replay-link-space" />}
                  </div>;
                })}
                {sequence.length > 0 && <div className="editing-playback" aria-busy={previewPending}>
                  <Text as="div" size="1" color="gray">{t('editingEffects.playbackOrder')}</Text>
                  <div className="editing-playback-flow" aria-label={t('editingEffects.playbackOrder')} aria-live="polite">
                    {sequence.map((shot, i) => <span key={`${i}-${shot.startTick}`} className="editing-playback-step">
                      {i > 0 && <span aria-hidden="true" className="editing-playback-arrow">→</span>}
                      <Tooltip delayDuration={150} content={`${clock(shot.startTick, tickRate)} – ${clock(shot.endTick, tickRate)}`}>
                        <Badge size="1" color={shot.kind === 'main' ? 'amber' : 'gray'}>{shot.kind === 'main'
                          ? t('editingEffects.mainPlayer') : shot.kind === 'opening' ? t('editingEffects.opening.title') : shot.player}</Badge>
                      </Tooltip>
                    </span>)}
                  </div>
                </div>}
              </fieldset>}
            </div>
          </section>;
        })}
      </>}
    </>}
    {previewError && <Text as="p" size="1" color="red" role="alert">{t('editingEffects.sequenceUnavailable')} {previewError}</Text>}
  </section>;
}
