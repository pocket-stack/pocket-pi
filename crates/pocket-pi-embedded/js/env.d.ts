declare global {
  var host:
    | {
        modelComplete(request: string): string;
        tool(callId: string, name: string, args: string): string;
      }
    | undefined;
  var PocketPiEmbedded:
    | {
        boot(config: string): void;
        prompt(text: string): void;
        abort(): void;
        drain(): string;
      }
    | undefined;
}

export {};
