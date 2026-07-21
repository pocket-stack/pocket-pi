// node:perf_hooks
export const performance = globalThis.performance || { now: () => Date.now(), timeOrigin: 0 };
export class PerformanceObserver { observe() {} disconnect() {} }
export default { performance, PerformanceObserver };
