import { EventEmitter } from "node:events";
class Socket extends EventEmitter {
  connect() {
    return this;
  }
  write() {
    return true;
  }
  end() {
  }
  destroy() {
  }
  setTimeout() {
  }
  setNoDelay() {
  }
  setKeepAlive() {
  }
}
class Server extends EventEmitter {
  listen() {
    return this;
  }
  close() {
  }
}
function connect() {
  return new Socket();
}
function createConnection() {
  return new Socket();
}
function createServer() {
  return new Server();
}
function isIP(s) {
  return /^\d+\.\d+\.\d+\.\d+$/.test(s) ? 4 : 0;
}
function isIPv4(s) {
  return isIP(s) === 4;
}
function isIPv6() {
  return false;
}
var net_default = { Socket, Server, connect, createConnection, createServer, isIP, isIPv4, isIPv6 };
export {
  Server,
  Socket,
  connect,
  createConnection,
  createServer,
  net_default as default,
  isIP,
  isIPv4,
  isIPv6
};
