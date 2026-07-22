import { EventEmitter } from "node:events";
function createInterface() {
  const rl = new EventEmitter();
  rl.question = (_q, cb) => cb && cb("");
  rl.close = () => {
  };
  rl.on = rl.on.bind(rl);
  return rl;
}
var readline_default = { createInterface };
export {
  createInterface,
  readline_default as default
};
