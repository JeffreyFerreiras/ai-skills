import assert from 'node:assert/strict';
import {test} from 'node:test';
import {frameAt, stateAt} from './timeline.mjs';

const spec = {profile: {fps: 30}, events: [
  {id: 'send', at: 1, duration_seconds: 1, edge_id: 'request', state_after: {service: 'received'}},
  {id: 'reply', at: 2, duration_seconds: 0.5, edge_id: 'response', state_after: {client: 'done'}},
]};
test('packet moves before receipt; result changes only on completion', () => {
  assert.deepEqual(stateAt(spec, 29), {states: {}, packets: []});
  assert.equal(stateAt(spec, 45).packets[0].progress, 0.5);
  assert.deepEqual(stateAt(spec, 59).states, {});
  assert.equal(stateAt(spec, 60).states.service, 'received');
  assert.equal(stateAt(spec, 60).packets[0].id, 'reply');
  assert.equal(stateAt(spec, 75).states.client, 'done');
});
test('seeking backward produces the same state', () => {
  const before = stateAt(spec, 45);
  stateAt(spec, 75);
  assert.deepEqual(stateAt(spec, 45), before);
  assert.equal(frameAt(1.001, 30), 31);
});
