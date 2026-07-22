// A minimal pi extension in its normal, unmodified shape — a default-exported
// factory that receives the extension `pi` API and registers a tool and a
// lifecycle hook. Pocket Pi loads this through its OWN oxc TypeScript loader (the
// `.ts` is transpiled natively, no jiti), proving extension compatibility.

interface Pi {
  registerTool(tool: {
    name: string;
    description: string;
    parameters: unknown;
    execute: (
      toolCallId: string,
      input: Record<string, unknown>,
      signal?: unknown,
      onUpdate?: unknown,
      ctx?: unknown,
    ) => Promise<unknown>;
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
    // pi calls execute(toolCallId, input, ...); the result must carry a `content`
    // array of blocks (the shape pi converts into a tool-result message).
    execute: async (_toolCallId, input): Promise<unknown> => {
      (globalThis as Record<string, unknown>).__echoCalled = input;
      return { content: [{ type: "text", text: String(input?.text ?? "") }] };
    },
  });

  pi.on("agent_start", (): void => {
    (globalThis as Record<string, unknown>).__extAgentStartFired = true;
  });
};
