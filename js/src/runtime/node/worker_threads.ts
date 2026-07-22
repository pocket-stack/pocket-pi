// node:worker_threads — stub. Pocket Pi is single-threaded (one QuickJS realm),
// so there are no real workers; these exist so imports resolve. `Worker` throws
// if actually constructed.
export class Worker {
  constructor() {
    throw new Error("worker_threads.Worker is not supported in Pocket Pi (single-threaded)");
  }
}
export const isMainThread = true;
export const parentPort = null;
export const threadId = 0;
export const workerData = null;
export class MessageChannel {
  constructor() { this.port1 = new MessagePort(); this.port2 = new MessagePort(); }
}
export class MessagePort {
  postMessage() {}
  on() { return this; }
  once() { return this; }
  close() {}
  ref() { return this; }
  unref() { return this; }
  start() {}
}
export const BroadcastChannel = class BroadcastChannel {
  postMessage() {}
  close() {}
  on() { return this; }
};
export function markAsUntransferable() {}
export function moveMessagePortToContext() { throw new Error("not supported"); }
export function receiveMessageOnPort() { return undefined; }
export function setEnvironmentData() {}
export function getEnvironmentData() { return undefined; }
export default {
  Worker, isMainThread, parentPort, threadId, workerData,
  MessageChannel, MessagePort, BroadcastChannel,
  markAsUntransferable, moveMessagePortToContext, receiveMessageOnPort,
  setEnvironmentData, getEnvironmentData,
};
