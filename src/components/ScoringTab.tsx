import { AnalysisExport } from './AnalysisExport.tsx';
import { useEffect, useRef, useState } from 'react';
import { Badge, Button, Card, Dialog, Flex, Heading, Table, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { api, clock, errorText, type Status, type AnalysisJob, type ScoringStep, type MatchAssessment, type Assessment, type BehaviorCheck, type AnalysisMeasurement, type DemoMeta, type ParsedDemo } from '../api.ts';
import { displayPlayerName } from '../playerName.ts';

export function ScoringTab({meta,parsed,assessment,busy,step,error,job,queuePosition,status,onSetup,onRendered,onAnalyze}: {
  status?:Status;onSetup:()=>void;onRendered:()=>void;onAnalyze:()=>void;
  meta: DemoMeta; parsed: ParsedDemo; assessment?: MatchAssessment; busy: boolean; step: ScoringStep; error?: string; job?: AnalysisJob; queuePosition?: number;
}) {
  const {t} = useTranslation();
  const [saved, setSaved] = useState<Record<string, Assessment[]>>({});
  const [historyError,setHistoryError] = useState<string>();
  const request = useRef(0);
  useEffect(() => {
    const generation = ++request.current;
    setHistoryError(undefined);
    setSaved({});
    if (assessment) return;
    // Load saved results for the roster; this never scans or analyzes the demo.
    void api.scoringHistory(meta.id).then(records => {
      if (request.current === generation) setSaved(records);
    }).catch(error => {
      if (request.current === generation) setHistoryError(errorText(error));
    });
    return () => { request.current++; };
  }, [meta.id, meta.parsedAt, parsed.info.players, assessment]);
  const players = assessment?.players ?? saved;
  const playerNames = Object.fromEntries(parsed.info.players.map(player => [player.steamid, displayPlayerName(player.name)]));
  const hasRecords = Object.values(players).some(records => records.length > 0);
  return <Flex direction="column" gap="3">
    <Flex align="center" gap="3" wrap="wrap">
      <Button size="1" onClick={onAnalyze} disabled={busy} aria-busy={busy}>
        {busy ? <span role="status">{job?.status==='queued' ? t('scoring.queue.waiting',{position:queuePosition}) : job?.status==='running' ? `${t('scoring.progress',{step,total:3})} · ${t(`scoring.steps.${step}`)}` : t('scoring.queue.submitting')}</span> : t(hasRecords?'scoring.restart':'scoring.start')}
      </Button>
    {!busy && job?.status==='error' && <Text role="alert" color="red">{t('scoring.failed')}{job.step && ` · ${t(`scoring.steps.${job.step}`)}`}: {job.error}</Text>}
    {(error || historyError) && <Text color="red" role="alert">{t('scoring.failed')}: {error || historyError}</Text>}
    </Flex>
    {hasRecords && <div style={{overflowX: 'auto', maxWidth: '100%'}}><Table.Root variant="surface" layout="auto" aria-label={t('scoring.title')}>
      <Table.Header><Table.Row>
        <Table.ColumnHeaderCell width="22%">{t('common.player')}</Table.ColumnHeaderCell>
        <Table.ColumnHeaderCell>{t('scoring.behavior')}</Table.ColumnHeaderCell>
        <Table.ColumnHeaderCell width="10em" style={{whiteSpace:'normal'}}>{t('scoring.details')}</Table.ColumnHeaderCell>
      </Table.Row></Table.Header>
      <Table.Body>{parsed.info.players.map(player => <PlayerAnalysis
        key={player.steamid} playerId={player.steamid} name={displayPlayerName(player.name)}
        records={players[player.steamid]} playerNames={playerNames} status={status} onSetup={onSetup} onRendered={onRendered}
      />)}</Table.Body>
    </Table.Root></div>}
  </Flex>;
}

function occurrenceCount(check: BehaviorCheck): number | null {
  if (check.state === 'unavailable' || check.state === 'failed') return null;
  return check.occurrences.length;
}

function Measurements({values}: {values: AnalysisMeasurement[]}) {
  const {t} = useTranslation();
  return <>{values.filter(m => m.name !== 'unclassifiedShots').map(m => <Text as="p" size="2" key={`${m.name}:${m.value}:${m.unit}:${m.threshold}`}>
    {t(`scoring.metrics.${m.name}`, {defaultValue:m.name})}: {Number.isFinite(m.value) ? Number(m.value.toFixed(4)) : '—'} {m.unit}
    {m.threshold != null && ` · ${t('scoring.threshold')}: ${m.threshold}`}
  </Text>)}</>;
}

function PlayerAnalysis({playerId,name,records,playerNames,status,onSetup,onRendered}: {
  playerId: string; name: string; records?: Assessment[];status?:Status;onSetup:()=>void;onRendered:()=>void;
  playerNames: Record<string, string>;
}) {
  const {t} = useTranslation();
  const latest = records?.[0];
  const record = latest;
  const observed = record?.checks.filter(check => (occurrenceCount(check) ?? 0) > 0) ?? [];
  const summaries = record?.checks.filter(check => check.summary.length > 0 && check.state !== 'unavailable' && check.state !== 'failed') ?? [];
  const unavailable = record?.checks.filter(check => check.state === 'unavailable' || check.state === 'failed') ?? [];
  return <Table.Row data-player-id={playerId}>
    <Table.RowHeaderCell style={{whiteSpace:'normal',overflowWrap:'anywhere'}}>{name}</Table.RowHeaderCell>
    <Table.Cell>
      {observed.length > 0 ? <Flex wrap="wrap" gap="2">{observed.map(check => <Badge key={check.definition.id} color="gray" size="2" style={{whiteSpace:'normal'}}>
        {t(`scoring.ruleNames.${check.definition.id}`, {defaultValue:check.definition.name})}
        <strong data-rule-count={check.definition.id} style={{fontVariantNumeric:'tabular-nums'}}>{check.occurrences.length}</strong>
      </Badge>)}</Flex> : '—'}
    </Table.Cell>
    <Table.Cell>
      <Flex align="center" gap="2">
      {record && <AnalysisExport record={record} status={status} onSetup={onSetup} onRendered={onRendered}/>}
      {record ? <Dialog.Root>
        <Dialog.Trigger><Button variant="soft" style={{whiteSpace:'normal',height:'auto',minHeight:'var(--button-height)',paddingBlock:8}}>{t('scoring.detailsShort')}</Button></Dialog.Trigger>
        {record && <Dialog.Content aria-describedby={undefined} maxWidth="960px" style={{maxHeight:'85vh',overflowY:'auto',overflowWrap:'anywhere'}}>
          <Dialog.Title>{name} · {t('scoring.details')}</Dialog.Title>
          <Flex direction="column" gap="3">
            {observed.length > 0 && <div style={{overflowX:'auto'}}><Table.Root variant="surface" layout="fixed">
              <Table.Header><Table.Row><Table.ColumnHeaderCell width="20%" style={{whiteSpace:'normal'}}>{t('scoring.behavior')}</Table.ColumnHeaderCell><Table.ColumnHeaderCell width="10%" style={{whiteSpace:'normal'}}>{t('scoring.count')}</Table.ColumnHeaderCell><Table.ColumnHeaderCell>{t('scoring.details')}</Table.ColumnHeaderCell></Table.Row></Table.Header>
              <Table.Body>{observed.map(check => <Table.Row key={check.definition.id} data-check-id={check.definition.id}>
                <Table.RowHeaderCell style={{whiteSpace:'normal'}}>{t(`scoring.ruleNames.${check.definition.id}`, {defaultValue:check.definition.name})}</Table.RowHeaderCell>
                <Table.Cell>{check.occurrences.length}</Table.Cell>
                <Table.Cell style={{whiteSpace:'normal'}}><Text as="p" size="2">{t(`scoring.ruleDescriptions.${check.definition.id}`, {defaultValue:check.definition.description})}</Text>
                  {check.reasonCode === 'shotPathsMissing' && <Text as="p" size="2" color="gray">{t('scoring.reasons.shotPathsMissing')}</Text>}
                  {check.occurrences.map(occurrence => <Card key={occurrence.id} data-occurrence-id={occurrence.id} mt="2">
                    <Text as="p" size="2">{t('common.roundN',{n:occurrence.round})} · {t('scoring.ticks')}: {occurrence.startTick}–{occurrence.endTick} · {clock(occurrence.startTick,record.tickRate)}–{clock(occurrence.endTick,record.tickRate)}</Text>
                    {occurrence.targetId && <Text as="p" size="2">{t('scoring.target')}: {playerNames[occurrence.targetId] ?? occurrence.targetId}</Text>}
                    <Measurements values={occurrence.measurements} />
                    <details><summary>{t('scoring.sourceRecords')}</summary><Text size="1">{occurrence.sourceIds.join(', ')}</Text></details>
                  </Card>)}
                </Table.Cell>
              </Table.Row>)}</Table.Body>
            </Table.Root></div>}
            {summaries.length > 0 && <><Heading size="3">{t('scoring.measuredSummary')}</Heading><Table.Root variant="surface" layout="fixed"><Table.Body>{summaries.map(check => <Table.Row key={check.definition.id}>
              <Table.RowHeaderCell style={{whiteSpace:'normal'}}>{t(`scoring.ruleNames.${check.definition.id}`,{defaultValue:check.definition.name})}</Table.RowHeaderCell><Table.Cell style={{whiteSpace:'normal'}}><Measurements values={check.summary} /></Table.Cell>
            </Table.Row>)}</Table.Body></Table.Root></>}
            {unavailable.length > 0 && <><Heading size="3">{t('scoring.dataStatus')}</Heading>{unavailable.map(check => <Text as="p" size="2" key={check.definition.id}>
              {t(`scoring.ruleNames.${check.definition.id}`,{defaultValue:check.definition.name})} · {t(`scoring.${check.state}`)}: {t(`scoring.reasons.${check.reasonCode}`,{defaultValue:check.reason})}
            </Text>)}</>}
            <Flex justify="end"><Dialog.Close><Button>{t('common.close')}</Button></Dialog.Close></Flex>
          </Flex>
        </Dialog.Content>}
      </Dialog.Root> : '—'}
      </Flex>
    </Table.Cell>
  </Table.Row>;
}
