// node:stream/promises
export function pipeline(...args) {
  if (typeof args[args.length - 1] === "function") args.pop();
  return Promise.resolve();
}
export function finished(stream) {
  return new Promise((res) => {
    if (stream && stream.on) { stream.on("end", res); stream.on("finish", res); }
    queueMicrotask(res);
  });
}
export default { pipeline, finished };
