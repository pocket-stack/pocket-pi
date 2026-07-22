import { Socket } from "node:net";
class TLSSocket extends Socket {
}
function connect() {
  return new TLSSocket();
}
function createSecureContext() {
  return {};
}
const rootCertificates = [];
var tls_default = { TLSSocket, connect, createSecureContext, rootCertificates };
export {
  TLSSocket,
  connect,
  createSecureContext,
  tls_default as default,
  rootCertificates
};
