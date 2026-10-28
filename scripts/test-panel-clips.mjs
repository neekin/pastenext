import assert from 'node:assert/strict';
import { reconcileClips } from '../src/panel/reconcileClips.ts';

const clip = (id) => ({
  id, kind: 'rich_text', text: 'Original', html: '<b>Original</b>',
  imagePath: null, filePaths: ['/a.txt'], sourceApp: 'Editor', sourceAppKey: 'editor',
  byteSize: 8, note: '', createdAt: 100, lastUsedAt: 0, useCount: 0,
  boardId: null, sensitive: false, tags: [{ id: 1, name: 'work' }],
});
const previous = [clip(1), clip(2)];
assert.equal(reconcileClips(previous, structuredClone(previous)), previous, 'unchanged refresh keeps the list reference');

for (const change of [
  { text: 'Edited', html: '<i>Edited</i>' }, { note: 'New note' },
  { sensitive: true }, { useCount: 1, lastUsedAt: 200 }, { boardId: 3 },
  { tags: [{ id: 1, name: 'renamed' }] }, { tags: [] },
  { filePaths: ['/b.txt'] }, { filePaths: null }, { imagePath: '/new.png' },
]) {
  const incoming = structuredClone(previous);
  Object.assign(incoming[0], change);
  const next = reconcileClips(previous, incoming);
  assert.deepEqual(next[0], incoming[0], 'edits must reach the rendered card');
  assert.notEqual(next[0], previous[0]);
  assert.equal(next[1], previous[1], 'unrelated cards should not redraw');
}
const reordered = reconcileClips(previous, structuredClone([previous[1], previous[0]]));
assert.equal(reordered[0], previous[1]);
assert.equal(reordered[1], previous[0]);
const added = reconcileClips(previous, [clip(3), ...structuredClone(previous)]);
assert.equal(added.length, 3);
assert.equal(added[1], previous[0]);
assert.deepEqual(reconcileClips(previous, []), []);
assert.equal(reconcileClips(previous, [structuredClone(previous[1])])[0], previous[1]);
console.log('✓ panel refresh: unchanged data, edits, reorder, insert, delete, empty results');
