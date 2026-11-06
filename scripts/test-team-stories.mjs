import assert from 'node:assert/strict';
import { test } from 'node:test';
import { teamStories } from '../src/teamStories.ts';

const roster = {a:'CT',b:'CT',x:'TERRORIST',y:'TERRORIST'};
const info={tickRate:64,players:Object.keys(roster).map(steamid=>({steamid,name:steamid}))};
const round={round:1,startTick:0,freezeEndTick:100,endTick:1400,roster,winner:'TERRORIST'};
const parsed={info,rounds:[round]};
const row=(pid,x,y,z=0,hp=100)=>[pid,x,y,z,0,hp,100,0];
const frames=Array.from({length:21},(_,i)=>({t:100+i*64,p:[row(0,1200,0),row(1,1200,100),row(2,i*60,0),row(3,i*60,100)]}));
const replay={tickRate:64,frames,players:info.players,events:[]};
const kill=(tick,attacker,victim)=>({tick,attacker:{steamid:attacker,team:roster[attacker]},victim:{steamid:victim,team:roster[victim]},isFreezePeriod:false});

test('proximity or ordinary walking alone does not invent a team story',()=>{
  assert.equal(teamStories(parsed,replay,[]).length,0);
});
test('utility bursts require different teammates; trade shows the loss and payoff',()=>{
  const data={...replay,events:[{t:700,k:'flash',p:2},{t:720,k:'smoke',p:3}]};
  const stories=teamStories(parsed,data,[kill(800,'a','x'),kill(850,'y','a')]);
  const story=stories.find(s=>s.team==='B');
  assert.ok(story);
  assert.ok(story.beats.some(b=>b.kind==='utility' && b.players.length===2));
  const trade=story.beats.find(b=>b.kind==='trade');
  assert.deepEqual(trade.players,['x','y']);
  assert.ok(trade.start<800 && trade.end>850);
  const same={...data,events:data.events.map(e=>({...e,p:2}))};
  assert.ok(!teamStories(parsed,same,[]).some(s=>s.beats.some(b=>b.kind==='utility')));
});
test('team identity survives side swaps',()=>{
  const swapped={...round,round:2,startTick:1500,freezeEndTick:1600,endTick:2800,roster:{a:'TERRORIST',b:'TERRORIST',x:'CT',y:'CT'},winner:'TERRORIST'};
  const data={...replay,events:[{t:1700,k:'flash',p:0},{t:1720,k:'smoke',p:1}]};
  const stories=teamStories({...parsed,rounds:[round,swapped]},data,[]);
  assert.equal(stories[0].team,'A');
  assert.equal(stories[0].side,'TERRORIST');
  assert.equal(stories[0].won,true);
});
test('two distinct approach trajectories qualify; stacked floors do not',()=>{
  const data={...replay,frames:Array.from({length:9},(_,i)=>({t:100+i*64,p:[row(0,0,0),row(1,0,50),row(2,-700+i*110,0),row(3,0,-700+i*110)]}))};
  const combat=[kill(500,'x','a')];
  assert.ok(teamStories(parsed,data,combat).some(s=>s.beats.some(b=>b.kind==='converge')));
  const stacked={...data,frames:data.frames.map(f=>({...f,p:f.p.map(p=>p[0]===3?[...p.slice(0,3),500,...p.slice(4)]:p)}))};
  assert.ok(!teamStories(parsed,stacked,combat).some(s=>s.beats.some(b=>b.kind==='converge')));
});
