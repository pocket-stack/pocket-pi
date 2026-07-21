export class AsyncLocalStorage { run(_s, cb) { return cb(); } getStore() { return undefined; } enterWith() {} disable() {} }
export function createHook() { return { enable() {}, disable() {} }; }
export default { AsyncLocalStorage, createHook };
