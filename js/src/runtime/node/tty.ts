// node:tty — stub (Pocket Pi runs headless).
export function isatty() { return false; }
export class ReadStream {}
export class WriteStream {}
export default { isatty, ReadStream, WriteStream };
