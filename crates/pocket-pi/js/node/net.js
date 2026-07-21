import { EventEmitter } from "node:events";
export class Socket extends EventEmitter { connect() { return this; } write() { return true; } end() {} destroy() {} setTimeout() {} setNoDelay() {} setKeepAlive() {} }
export class Server extends EventEmitter { listen() { return this; } close() {} }
export function connect() { return new Socket(); }
export function createConnection() { return new Socket(); }
export function createServer() { return new Server(); }
export function isIP(s) { return /^\d+\.\d+\.\d+\.\d+$/.test(s) ? 4 : 0; }
export function isIPv4(s) { return isIP(s) === 4; }
export function isIPv6() { return false; }
export default { Socket, Server, connect, createConnection, createServer, isIP, isIPv4, isIPv6 };
