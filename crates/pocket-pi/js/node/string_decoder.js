class StringDecoder {
  constructor(encoding) {
    this.encoding = encoding || "utf8";
    this._dec = new TextDecoder();
  }
  write(buf) {
    return this._dec.decode(buf instanceof Uint8Array ? buf : globalThis.Buffer.from(buf));
  }
  end(buf) {
    return buf ? this.write(buf) : "";
  }
}
var string_decoder_default = { StringDecoder };
export {
  StringDecoder,
  string_decoder_default as default
};
