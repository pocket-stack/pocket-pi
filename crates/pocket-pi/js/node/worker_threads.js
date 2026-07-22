class Worker {
  constructor() {
    throw new Error("worker_threads.Worker is not supported in Pocket Pi (single-threaded)");
  }
}
const isMainThread = true;
const parentPort = null;
const threadId = 0;
const workerData = null;
class MessageChannel {
  constructor() {
    this.port1 = new MessagePort();
    this.port2 = new MessagePort();
  }
}
class MessagePort {
  postMessage() {
  }
  on() {
    return this;
  }
  once() {
    return this;
  }
  close() {
  }
  ref() {
    return this;
  }
  unref() {
    return this;
  }
  start() {
  }
}
const BroadcastChannel = class BroadcastChannel2 {
  postMessage() {
  }
  close() {
  }
  on() {
    return this;
  }
};
function markAsUntransferable() {
}
function moveMessagePortToContext() {
  throw new Error("not supported");
}
function receiveMessageOnPort() {
  return void 0;
}
function setEnvironmentData() {
}
function getEnvironmentData() {
  return void 0;
}
var worker_threads_default = {
  Worker,
  isMainThread,
  parentPort,
  threadId,
  workerData,
  MessageChannel,
  MessagePort,
  BroadcastChannel,
  markAsUntransferable,
  moveMessagePortToContext,
  receiveMessageOnPort,
  setEnvironmentData,
  getEnvironmentData
};
export {
  BroadcastChannel,
  MessageChannel,
  MessagePort,
  Worker,
  worker_threads_default as default,
  getEnvironmentData,
  isMainThread,
  markAsUntransferable,
  moveMessagePortToContext,
  parentPort,
  receiveMessageOnPort,
  setEnvironmentData,
  threadId,
  workerData
};
