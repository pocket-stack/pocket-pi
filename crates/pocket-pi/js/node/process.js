const p = globalThis.process;
const env = p.env;
const platform = p.platform;
const argv = p.argv;
const version = p.version;
const versions = p.versions;
const cwd = p.cwd;
const nextTick = p.nextTick;
const exit = p.exit;
var process_default = p;
export {
  argv,
  cwd,
  process_default as default,
  env,
  exit,
  nextTick,
  platform,
  version,
  versions
};
