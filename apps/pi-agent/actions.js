const SYSTEM_ROOTS = new Set([".pi-agent", ".system", "apps", "data", "system"]);

function deleteFile({ path }) {
  try {
    if (typeof path !== "string" || !path || SYSTEM_ROOTS.has(path.split("/", 1)[0])) {
      throw new Error("file is outside the user workspace");
    }
    if (JSON.parse(fs.stat(path)).kind !== "file") throw new Error("path is not a file");
    if (fs.remove(path, 0) !== 0) throw new Error(fs.lastError() || "file deletion failed");
  } finally {
    PocketPi.data.commit();
  }
  return { path };
}

PocketPi.defineActions({ deleteFile });
