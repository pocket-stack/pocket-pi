// node:readline — stub sufficient for import; interactive input isn't used headless.
import { EventEmitter } from "node:events";
export function createInterface() {
  const rl = new EventEmitter();
  rl.question = (_q, cb) => cb && cb("");
  rl.close = () => {};
  rl.on = rl.on.bind(rl);
  return rl;
}
export default { createInterface };
