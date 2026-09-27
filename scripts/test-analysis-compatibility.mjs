import test from 'node:test';
import assert from 'node:assert/strict';
import {summarize,compare} from './check-analysis-compatibility.mjs';

const native=()=>({
  stateContract:{module:'match-state',schemaVersion:7,implementationVersion:'test'},
  source:{demoFingerprint:'sha1:synthetic',gameBuild:'1',gamePatch:'2'},
  clientSha256:'client',gameContentFingerprint:'game',
  coverage:{allAlivePawnFrames:12,pawnFrames:10,measuredFrames:8,hitboxFrames:7,
    unavailableReasons:{cached_pose_missing:2},diagnosticSamples:[{reason:'cached_pose_missing'}]},
});
const assessments=()=>[{demoFingerprint:'sha1:synthetic',rulesetVersion:'test',
  inputProvenance:{native:{gameContentFingerprint:'game'}},
  checks:[{definition:{id:'aim'},evaluatedSamples:3,state:'sufficient'}]}];

test('compatibility baseline detects source, game, body, and rule changes',()=>{
  const baseline=summarize(native(),assessments());
  assert.deepEqual(compare(summarize(native(),assessments()),baseline),[]);
  const changed=native();changed.gameContentFingerprint='next-game';
  assert.deepEqual(compare(summarize(changed,null),baseline).sort(),
    ['gameContentFingerprint','rules','rulesetVersion'].sort());
  const body=native();body.coverage.unavailableReasons.cached_pose_missing=1;
  assert.throws(()=>summarize(body,assessments()),/Unaccounted pawn frames/);
  const rules=assessments();rules[0].checks[0].evaluatedSamples=0;
  assert.deepEqual(compare(summarize(native(),rules),baseline),['rules']);
  const other=assessments();other[0].demoFingerprint='sha1:other';
  assert.throws(()=>summarize(native(),other),/another demo/);
});
