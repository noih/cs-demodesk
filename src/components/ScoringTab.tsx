import { AnalysisExport } from './AnalysisExport.tsx';
import { useState } from 'react';
import { Badge, Button, Dialog, Flex, Heading, Table, Text } from '@radix-ui/themes';
import { useTranslation } from 'react-i18next';
import { clock, type Status, type AnalysisJob, type ScoringStep, type Assessment, type BehaviorCheck, type AnalysisMeasurement, type ParsedDemo } from '../api.ts';
import { displayPlayerName } from '../playerName.ts';

export function ScoringTab({players,parsed,busy,step,error,job,queuePosition,status,onSetup,onRendered,onAnalyze}: {
  status?:Status;onSetup:()=>void;onRendered:()=>void;onAnalyze:()=>void;
  players: Record<string, Assessment[]> | undefined; parsed: ParsedDemo; busy: boolean; step: ScoringStep; error?: string; job?: AnalysisJob; queuePosition?: number;
}) {
  const {t} = useTranslation();
  const playerNames = Object.fromEntries(parsed.info.players.map(player => [player.steamid, displayPlayerName(player.name)]));
  const hasRecords = Object.values(players ?? {}).some(records => records.length > 0);
  return <Flex direction="column" gap="3">
    <Flex align="center" justify={hasRecords ? "end" : "start"} gap="3" wrap="wrap">
      {((players !== undefined && !hasRecords) || busy) && <Button size="1" onClick={onAnalyze} disabled={busy} aria-busy={busy}>
        {busy ? <span role="status">{job?.status==='queued' ? t('scoring.queue.waiting',{position:queuePosition}) : job?.status==='running' ? `${t('scoring.progress',{step,total:3})} · ${t(`scoring.steps.${step}`)}` : t('scoring.queue.submitting')}</span> : t('scoring.start')}
      </Button>}
    {!busy && job?.status==='error' && <Text role="alert" color="red">{t('scoring.failed')}{job.step && ` · ${t(`scoring.steps.${job.step}`)}`}: {job.error}</Text>}
    {error && <Text color="red" role="alert">{t('scoring.failed')}: {error}</Text>}
    </Flex>
    {hasRecords && <div style={{overflowX: 'auto', maxWidth: '100%'}}><Table.Root variant="surface" layout="auto" aria-label={t('scoring.title')}>
      <Table.Header><Table.Row>
        <Table.ColumnHeaderCell width="22%">{t('common.player')}</Table.ColumnHeaderCell>
        <Table.ColumnHeaderCell>{t('scoring.behavior')}</Table.ColumnHeaderCell>
        <Table.ColumnHeaderCell width="10em" style={{whiteSpace:'normal'}}>{t('scoring.details')}</Table.ColumnHeaderCell>
      </Table.Row></Table.Header>
      <Table.Body>{parsed.info.players.map(player => <PlayerAnalysis
        key={player.steamid} playerId={player.steamid} name={displayPlayerName(player.name)}
        records={players?.[player.steamid]} playerNames={playerNames} status={status} onSetup={onSetup} onRendered={onRendered}
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
  const rate = values.find(m => m.name === 'smokeHitRate')?.value;
  const shots = values.find(m => m.name === 'smokeShots')?.value ?? 0;
  const hits = values.find(m => m.name === 'estimatedSmokeHits')?.value ?? 0;
  return <>{shots > 0 && rate !== undefined && <Text as="p" size="2" data-smoke-estimate>
    {t('scoring.metrics.smokeHitRate')}: {Number(rate.toFixed(1))}% · {hits}／{shots} {t('scoring.units.shots')}
  </Text>}{values.filter(m => !['unclassifiedShots','smokeHitRate','smokeShots','estimatedSmokeHits'].includes(m.name) && !(m.name === 'smokeHits' && m.value === 0)).map(m => <Text as="p" size="2" key={`${m.name}:${m.value}:${t(`scoring.units.${m.unit}`, {defaultValue:m.unit})}:${m.threshold}`}>
    {t(`scoring.metrics.${m.name}`, {defaultValue:m.name})}: {Number.isFinite(m.value) ? Number(m.value.toFixed(4)) : '—'} {t(`scoring.units.${m.unit}`, {defaultValue:m.unit})}
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
  const smoke = record?.checks.find(check => check.definition.id === 'smoke-hit-rate');
  const smokeRate = smoke?.summary.find(m => m.name === 'smokeHitRate')?.value;
  const observed = record?.checks.filter(check => (occurrenceCount(check) ?? 0) > 0) ?? [];
  const summaries = record?.checks.filter(check => check.summary.length > 0 && check.state !== 'unavailable' && check.state !== 'failed') ?? [];
  return <Table.Row data-player-id={playerId}>
    <Table.RowHeaderCell style={{whiteSpace:'normal',overflowWrap:'anywhere'}}>{name}</Table.RowHeaderCell>
    <Table.Cell>
      {observed.length > 0 || smokeRate !== undefined ? <Flex wrap="wrap" gap="2">{smokeRate !== undefined && <Badge color="gray" size="2">{t('scoring.metrics.smokeHitRate')} · {Number(smokeRate.toFixed(1))}%</Badge>}{observed.map(check => <Badge key={check.definition.id} color="gray" size="2" style={{whiteSpace:'normal'}}>
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
            {summaries.length > 0 && <><Heading size="3">{t('scoring.measuredSummary')}</Heading>
              <Table.Root size="1" variant="surface"><Table.Body>{summaries.map(check => <Table.Row key={check.definition.id} data-summary-id={check.definition.id}>
                <Table.RowHeaderCell>{t(`scoring.ruleNames.${check.definition.id}`,{defaultValue:check.definition.name})}</Table.RowHeaderCell>
                <Table.Cell><Measurements values={check.summary} /></Table.Cell>
              </Table.Row>)}</Table.Body></Table.Root>
            </>}
            {observed.length > 0 && <Heading size="3">{t('scoring.details')}</Heading>}
            {observed.map(check => <details key={check.definition.id} data-check-id={check.definition.id} style={{borderBottom:'1px solid var(--gray-a5)',paddingBottom:12}}>
              <summary style={{cursor:'pointer'}}>{t(`scoring.ruleNames.${check.definition.id}`, {defaultValue:check.definition.name})} <Badge color="gray" ml="2">{check.occurrences.length}</Badge></summary>
              <Text as="p" size="2" color="gray" my="2">{t(`scoring.ruleDescriptions.${check.definition.id}`, {defaultValue:check.definition.description})}</Text>
              <EvidenceTable items={check.occurrences} tickRate={record.tickRate} playerNames={playerNames} />
            </details>)}
            {summaries.filter(check => check.definition.id === 'time-to-damage').map(check => <details key={check.definition.id}>
              <summary style={{cursor:'pointer'}}>{t('scoring.ttdSamples')}</summary>
              <EvidenceTable items={[...(check.findings ?? []), ...(check.observations ?? [])].sort((a,b) => a.startTick-b.startTick).map(sample => ({...sample,sourceIds:[]}))} tickRate={record.tickRate} playerNames={playerNames} />
            </details>)}
            <Flex justify="end"><Dialog.Close><Button>{t('common.close')}</Button></Dialog.Close></Flex>
          </Flex>
        </Dialog.Content>}
      </Dialog.Root> : '—'}
      </Flex>
    </Table.Cell>
  </Table.Row>;
}

function EvidenceTable({items,tickRate,playerNames}: {items: BehaviorCheck['occurrences'];tickRate:number;playerNames:Record<string,string>}) {
  const {t} = useTranslation();
  const [page,setPage] = useState(0);
  const pages = Math.ceil(items.length / 5);
  return <div style={{marginTop:8}}>
                <Table.Root size="1" variant="surface">
                  <Table.Header><Table.Row>
                    <Table.ColumnHeaderCell>{t('scoring.ticks')}</Table.ColumnHeaderCell>
                    <Table.ColumnHeaderCell>{t('scoring.target')}</Table.ColumnHeaderCell>
                    <Table.ColumnHeaderCell>{t('scoring.details')}</Table.ColumnHeaderCell>
                  </Table.Row></Table.Header>
                  <Table.Body>{items.slice(page * 5, (page + 1) * 5).map(occurrence => <Table.Row key={occurrence.id} data-occurrence-id={occurrence.id}>
                    <Table.Cell style={{whiteSpace:'nowrap'}}>
                      <Text as="p" size="2">{t('common.roundN',{n:occurrence.round})} · {clock(occurrence.startTick,tickRate)}–{clock(occurrence.endTick,tickRate)}</Text>
                      <Text as="p" size="1" color="gray">{t('scoring.ticks')}: {occurrence.startTick}–{occurrence.endTick}</Text>
                    </Table.Cell>
                    <Table.Cell>{occurrence.targetId ? playerNames[occurrence.targetId] ?? occurrence.targetId : '—'}</Table.Cell>
                    <Table.Cell style={{whiteSpace:'normal',minWidth:240}}>
                      <Measurements values={occurrence.measurements} />
                      {occurrence.sourceIds.length > 0 && <Text as="p" size="1" color="gray">{t('scoring.sourceRecords')}: {occurrence.sourceIds.join(', ')}</Text>}
                    </Table.Cell>
                  </Table.Row>)}</Table.Body>
                </Table.Root>
                {pages > 1 && <Flex justify="end" align="center" gap="2" mt="2">
                  <Button size="1" variant="soft" disabled={page === 0} onClick={() => setPage(page - 1)}>{t('scoring.previousPage')}</Button>
                  <Text size="2">{page + 1} / {pages}</Text>
                  <Button size="1" variant="soft" disabled={page + 1 === pages} onClick={() => setPage(page + 1)}>{t('scoring.nextPage')}</Button>
                </Flex>}
              </div>;
}
