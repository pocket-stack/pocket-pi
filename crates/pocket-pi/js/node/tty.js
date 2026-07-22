function isatty() {
  return false;
}
class ReadStream {
}
class WriteStream {
}
var tty_default = { isatty, ReadStream, WriteStream };
export {
  ReadStream,
  WriteStream,
  tty_default as default,
  isatty
};
