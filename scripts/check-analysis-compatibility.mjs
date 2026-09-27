import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const integer=n=>Number.isSafeInteger(n)&&n>=0;
const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'
  ?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
const sorted=value=>JSON.stringify(canonical(value));

export function summarize(native,assessments) {
  const c=native?.coverage;
  assert(c&&native.stateContract?.module==='match-state'&&integer(native.stateContract.schemaVersion),'Invalid native diagnostic report');
  assert(typeof native.source?.demoFingerprint==='string'&&typeof native.gameContentFingerprint==='string','Missing source or game fingerprint');
  for(const key of ['allAlivePawnFrames','pawnFrames','measuredFrames','hitboxFrames'])assert(integer(c[key]),`Invalid ${key}`);
  assert(c.allAlivePawnFrames>=c.pawnFrames&&c.pawnFrames>=c.measuredFrames&&c.measuredFrames>=c.hitboxFrames,'Invalid body coverage');
  const reasons=c.unavailableReasons??{};
  assert(Object.values(reasons).every(integer),'Invalid failure counts');
  assert(c.measuredFrames+Object.values(reasons).reduce((sum,n)=>sum+n,0)===c.pawnFrames,'Unaccounted pawn frames');
  const samples=c.diagnosticSamples??[];
  for(const reason of Object.keys(reasons))assert(samples.filter(s=>s.reason===reason).length<=3,'Unbounded diagnostic samples');
  let rules=null,rulesetVersion=null;
  if(assessments!==null) {
    assert(Array.isArray(assessments)&&assessments.length>0,'Missing rule assessments');
    rules={};
    for(const record of assessments) {
      assert(record.demoFingerprint===native.source.demoFingerprint,'Assessment belongs to another demo');
      assert(record.inputProvenance?.native?.gameContentFingerprint===native.gameContentFingerprint,'Assessment used other game content');
      rulesetVersion??=record.rulesetVersion;
      assert(record.rulesetVersion===rulesetVersion,'Mixed ruleset versions');
      for(const check of record.checks??[]) {
        const id=check.definition?.id;
        assert(typeof id==='string'&&integer(check.evaluatedSamples),'Invalid rule coverage');
        const count=rules[id]??={evaluatedSamples:0,states:{}};
        count.evaluatedSamples+=check.evaluatedSamples;
        const state=String(check.state);
        count.states[state]=(count.states[state]??0)+1;
      }
    }
  }
  return {
    sourceFingerprint:native.source.demoFingerprint,
    gameBuild:native.source.gameBuild,
    gamePatch:native.source.gamePatch,
    stateContract:native.stateContract,
    clientSha256:native.clientSha256,
    gameContentFingerprint:native.gameContentFingerprint,
    coverage:{allAlivePawnFrames:c.allAlivePawnFrames,pawnFrames:c.pawnFrames,
      measuredFrames:c.measuredFrames,hitboxFrames:c.hitboxFrames,unavailableReasons:reasons},
    rulesetVersion,rules,
  };
}

export function compare(current,baseline) {
  return Object.keys(current).filter(key=>sorted(current[key])!==sorted(baseline?.[key]));
}

if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const args=process.argv.slice(2),report=args.shift();
  assert(report,'Usage: node scripts/check-analysis-compatibility.mjs REPORT.json [--assessments LATEST.json] [--baseline BASELINE.json | --accept NEW_BASELINE.json]');
  let assessmentFile=null,baselineFile=null,acceptFile=null;
  while(args.length) {
    const flag=args.shift(),file=args.shift();
    assert(file&&['--assessments','--baseline','--accept'].includes(flag),`Invalid option ${flag}`);
    if(flag==='--assessments')assessmentFile=file;
    if(flag==='--baseline')baselineFile=file;
    if(flag==='--accept')acceptFile=file;
  }
  assert(!(baselineFile&&acceptFile),'Choose baseline comparison or explicit acceptance');
  const native=JSON.parse(await readFile(report,'utf8'));
  const assessments=assessmentFile?JSON.parse(await readFile(assessmentFile,'utf8')):null;
  const current=summarize(native,assessments);
  if(acceptFile) {
    assert(assessments!==null,'Rule assessments required to accept a baseline');
    await writeFile(acceptFile,JSON.stringify(current,null,2)+'\n',{flag:'wx'});
    console.log(JSON.stringify({status:'accepted',baseline:acceptFile}));
  } else if(baselineFile) {
    const differences=compare(current,JSON.parse(await readFile(baselineFile,'utf8')));
    console.log(JSON.stringify({status:differences.length?'changed':'matched',differences}));
    if(differences.length)process.exitCode=1;
  } else {
    console.log(JSON.stringify({status:'unverified',coverage:current.coverage,ruleCoverage:current.rules}));
    process.exitCode=2;
  }
}
