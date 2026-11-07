import assert from 'node:assert/strict';
import { imagePoint } from '../src/pins/annotations.ts';

// Landscape image centered in a taller pin: toolbar/padding must never receive marks.
const box = { width: 400, height: 400 };
const image = { width: 1600, height: 800 };
assert.equal(imagePoint(200, 50, box, image), null);
assert.equal(imagePoint(200, 350, box, image), null);
const point = imagePoint(200, 200, box, image);
assert.equal(point.x, 800);
assert.equal(point.y, 400);
// Zoom does not move the same source-image location.
const zoomed = imagePoint(400, 400, { width: 800, height: 800 }, image);
assert.equal(zoomed.x, point.x);
assert.equal(zoomed.y, point.y);
assert.equal(imagePoint(0, 0, { width: 0, height: 0 }, image), null);
const edge = imagePoint(0, 100, box, image);
assert.equal(edge.x, edge.radius, 'edge marks remain inside the source image');
assert.equal(edge.y, edge.radius);
const portrait = imagePoint(150, 200, { width: 300, height: 400 }, { width: 600, height: 1200 });
assert.equal(portrait.x, 300);
assert.equal(portrait.y, 600);
console.log('✓ number mark coordinates: letterboxing, zoom, empty bounds, edge clamping, portrait images');
