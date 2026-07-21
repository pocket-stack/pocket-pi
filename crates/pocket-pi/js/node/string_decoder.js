// node:string_decoder
export class StringDecoder {
  constructor(encoding) { this.encoding = encoding || "utf8"; this._dec = new TextDecoder(); }
  write(buf) { return this._dec.decode(buf instanceof Uint8Array ? buf : globalThis.Buffer.from(buf)); }
  end(buf) { return buf ? this.write(buf) : ""; }
}
export default { StringDecoder };
