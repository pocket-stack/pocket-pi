import { Socket } from "node:net";
export class TLSSocket extends Socket {}
export function connect() { return new TLSSocket(); }
export function createSecureContext() { return {}; }
export const rootCertificates = [];
export default { TLSSocket, connect, createSecureContext, rootCertificates };
