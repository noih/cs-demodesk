import test from 'node:test';
import assert from 'node:assert/strict';
import {compare} from './compare-native-attachments.mjs';

test('explicit clock and attachment error decide the oracle verdict',()=>{
  const attachments={marker:{position:[2,0,0]}};
  const capture={frames:[{tick:10,players:Array.from({length:32},(_,entityId)=>({entityId,attachments}))}]};
  const transform={position:[1,0,0],rotation:[0,0,0,1],scale:1};
  const native={models:{'1':[ {hitboxes:[{bone:'root'}]} ]},frames:[{tick:9,
    players:Array.from({length:32},(_,entity)=>({entity,hitboxSet:['1',0],transforms:[transform]}))}]};
  const model={resourceId:'1',definitions:{marker:{bone:'root',offset:[1,0,0]}}};
  assert.equal(compare(capture,native,model,-1,0.01).status,'matched');
  assert.equal(compare(capture,native,model,0,0.01).status,'insufficientData');
  attachments.marker.position=[2.1,0,0];
  assert.equal(compare(capture,native,model,-1,0.01).status,'mismatch');
});
