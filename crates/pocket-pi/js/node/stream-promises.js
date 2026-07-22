function pipeline(...args) {
  if (typeof args[args.length - 1] === "function") args.pop();
  return Promise.resolve();
}
function finished(stream) {
  return new Promise((res) => {
    if (stream && stream.on) {
      stream.on("end", res);
      stream.on("finish", res);
    }
    queueMicrotask(res);
  });
}
var stream_promises_default = { pipeline, finished };
export {
  stream_promises_default as default,
  finished,
  pipeline
};
