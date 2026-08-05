import { getOps } from "@pocketjs/framework";

export type ViewName = "chat" | "workspace";
export type Role = "user" | "assistant" | "system";

export interface Turn {
  id: number;
  role: Role;
  text: string;
  streaming: boolean;
}

export interface FileEntry {
  name: string;
  size: number;
  modified_unix_seconds: number;
}

export interface Snapshot {
  revision: number;
  active_view: ViewName;
  chat: { turns: Turn[]; busy: boolean };
  workspace: {
    path: string;
    entries: FileEntry[];
    open_file: { name: string; content: string } | null;
  };
  system: { backend: string; network: string; free_ram_kib: number; fps: number };
}

export type Command =
  | { type: "switch_view"; view: ViewName }
  | { type: "send_prompt"; text: string }
  | { type: "open_path"; name: string }
  | { type: "close_file" };

export interface AppService {
  poll(): Snapshot | null;
  send(command: Command): void;
}

export function connect(): AppService | null {
  const ops = getOps();
  if (!ops.svcOpen || !ops.svcPoll || !ops.svcSend || !ops.svcOpen("pocket-pi")) return null;
  const poll = ops.svcPoll.bind(ops);
  const send = ops.svcSend.bind(ops);
  return {
    poll() {
      const batch = poll();
      if (!batch) return null;
      let latest: Snapshot | null = null;
      for (const line of batch.split("\n")) {
        if (!line) continue;
        try {
          const message = JSON.parse(line);
          if (message.type === "snapshot" && message.version === 1) latest = message.snapshot;
        } catch {
          // Ignore a malformed host frame; the next snapshot is authoritative.
        }
      }
      return latest;
    },
    send(command) {
      send(JSON.stringify({ type: "command", version: 1, command }));
    },
  };
}
