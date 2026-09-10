import { draw, DEFAULT_TOGGLES } from '../src/replay/draw.ts';
import assert from 'node:assert/strict';
import test from 'node:test';
import { Replay } from '../src/replay/engine.ts';
import { FLAG } from '../src/api.ts';

const DEFUSING = 256;
function replay(frames, events) {
  return new Replay({
    schemaVersion: 3, tickRate: 64, step: 4, firstTick: frames[0].t,
    lastTick: frames.at(-1).t, players: [{ steamid: 'ct0', name: 'CT 0' }, { steamid: 'ct1', name: 'CT 1' }],
    weapons: [''], frames, events,
  }, {
    rounds: [{ round: 9, startTick: 41266, freezeEndTick: 42500, endTick: 48743, officiallyEndedTick: 49191, roster: { ct0: 'CT', ct1: 'CT' } }],
    info: { players: [] }, stats: [], roundSummaries: [],
  }, []);
}
function frame(t, active = -1, alive = true) {
  return { t, p: [0, 1].map((pid) => [pid, 0, 0, 0, 0, alive ? 100 : 0, 0, (alive ? FLAG.alive : 0) | (pid === active ? DEFUSING : 0), 0, 0]), g: [] };
}

// Round 9 transitions from the reported demo; it contains no bomb_abortdefuse events.
test('round 9 fake defuses stop at the first sampled release, including seeking', () => {
  const r = replay([
    frame(47628), frame(47632, 0), frame(47644, 0), frame(47648),
    frame(47916), frame(47920, 1), frame(47936, 1), frame(47940),
    frame(48228), frame(48232, 1), frame(48424, 1), frame(48428), frame(48744),
  ], [
    { t: 46119, k: 'plant' }, { t: 47631, k: 'defuseStart', p: 0, kit: false },
    { t: 47917, k: 'defuseStart', p: 1, kit: false }, { t: 48231, k: 'defuseStart', p: 1, kit: false },
    { t: 48743, k: 'explode' },
  ]);
  for (const tick of [47648, 47940, 48428, 48740]) assert.equal(r.stateAt(tick).bomb.defuse, undefined, `released at ${tick}`);
  for (const tick of [47631, 47644, 47920, 48232, 48424]) assert.ok(r.stateAt(tick).bomb.defuse, `active at ${tick}`);
  assert.equal(r.stateAt(47917).bomb.defuse.remaining, 10);
  assert.equal(r.stateAt(48744).bomb.state, 'exploded');
});

test('a tap shorter than one replay sample cannot leave a countdown running', () => {
  const r = replay([frame(47628), frame(47632), frame(47636)], [{ t: 46119, k: 'plant' }, { t: 47630, k: 'defuseStart', p: 0, kit: false }]);
  assert.ok(r.stateAt(47630).bomb.defuse);
  assert.equal(r.stateAt(47632).bomb.defuse, undefined);
});

test('the state belongs to the defusing player, not another CT', () => {
  const r = replay([frame(47628), frame(47632, 1)], [{ t: 46119, k: 'plant' }, { t: 47630, k: 'defuseStart', p: 0 }]);
  assert.equal(r.stateAt(47632).bomb.defuse, undefined);
});

test('normal kit and no-kit defuses keep their countdown until completion', () => {
  for (const kit of [true, false]) {
    const needs = kit ? 5 : 10, since = 47632, finish = since + needs * 64;
    const r = replay([frame(since, 0), frame(since + 64, 0), frame(finish)], [
      { t: 46119, k: 'plant' }, { t: since, k: 'defuseStart', p: 0, kit }, { t: finish, k: 'defuse', p: 0 },
    ]);
    assert.equal(r.stateAt(since + 64).bomb.defuse.remaining, needs - 1);
    assert.equal(r.stateAt(finish).bomb.state, 'defused');
    assert.equal(r.stateAt(finish).bomb.defuse, undefined);
  }
});

test('an explicit abort stops immediately even before the next player sample', () => {
  const r = replay([frame(47628, 0), frame(47632, 0)], [
    { t: 46119, k: 'plant' }, { t: 47628, k: 'defuseStart', p: 0 }, { t: 47630, k: 'defuseAbort', p: 0 },
  ]);
  assert.ok(r.stateAt(47629).bomb.defuse);
  assert.equal(r.stateAt(47630).bomb.defuse, undefined);
});

test('a dead defuser cannot keep a countdown active', () => {
  const r = replay([frame(47628, 0), frame(47632, 0, false)], [
    { t: 46119, k: 'plant' }, { t: 47628, k: 'defuseStart', p: 0 },
  ]);
  assert.equal(r.stateAt(47632).bomb.defuse, undefined);
});

test('player labels stay above every marker and focus is painted last', () => {
  const calls = [];
  const ctx = new Proxy({}, {
    get(target, key) {
      if (key in target) return target[key];
      if (key === 'measureText') return text => ({ width: text.length * 8 });
      return (...args) => calls.push({ key, args, color: target.fillStyle, stroke: target.strokeStyle });
    },
  });
  const player = { x: 100, y: 100, z: 0, yaw: 0, hp: 100, alive: true, team: 'CT' };
  const players = [{ ...player, pid: 1, name: 'Focused' }, { ...player, pid: 2, name: 'Other' }];
  const map = { posX: 0, posY: 0, scale: 1, layers: [{ altitudeMin: -100, altitudeMax: 100 }] };
  const state = { players, grenades: [], effects: [], shots: [], deaths: [] };
  draw(ctx, 500, 500, map, [], state, { ...DEFAULT_TOGGLES, sound: 'off' }, { zoom: 2, panX: 0, panY: 0 }, 1);
  const firstLabel = calls.findIndex(call => call.key === 'fillText');
  assert.ok(firstLabel > calls.findLastIndex(call => call.key === 'arc'));
  assert.deepEqual(calls.filter(call => call.key === 'fillText').map(call => call.args[0]), ['Other', 'Focused']);
  const discs = calls.filter(call => call.key === 'fillRect');
  assert.equal(discs[0].color, 'rgba(0,0,0,0.55)');
  assert.equal(discs[2].color, 'rgba(0,0,0,0.55)', 'Focused and team markers share a dark health backing');
  assert.equal(discs[3].color, '#b995ff');
  assert.deepEqual(players.map(p => p.pid), [1, 2], 'Drawing must not reorder replay state');
});

test('sampled fire cells disappear and seeking restores their recorded state', () => {
  const r = replay([frame(100), frame(104), frame(108)], []);
  r.data.schemaVersion = 5;
  r.data.frames[0].f = [[10, 20, 30], [40, 50, 30]];
  r.data.frames[1].f = [[10, 20, 30]];
  assert.deepEqual(r.stateAt(102).fireCells, [[10, 20, 30], [40, 50, 30]]);
  assert.deepEqual(r.stateAt(104).fireCells, [[10, 20, 30]]);
  assert.deepEqual(r.stateAt(108).fireCells, []);
  assert.equal(r.stateAt(100).fireCells.length, 2);
});

// Drawing coordinates use radar pixels so resize, pan and zoom preserve map positions.
test('annotations stay on their floor through zoom, pan and vertical layout', async () => {
  const { annotationPoint, drawAnnotations } = await import('../src/replay/annotations.ts');
  const { layout } = await import('../src/replay/draw.ts');
  const initial = layout(1000,600,2,{zoom:1,panX:0,panY:0});
  const [ox,oy] = initial.origins[1];
  const hit = annotationPoint(initial,ox+initial.side/2,oy+initial.side/4);
  assert.deepEqual(hit,{layer:1,point:[512,256]});
  assert.equal(annotationPoint(initial,-1,-1),undefined);
  assert.deepEqual(annotationPoint(initial,-100,-100,1).point,[0,0]);
  assert.deepEqual(annotationPoint(initial,10000,10000,1).point,[1024,1024]);
  for(const [width,height,view] of [[1000,600,{zoom:2,panX:50,panY:-30}],[400,900,{zoom:1,panX:0,panY:0}]]) {
    const lay=layout(width,height,2,view), calls=[];
    const ctx=new Proxy({}, {get:(_,key)=>(...args)=>calls.push([key,...args])});
    drawAnnotations(ctx,lay,[{tool:'arrow',color:'#fff',layer:hit.layer,points:[hit.point,[700,400]]}]);
    assert.deepEqual(calls.find(c=>c[0]==='moveTo'),['moveTo',lay.origins[1][0]+512*lay.k,lay.origins[1][1]+256*lay.k]);
    assert.equal(calls.filter(c=>c[0]==='stroke').length,2,'Dark outline and colored stroke');
    assert.ok(calls.some(c=>c[0]==='clip'),'Drawing stays inside its floor');
    assert.equal(calls.filter(c=>c[0]==='moveTo').length,3,'Arrow has two head segments');
  }
});

test('ellipse and rectangle support dragging in either direction without a diagonal', async () => {
  const { drawAnnotations } = await import('../src/replay/annotations.ts');
  const lay={side:1024,k:1,origins:[[10,20]]};
  for(const tool of ['ellipse','rectangle']) for(const points of [[[100,200],[300,400]],[[300,400],[100,200]]]) {
    const calls=[];
    const ctx=new Proxy({}, {get:(_,key)=>(...args)=>calls.push([key,...args])});
    drawAnnotations(ctx,lay,[{tool,color:'#fff',layer:0,points}]);
    if(tool==='ellipse') assert.deepEqual(calls.find(c=>c[0]==='ellipse'),['ellipse',210,320,100,100,0,0,Math.PI*2]);
    else assert.deepEqual(calls.filter(c=>c[0]==='rect').at(-1),['rect',110,220,200,200]);
    assert.ok(!calls.some(c=>c[0]==='lineTo'),'Shapes have no diagonal line');
  }
});

test('annotations sit directly above the radar and below replay effects', () => {
  const calls=[];
  const ctx=new Proxy({}, {get(target,key) {
    if(key in target) return target[key];
    return (...args)=>calls.push({key,args,stroke:target.strokeStyle});
  }});
  const map={posX:0,posY:0,scale:1,layers:[{altitudeMin:-100,altitudeMax:100}]};
  const state={tick:1,players:[],grenades:[],shots:[],deaths:[],effects:[{kind:'smoke',x:100,y:100,z:0,start:0,end:10}]};
  draw(ctx,500,500,map,[{complete:true,naturalWidth:1024}],state,DEFAULT_TOGGLES,{zoom:1,panX:0,panY:0},undefined,14,[{tool:'pen',color:'#ff70d4',layer:0,points:[[0,0],[200,200]]}]);
  const radar=calls.findIndex(c=>c.key==='drawImage');
  const ink=calls.findIndex(c=>c.key==='stroke' && c.stroke==='#ff70d4');
  const smoke=calls.findIndex(c=>c.key==='arc');
  assert.ok(radar>=0 && radar<ink && ink<smoke,'Radar → annotation → smoke');
});
