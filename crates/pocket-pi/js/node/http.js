// node:http — stub; Pocket Pi routes HTTP through fetch(). Imports resolve; the
// classic client APIs throw if actually used.
import { EventEmitter } from "node:events";
export class Agent { constructor(o) { this.options = o || {}; } }
export class Server extends EventEmitter { listen() { return this; } close() {} }
export function request() { throw new Error("http.request not supported (use fetch)"); }
export function get() { throw new Error("http.get not supported (use fetch)"); }
export const globalAgent = new Agent();
export const METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
export const STATUS_CODES = {};
export function createServer() { return new Server(); }
export default { Agent, Server, request, get, globalAgent, METHODS, STATUS_CODES, createServer };
