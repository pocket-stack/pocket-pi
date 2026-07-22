const performance = globalThis.performance || { now: () => Date.now(), timeOrigin: 0 };
class PerformanceObserver {
  observe() {
  }
  disconnect() {
  }
}
var perf_hooks_default = { performance, PerformanceObserver };
export {
  PerformanceObserver,
  perf_hooks_default as default,
  performance
};
