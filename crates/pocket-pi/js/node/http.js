import { EventEmitter } from "node:events";
class Agent {
  constructor(o) {
    this.options = o || {};
  }
}
class Server extends EventEmitter {
  listen() {
    return this;
  }
  close() {
  }
}
function request() {
  throw new Error("http.request not supported (use fetch)");
}
function get() {
  throw new Error("http.get not supported (use fetch)");
}
const globalAgent = new Agent();
const METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
const STATUS_CODES = {};
function createServer() {
  return new Server();
}
var http_default = { Agent, Server, request, get, globalAgent, METHODS, STATUS_CODES, createServer };
export {
  Agent,
  METHODS,
  STATUS_CODES,
  Server,
  createServer,
  http_default as default,
  get,
  globalAgent,
  request
};
