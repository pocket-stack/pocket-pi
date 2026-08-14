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
        replaceTools(definitions: string): void;
      }
    | undefined;
}

export {};
