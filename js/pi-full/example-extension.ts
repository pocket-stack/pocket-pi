// A minimal pi extension in its normal, unmodified shape — a default-exported
// factory that receives the extension `pi` API and registers a tool and a
// lifecycle hook. Pocket Pi loads this through its OWN oxc TypeScript loader (the
// `.ts` is transpiled natively, no jiti), proving extension compatibility.

interface Pi {
  registerTool(tool: {
    name: string;
    description: string;
    parameters: unknown;
    execute: (args: Record<string, unknown>) => Promise<unknown>;
  }): void;
  on(event: string, handler: (event: unknown) => void): void;
}

export default (pi: Pi): void => {
  pi.registerTool({
    name: "echo",
    description: "Echo the given text back to the caller.",
    parameters: {
      type: "object",
      properties: { text: { type: "string" } },
      required: ["text"],
    },
    execute: async (args): Promise<unknown> => ({ output: String(args.text ?? "") }),
  });

  pi.on("agent_start", (): void => {
    (globalThis as Record<string, unknown>).__extAgentStartFired = true;
  });
};
