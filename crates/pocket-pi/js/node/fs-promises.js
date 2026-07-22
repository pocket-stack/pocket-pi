import fs from "node:fs";
const p = fs.promises;
const readFile = p.readFile;
const writeFile = p.writeFile;
const readdir = p.readdir;
const mkdir = p.mkdir;
const stat = p.stat;
const lstat = p.lstat;
const realpath = p.realpath;
const readlink = p.readlink;
const unlink = p.unlink;
const rm = p.rm;
const rmdir = p.rmdir;
const rename = p.rename;
const copyFile = p.copyFile;
const cp = p.cp;
const appendFile = p.appendFile;
const chmod = p.chmod;
const symlink = p.symlink;
const utimes = p.utimes;
const truncate = p.truncate;
const mkdtemp = p.mkdtemp;
const access = p.access;
const open = p.open;
var fs_promises_default = p;
export {
  access,
  appendFile,
  chmod,
  copyFile,
  cp,
  fs_promises_default as default,
  lstat,
  mkdir,
  mkdtemp,
  open,
  readFile,
  readdir,
  readlink,
  realpath,
  rename,
  rm,
  rmdir,
  stat,
  symlink,
  truncate,
  unlink,
  utimes,
  writeFile
};
