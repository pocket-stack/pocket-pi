declare global {
  var host:
    | {
        startModel(request: string): number;
        startTool(callId: string, name: string, args: string): number;
        poll(): string;
      }
    | undefined;
  var PocketPiEmbedded:
    | {
        boot(config: string): void;
        prompt(text: string): void;
        tick(): void;
        drain(): string;
        replaceAppContext(definitions: string, installedApps: string): void;
      }
    | undefined;
}

export {};
