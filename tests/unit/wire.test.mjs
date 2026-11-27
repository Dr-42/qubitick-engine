// wire.test.mjs — what a wire does with a value, and the four behavioursthat took a bug to learn.
//
//   node --test tests/unit
//
// `js/core/wire.js` is the wirer: it subscribes a SOURCE port and forwards what arrives to one or
// more DESTINATION ports. Four of its behaviours are load-bearing and none of them is obvious from
// the call site:
//
//   · a value port delivers its current value on subscribe, so wiring is not a no-op on a settled
//     pane — and the same value twice is forwarded ONCE (`dedupe`), or every pane re-renders on
//     every unrelated change;
//   · a call port cannot be a source: there is no value to send, only something to invoke;
//   · a loop is CUT at `maxDepth` and reported on the bus, because a feedback wire with a mapping
//     never repeats a value and would otherwise recurse until the stack goes;
//   · a call destination that is not bound REJECTS (by design), and an unhandled rejection is a page
//     error — so the wirer catches it, reports `ws:wire-unbound` and lets the rest of the graph work.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createWirer } from '../../web-next/js/core/wire.js';
import { callPort, eventPort, valuePort } from '../../web-next/js/core/ports.js';

const busWith = () => {
  const seen = [];
  return { seen, emit: (name, payload) => seen.push({ name, payload }) };
};

test('a value port hands its current value to a destination when wired', () => {
  const wirer = createWirer();
  const src = valuePort(7);
  const dst = valuePort();
  wirer(src, dst);
  assert.equal(dst.get(), 7, 'wiring a settled port is not a no-op');
});

test('the same value twice is forwarded once, and a different value gets through', () => {
  const wirer = createWirer();
  const src = valuePort(1);
  const dst = valuePort();
  const seen = [];
  dst.subscribe((v) => seen.push(v));
  wirer(src, dst);
  src.set(1);
  src.set(1);
  src.set(2);
  assert.deepEqual(seen, [1, 2], 'one forward per distinct value');
});

test('map and filter decide what crosses, and dedupe applies to the MAPPED value', () => {
  const wirer = createWirer();
  const src = valuePort(2);
  const dst = valuePort();
  const seen = [];
  dst.subscribe((v) => seen.push(v));
  wirer(src, dst, { map: (v) => v * 10, filter: (v) => v % 2 === 0 });
  src.set(4);
  src.set(3); // filtered out
  src.set(4); // same mapped value as the second one → deduped
  assert.deepEqual(seen, [20, 40]);
});

test('dedupe can be turned off for a wire that must repeat', () => {
  const wirer = createWirer();
  const src = valuePort(1);
  const dst = valuePort();
  const seen = [];
  dst.subscribe((v) => seen.push(v));
  wirer(src, dst, { dedupe: false });
  src.set(1);
  assert.deepEqual(seen, [1, 1]);
});

test('one source feeds several destinations, and an event port works as a source too', () => {
  const wirer = createWirer();
  const events = eventPort();
  const a = valuePort();
  const b = valuePort();
  wirer(events, [a, b]);
  events.emit('ping');
  assert.equal(a.get(), 'ping');
  assert.equal(b.get(), 'ping');
});

test('a call port cannot be a source: there is nothing to send, only something to invoke', () => {
  const wirer = createWirer();
  const call = callPort(async (x) => x);
  const dst = valuePort();
  assert.throws(() => wirer(call, dst), TypeError);
});

test('a call destination is invoked with the payload', async () => {
  const wirer = createWirer();
  const src = valuePort(0);
  const got = [];
  const call = callPort(async (v) => { got.push(v); return v; });
  wirer(src, call);
  src.set(5);
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(got, [0, 5], 'the current value arrives on wiring, then each change');
});

test('a loop with a mapping is cut at maxDepth and reported on the bus', () => {
  const bus = busWith();
  const wirer = createWirer({ bus, maxDepth: 6 });
  const a = valuePort(0);
  const b = valuePort(-1);
  /* a → b → a, each hop incrementing: the value never repeats, so dedupe cannot stop it.
     The cycle only closes once BOTH subscribers are registered — `valuePort.subscribe` calls the
     listener before it records it — so the loop starts at the next `set`, not at wiring time. */
  wirer(a, b, { map: (v) => v + 1 });
  wirer(b, a, { map: (v) => v + 1 });
  a.set(100);
  const cut = bus.seen.filter((e) => e.name === 'ws:wire-cut');
  assert.equal(cut.length, 1, `expected exactly one cut, saw ${JSON.stringify(bus.seen)}`);
});

test('an unbound call destination is reported, not thrown, and the wire keeps working', async () => {
  const bus = busWith();
  const wirer = createWirer({ bus });
  const src = valuePort(1);
  const unbound = callPort(); // never bound
  const other = valuePort();
  wirer(src, [unbound, other]);
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(other.get(), 1, 'the other destination still received the value');
  const reported = bus.seen.filter((e) => e.name === 'ws:wire-unbound');
  assert.equal(reported.length, 1, JSON.stringify(bus.seen));
  assert.match(reported[0].payload.error, /not bound/);
});

test('unwiring stops delivery', () => {
  const wirer = createWirer();
  const src = valuePort('first');
  const dst = valuePort();
  const off = wirer(src, dst);
  off();
  /* `set` on the destination's own port does not go through the wire; assert on the source side. */
  const seen = [];
  dst.subscribe((v) => seen.push(v));
  src.set('second');
  assert.deepEqual(seen, ['first'], 'nothing crossed after the unsubscribe');
});

test('a scope collects the unsubscribe, so a closed pane leaves no live wire', () => {
  const wirer = createWirer();
  const src = valuePort(1);
  const dst = valuePort();
  const closed = [];
  const scope = { add: (off) => closed.push(off) };
  wirer(src, dst, {}, scope);
  assert.equal(closed.length, 1);
  closed.forEach((off) => off());
  const seen = [];
  dst.subscribe((v) => seen.push(v));
  src.set(2);
  assert.deepEqual(seen, [1], 'only the value from before the scope was closed');
});