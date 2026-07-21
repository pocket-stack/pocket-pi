function assert(v, msg) { if (!v) throw new Error(msg || "Assertion failed"); }
assert.ok = assert;
assert.equal = (a, b, m) => { if (a != b) throw new Error(m || a + " != " + b); };
assert.strictEqual = (a, b, m) => { if (a !== b) throw new Error(m || a + " !== " + b); };
assert.deepEqual = (a, b, m) => { if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(m || "deepEqual failed"); };
assert.deepStrictEqual = assert.deepEqual;
assert.notEqual = (a, b, m) => { if (a == b) throw new Error(m || "notEqual failed"); };
assert.throws = (fn, m) => { try { fn(); } catch { return; } throw new Error(m || "expected throw"); };
assert.fail = (m) => { throw new Error(m || "fail"); };
export default assert;
export { assert };
