import assert from 'node:assert/strict';
import { cropFor, blurPixels } from '../src/capture/drawing.ts';
import { editHistory } from '../src/capture/history.ts';
const mark={tool:'crop',from:{x:91.5,y:82},to:{x:-10,y:12.4},color:'#fff',width:4};
assert.deepEqual(cropFor([mark],80,60),{x:0,y:12,width:80,height:48});
let history={marks:[],redo:[]};
const arrow={...mark,tool:'arrow'};
for(const item of [mark,arrow])history=editHistory(history,{type:'push',mark:item});
const before=structuredClone(history);
const undo=editHistory(history,{type:'undo'});
assert.deepEqual(history,before,'Reducer must not mutate React previous state');
assert.deepEqual(editHistory(history,{type:'undo'}),undo,'StrictMode replay must be deterministic');
assert.deepEqual(editHistory(undo,{type:'redo'}),before);
assert.equal(editHistory(editHistory(undo,{type:'push',mark}),{type:'redo'}).marks.length,2,'New edit invalidates redo');
const pixels=new Uint8ClampedArray(7*5*4);
for(let i=0;i<pixels.length;i+=4)pixels.set([20,70,130,100],i);
const original=pixels.slice();blurPixels(pixels,7,5,3);assert.deepEqual(pixels,original,'Uniform colors/alpha and border normalization must be retained');
const impulse=new Uint8ClampedArray(7*5*4);impulse[(2*7+3)*4]=255;blurPixels(impulse,7,5,1);
assert.ok(impulse[(2*7+3)*4]>0&&impulse[(2*7+3)*4]<255);
assert.equal(impulse[(2*7+2)*4],impulse[(2*7+4)*4]);
console.log('Capture geometry, immutable undo/redo and blur checks passed');

const { marksFromPin } = await import('../src/capture/pinEdits.ts');
const pinMarks = [{ x: 200, y: 150, radius: 26 }, { x: 400, y: 300, radius: 13 }];
const studioMarks = marksFromPin(pinMarks);
assert.deepEqual(studioMarks.map(m => [m.tool, m.text, m.from, m.pinRadius]), [
  ['number', '1', { x: 200, y: 150 }, 26], ['number', '2', { x: 400, y: 300 }, 13],
]);
const restoredPin = { marks: studioMarks, redo: [] };
assert.equal(editHistory(restoredPin, {type:'undo'}).marks.length, 1);
assert.deepEqual(editHistory(editHistory(restoredPin, {type:'undo'}), {type:'redo'}), restoredPin);
assert.deepEqual(pinMarks, [{ x: 200, y: 150, radius: 26 }, { x: 400, y: 300, radius: 13 }]);
console.log('✓ pin edit: original coordinates and number styles remain editable with undo/redo');
