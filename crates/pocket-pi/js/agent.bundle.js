(() => {
  // js/src/pi-ai-stub.js
  var EventStream = class {
    constructor(isComplete, extractResult) {
      this.isComplete = isComplete;
      this.extractResult = extractResult;
      this.queue = [];
      this.waiting = [];
      this.done = false;
      this.finalResultPromise = new Promise((resolve) => {
        this.resolveFinalResult = resolve;
      });
    }
    push(event) {
      if (this.done) return;
      if (this.isComplete(event)) {
        this.done = true;
        this.resolveFinalResult(this.extractResult(event));
      }
      const waiter = this.waiting.shift();
      if (waiter) waiter({ value: event, done: false });
      else this.queue.push(event);
    }
    end(result) {
      this.done = true;
      if (result !== void 0) this.resolveFinalResult(result);
      while (this.waiting.length > 0) this.waiting.shift()({ value: void 0, done: true });
    }
    async *[Symbol.asyncIterator]() {
      while (true) {
        if (this.queue.length > 0) {
          yield this.queue.shift();
        } else if (this.done) {
          return;
        } else {
          const result = await new Promise((resolve) => this.waiting.push(resolve));
          if (result.done) return;
          yield result.value;
        }
      }
    }
    result() {
      return this.finalResultPromise;
    }
  };
  var AssistantMessageEventStream = class extends EventStream {
    constructor() {
      super(
        (event) => event.type === "done" || event.type === "error",
        (event) => {
          if (event.type === "done") return event.message;
          if (event.type === "error") return event.error;
          throw new Error("Unexpected event type for final result");
        }
      );
    }
  };
  function streamSimple() {
    throw new Error(
      "pi-ai streamSimple is not available in Pocket Pi; pass a streamFn (see anthropic-stream.js)"
    );
  }
  function validateToolArguments(_tool, toolCall) {
    return toolCall.arguments ?? {};
  }

  // js/vendor/pi-agent-core/dist/agent-loop.js
  async function runAgentLoop(prompts, context, config, emit, signal, streamFn) {
    const newMessages = [...prompts];
    const currentContext = {
      ...context,
      messages: [...context.messages, ...prompts]
    };
    await emit({ type: "agent_start" });
    await emit({ type: "turn_start" });
    for (const prompt of prompts) {
      await emit({ type: "message_start", message: prompt });
      await emit({ type: "message_end", message: prompt });
    }
    await runLoop(currentContext, newMessages, config, signal, emit, streamFn);
    return newMessages;
  }
  async function runAgentLoopContinue(context, config, emit, signal, streamFn) {
    if (context.messages.length === 0) {
      throw new Error("Cannot continue: no messages in context");
    }
    if (context.messages[context.messages.length - 1].role === "assistant") {
      throw new Error("Cannot continue from message role: assistant");
    }
    const newMessages = [];
    const currentContext = { ...context };
    await emit({ type: "agent_start" });
    await emit({ type: "turn_start" });
    await runLoop(currentContext, newMessages, config, signal, emit, streamFn);
    return newMessages;
  }
  async function runLoop(currentContext, newMessages, config, signal, emit, streamFn) {
    let firstTurn = true;
    let pendingMessages = await config.getSteeringMessages?.() || [];
    while (true) {
      let hasMoreToolCalls = true;
      while (hasMoreToolCalls || pendingMessages.length > 0) {
        if (!firstTurn) {
          await emit({ type: "turn_start" });
        } else {
          firstTurn = false;
        }
        if (pendingMessages.length > 0) {
          for (const message2 of pendingMessages) {
            await emit({ type: "message_start", message: message2 });
            await emit({ type: "message_end", message: message2 });
            currentContext.messages.push(message2);
            newMessages.push(message2);
          }
          pendingMessages = [];
        }
        const message = await streamAssistantResponse(currentContext, config, signal, emit, streamFn);
        newMessages.push(message);
        if (message.stopReason === "error" || message.stopReason === "aborted") {
          await emit({ type: "turn_end", message, toolResults: [] });
          await emit({ type: "agent_end", messages: newMessages });
          return;
        }
        const toolCalls = message.content.filter((c) => c.type === "toolCall");
        const toolResults = [];
        hasMoreToolCalls = false;
        if (toolCalls.length > 0) {
          const executedToolBatch = await executeToolCalls(currentContext, message, config, signal, emit);
          toolResults.push(...executedToolBatch.messages);
          hasMoreToolCalls = !executedToolBatch.terminate;
          for (const result of toolResults) {
            currentContext.messages.push(result);
            newMessages.push(result);
          }
        }
        await emit({ type: "turn_end", message, toolResults });
        if (await config.shouldStopAfterTurn?.({
          message,
          toolResults,
          context: currentContext,
          newMessages
        })) {
          await emit({ type: "agent_end", messages: newMessages });
          return;
        }
        pendingMessages = await config.getSteeringMessages?.() || [];
      }
      const followUpMessages = await config.getFollowUpMessages?.() || [];
      if (followUpMessages.length > 0) {
        pendingMessages = followUpMessages;
        continue;
      }
      break;
    }
    await emit({ type: "agent_end", messages: newMessages });
  }
  async function streamAssistantResponse(context, config, signal, emit, streamFn) {
    let messages = context.messages;
    if (config.transformContext) {
      messages = await config.transformContext(messages, signal);
    }
    const llmMessages = await config.convertToLlm(messages);
    const llmContext = {
      systemPrompt: context.systemPrompt,
      messages: llmMessages,
      tools: context.tools
    };
    const streamFunction = streamFn || streamSimple;
    const resolvedApiKey = (config.getApiKey ? await config.getApiKey(config.model.provider) : void 0) || config.apiKey;
    const response = await streamFunction(config.model, llmContext, {
      ...config,
      apiKey: resolvedApiKey,
      signal
    });
    let partialMessage = null;
    let addedPartial = false;
    for await (const event of response) {
      switch (event.type) {
        case "start":
          partialMessage = event.partial;
          context.messages.push(partialMessage);
          addedPartial = true;
          await emit({ type: "message_start", message: { ...partialMessage } });
          break;
        case "text_start":
        case "text_delta":
        case "text_end":
        case "thinking_start":
        case "thinking_delta":
        case "thinking_end":
        case "toolcall_start":
        case "toolcall_delta":
        case "toolcall_end":
          if (partialMessage) {
            partialMessage = event.partial;
            context.messages[context.messages.length - 1] = partialMessage;
            await emit({
              type: "message_update",
              assistantMessageEvent: event,
              message: { ...partialMessage }
            });
          }
          break;
        case "done":
        case "error": {
          const finalMessage2 = await response.result();
          if (addedPartial) {
            context.messages[context.messages.length - 1] = finalMessage2;
          } else {
            context.messages.push(finalMessage2);
          }
          if (!addedPartial) {
            await emit({ type: "message_start", message: { ...finalMessage2 } });
          }
          await emit({ type: "message_end", message: finalMessage2 });
          return finalMessage2;
        }
      }
    }
    const finalMessage = await response.result();
    if (addedPartial) {
      context.messages[context.messages.length - 1] = finalMessage;
    } else {
      context.messages.push(finalMessage);
      await emit({ type: "message_start", message: { ...finalMessage } });
    }
    await emit({ type: "message_end", message: finalMessage });
    return finalMessage;
  }
  async function executeToolCalls(currentContext, assistantMessage, config, signal, emit) {
    const toolCalls = assistantMessage.content.filter((c) => c.type === "toolCall");
    const hasSequentialToolCall = toolCalls.some((tc) => currentContext.tools?.find((t) => t.name === tc.name)?.executionMode === "sequential");
    if (config.toolExecution === "sequential" || hasSequentialToolCall) {
      return executeToolCallsSequential(currentContext, assistantMessage, toolCalls, config, signal, emit);
    }
    return executeToolCallsParallel(currentContext, assistantMessage, toolCalls, config, signal, emit);
  }
  async function executeToolCallsSequential(currentContext, assistantMessage, toolCalls, config, signal, emit) {
    const finalizedCalls = [];
    const messages = [];
    for (const toolCall of toolCalls) {
      await emit({
        type: "tool_execution_start",
        toolCallId: toolCall.id,
        toolName: toolCall.name,
        args: toolCall.arguments
      });
      const preparation = await prepareToolCall(currentContext, assistantMessage, toolCall, config, signal);
      let finalized;
      if (preparation.kind === "immediate") {
        finalized = {
          toolCall,
          result: preparation.result,
          isError: preparation.isError
        };
      } else {
        const executed = await executePreparedToolCall(preparation, signal, emit);
        finalized = await finalizeExecutedToolCall(currentContext, assistantMessage, preparation, executed, config, signal);
      }
      await emitToolExecutionEnd(finalized, emit);
      const toolResultMessage = createToolResultMessage(finalized);
      await emitToolResultMessage(toolResultMessage, emit);
      finalizedCalls.push(finalized);
      messages.push(toolResultMessage);
    }
    return {
      messages,
      terminate: shouldTerminateToolBatch(finalizedCalls)
    };
  }
  async function executeToolCallsParallel(currentContext, assistantMessage, toolCalls, config, signal, emit) {
    const finalizedCalls = [];
    for (const toolCall of toolCalls) {
      await emit({
        type: "tool_execution_start",
        toolCallId: toolCall.id,
        toolName: toolCall.name,
        args: toolCall.arguments
      });
      const preparation = await prepareToolCall(currentContext, assistantMessage, toolCall, config, signal);
      if (preparation.kind === "immediate") {
        const finalized = {
          toolCall,
          result: preparation.result,
          isError: preparation.isError
        };
        await emitToolExecutionEnd(finalized, emit);
        finalizedCalls.push(finalized);
        continue;
      }
      finalizedCalls.push(async () => {
        const executed = await executePreparedToolCall(preparation, signal, emit);
        const finalized = await finalizeExecutedToolCall(currentContext, assistantMessage, preparation, executed, config, signal);
        await emitToolExecutionEnd(finalized, emit);
        return finalized;
      });
    }
    const orderedFinalizedCalls = await Promise.all(finalizedCalls.map((entry) => typeof entry === "function" ? entry() : Promise.resolve(entry)));
    const messages = [];
    for (const finalized of orderedFinalizedCalls) {
      const toolResultMessage = createToolResultMessage(finalized);
      await emitToolResultMessage(toolResultMessage, emit);
      messages.push(toolResultMessage);
    }
    return {
      messages,
      terminate: shouldTerminateToolBatch(orderedFinalizedCalls)
    };
  }
  function shouldTerminateToolBatch(finalizedCalls) {
    return finalizedCalls.length > 0 && finalizedCalls.every((finalized) => finalized.result.terminate === true);
  }
  function prepareToolCallArguments(tool, toolCall) {
    if (!tool.prepareArguments) {
      return toolCall;
    }
    const preparedArguments = tool.prepareArguments(toolCall.arguments);
    if (preparedArguments === toolCall.arguments) {
      return toolCall;
    }
    return {
      ...toolCall,
      arguments: preparedArguments
    };
  }
  async function prepareToolCall(currentContext, assistantMessage, toolCall, config, signal) {
    const tool = currentContext.tools?.find((t) => t.name === toolCall.name);
    if (!tool) {
      return {
        kind: "immediate",
        result: createErrorToolResult(`Tool ${toolCall.name} not found`),
        isError: true
      };
    }
    try {
      const preparedToolCall = prepareToolCallArguments(tool, toolCall);
      const validatedArgs = validateToolArguments(tool, preparedToolCall);
      if (config.beforeToolCall) {
        const beforeResult = await config.beforeToolCall({
          assistantMessage,
          toolCall,
          args: validatedArgs,
          context: currentContext
        }, signal);
        if (beforeResult?.block) {
          return {
            kind: "immediate",
            result: createErrorToolResult(beforeResult.reason || "Tool execution was blocked"),
            isError: true
          };
        }
      }
      return {
        kind: "prepared",
        toolCall,
        tool,
        args: validatedArgs
      };
    } catch (error) {
      return {
        kind: "immediate",
        result: createErrorToolResult(error instanceof Error ? error.message : String(error)),
        isError: true
      };
    }
  }
  async function executePreparedToolCall(prepared, signal, emit) {
    const updateEvents = [];
    try {
      const result = await prepared.tool.execute(prepared.toolCall.id, prepared.args, signal, (partialResult) => {
        updateEvents.push(Promise.resolve(emit({
          type: "tool_execution_update",
          toolCallId: prepared.toolCall.id,
          toolName: prepared.toolCall.name,
          args: prepared.toolCall.arguments,
          partialResult
        })));
      });
      await Promise.all(updateEvents);
      return { result, isError: false };
    } catch (error) {
      await Promise.all(updateEvents);
      return {
        result: createErrorToolResult(error instanceof Error ? error.message : String(error)),
        isError: true
      };
    }
  }
  async function finalizeExecutedToolCall(currentContext, assistantMessage, prepared, executed, config, signal) {
    let result = executed.result;
    let isError = executed.isError;
    if (config.afterToolCall) {
      try {
        const afterResult = await config.afterToolCall({
          assistantMessage,
          toolCall: prepared.toolCall,
          args: prepared.args,
          result,
          isError,
          context: currentContext
        }, signal);
        if (afterResult) {
          result = {
            content: afterResult.content ?? result.content,
            details: afterResult.details ?? result.details,
            terminate: afterResult.terminate ?? result.terminate
          };
          isError = afterResult.isError ?? isError;
        }
      } catch (error) {
        result = createErrorToolResult(error instanceof Error ? error.message : String(error));
        isError = true;
      }
    }
    return {
      toolCall: prepared.toolCall,
      result,
      isError
    };
  }
  function createErrorToolResult(message) {
    return {
      content: [{ type: "text", text: message }],
      details: {}
    };
  }
  async function emitToolExecutionEnd(finalized, emit) {
    await emit({
      type: "tool_execution_end",
      toolCallId: finalized.toolCall.id,
      toolName: finalized.toolCall.name,
      result: finalized.result,
      isError: finalized.isError
    });
  }
  function createToolResultMessage(finalized) {
    return {
      role: "toolResult",
      toolCallId: finalized.toolCall.id,
      toolName: finalized.toolCall.name,
      content: finalized.result.content,
      details: finalized.result.details,
      isError: finalized.isError,
      timestamp: Date.now()
    };
  }
  async function emitToolResultMessage(toolResultMessage, emit) {
    await emit({ type: "message_start", message: toolResultMessage });
    await emit({ type: "message_end", message: toolResultMessage });
  }

  // js/vendor/pi-agent-core/dist/agent.js
  function defaultConvertToLlm(messages) {
    return messages.filter((message) => message.role === "user" || message.role === "assistant" || message.role === "toolResult");
  }
  var EMPTY_USAGE = {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 }
  };
  var DEFAULT_MODEL = {
    id: "unknown",
    name: "unknown",
    api: "unknown",
    provider: "unknown",
    baseUrl: "",
    reasoning: false,
    input: [],
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 0,
    maxTokens: 0
  };
  function createMutableAgentState(initialState) {
    let tools = initialState?.tools?.slice() ?? [];
    let messages = initialState?.messages?.slice() ?? [];
    return {
      systemPrompt: initialState?.systemPrompt ?? "",
      model: initialState?.model ?? DEFAULT_MODEL,
      thinkingLevel: initialState?.thinkingLevel ?? "off",
      get tools() {
        return tools;
      },
      set tools(nextTools) {
        tools = nextTools.slice();
      },
      get messages() {
        return messages;
      },
      set messages(nextMessages) {
        messages = nextMessages.slice();
      },
      isStreaming: false,
      streamingMessage: void 0,
      pendingToolCalls: /* @__PURE__ */ new Set(),
      errorMessage: void 0
    };
  }
  var PendingMessageQueue = class {
    mode;
    messages = [];
    constructor(mode) {
      this.mode = mode;
    }
    enqueue(message) {
      this.messages.push(message);
    }
    hasItems() {
      return this.messages.length > 0;
    }
    drain() {
      if (this.mode === "all") {
        const drained = this.messages.slice();
        this.messages = [];
        return drained;
      }
      const first = this.messages[0];
      if (!first) {
        return [];
      }
      this.messages = this.messages.slice(1);
      return [first];
    }
    clear() {
      this.messages = [];
    }
  };
  var Agent = class {
    _state;
    listeners = /* @__PURE__ */ new Set();
    steeringQueue;
    followUpQueue;
    convertToLlm;
    transformContext;
    streamFn;
    getApiKey;
    onPayload;
    onResponse;
    beforeToolCall;
    afterToolCall;
    activeRun;
    /** Session identifier forwarded to providers for cache-aware backends. */
    sessionId;
    /** Optional per-level thinking token budgets forwarded to the stream function. */
    thinkingBudgets;
    /** Preferred transport forwarded to the stream function. */
    transport;
    /** Optional cap for provider-requested retry delays. */
    maxRetryDelayMs;
    /** Tool execution strategy for assistant messages that contain multiple tool calls. */
    toolExecution;
    constructor(options = {}) {
      this._state = createMutableAgentState(options.initialState);
      this.convertToLlm = options.convertToLlm ?? defaultConvertToLlm;
      this.transformContext = options.transformContext;
      this.streamFn = options.streamFn ?? streamSimple;
      this.getApiKey = options.getApiKey;
      this.onPayload = options.onPayload;
      this.onResponse = options.onResponse;
      this.beforeToolCall = options.beforeToolCall;
      this.afterToolCall = options.afterToolCall;
      this.steeringQueue = new PendingMessageQueue(options.steeringMode ?? "one-at-a-time");
      this.followUpQueue = new PendingMessageQueue(options.followUpMode ?? "one-at-a-time");
      this.sessionId = options.sessionId;
      this.thinkingBudgets = options.thinkingBudgets;
      this.transport = options.transport ?? "auto";
      this.maxRetryDelayMs = options.maxRetryDelayMs;
      this.toolExecution = options.toolExecution ?? "parallel";
    }
    /**
     * Subscribe to agent lifecycle events.
     *
     * Listener promises are awaited in subscription order and are included in
     * the current run's settlement. Listeners also receive the active abort
     * signal for the current run.
     *
     * `agent_end` is the final emitted event for a run, but the agent does not
     * become idle until all awaited listeners for that event have settled.
     */
    subscribe(listener) {
      this.listeners.add(listener);
      return () => this.listeners.delete(listener);
    }
    /**
     * Current agent state.
     *
     * Assigning `state.tools` or `state.messages` copies the provided top-level array.
     */
    get state() {
      return this._state;
    }
    /** Controls how queued steering messages are drained. */
    set steeringMode(mode) {
      this.steeringQueue.mode = mode;
    }
    get steeringMode() {
      return this.steeringQueue.mode;
    }
    /** Controls how queued follow-up messages are drained. */
    set followUpMode(mode) {
      this.followUpQueue.mode = mode;
    }
    get followUpMode() {
      return this.followUpQueue.mode;
    }
    /** Queue a message to be injected after the current assistant turn finishes. */
    steer(message) {
      this.steeringQueue.enqueue(message);
    }
    /** Queue a message to run only after the agent would otherwise stop. */
    followUp(message) {
      this.followUpQueue.enqueue(message);
    }
    /** Remove all queued steering messages. */
    clearSteeringQueue() {
      this.steeringQueue.clear();
    }
    /** Remove all queued follow-up messages. */
    clearFollowUpQueue() {
      this.followUpQueue.clear();
    }
    /** Remove all queued steering and follow-up messages. */
    clearAllQueues() {
      this.clearSteeringQueue();
      this.clearFollowUpQueue();
    }
    /** Returns true when either queue still contains pending messages. */
    hasQueuedMessages() {
      return this.steeringQueue.hasItems() || this.followUpQueue.hasItems();
    }
    /** Active abort signal for the current run, if any. */
    get signal() {
      return this.activeRun?.abortController.signal;
    }
    /** Abort the current run, if one is active. */
    abort() {
      this.activeRun?.abortController.abort();
    }
    /**
     * Resolve when the current run and all awaited event listeners have finished.
     *
     * This resolves after `agent_end` listeners settle.
     */
    waitForIdle() {
      return this.activeRun?.promise ?? Promise.resolve();
    }
    /** Clear transcript state, runtime state, and queued messages. */
    reset() {
      this._state.messages = [];
      this._state.isStreaming = false;
      this._state.streamingMessage = void 0;
      this._state.pendingToolCalls = /* @__PURE__ */ new Set();
      this._state.errorMessage = void 0;
      this.clearFollowUpQueue();
      this.clearSteeringQueue();
    }
    async prompt(input, images) {
      if (this.activeRun) {
        throw new Error("Agent is already processing a prompt. Use steer() or followUp() to queue messages, or wait for completion.");
      }
      const messages = this.normalizePromptInput(input, images);
      await this.runPromptMessages(messages);
    }
    /** Continue from the current transcript. The last message must be a user or tool-result message. */
    async continue() {
      if (this.activeRun) {
        throw new Error("Agent is already processing. Wait for completion before continuing.");
      }
      const lastMessage = this._state.messages[this._state.messages.length - 1];
      if (!lastMessage) {
        throw new Error("No messages to continue from");
      }
      if (lastMessage.role === "assistant") {
        const queuedSteering = this.steeringQueue.drain();
        if (queuedSteering.length > 0) {
          await this.runPromptMessages(queuedSteering, { skipInitialSteeringPoll: true });
          return;
        }
        const queuedFollowUps = this.followUpQueue.drain();
        if (queuedFollowUps.length > 0) {
          await this.runPromptMessages(queuedFollowUps);
          return;
        }
        throw new Error("Cannot continue from message role: assistant");
      }
      await this.runContinuation();
    }
    normalizePromptInput(input, images) {
      if (Array.isArray(input)) {
        return input;
      }
      if (typeof input !== "string") {
        return [input];
      }
      const content = [{ type: "text", text: input }];
      if (images && images.length > 0) {
        content.push(...images);
      }
      return [{ role: "user", content, timestamp: Date.now() }];
    }
    async runPromptMessages(messages, options = {}) {
      await this.runWithLifecycle(async (signal) => {
        await runAgentLoop(messages, this.createContextSnapshot(), this.createLoopConfig(options), (event) => this.processEvents(event), signal, this.streamFn);
      });
    }
    async runContinuation() {
      await this.runWithLifecycle(async (signal) => {
        await runAgentLoopContinue(this.createContextSnapshot(), this.createLoopConfig(), (event) => this.processEvents(event), signal, this.streamFn);
      });
    }
    createContextSnapshot() {
      return {
        systemPrompt: this._state.systemPrompt,
        messages: this._state.messages.slice(),
        tools: this._state.tools.slice()
      };
    }
    createLoopConfig(options = {}) {
      let skipInitialSteeringPoll = options.skipInitialSteeringPoll === true;
      return {
        model: this._state.model,
        reasoning: this._state.thinkingLevel === "off" ? void 0 : this._state.thinkingLevel,
        sessionId: this.sessionId,
        onPayload: this.onPayload,
        onResponse: this.onResponse,
        transport: this.transport,
        thinkingBudgets: this.thinkingBudgets,
        maxRetryDelayMs: this.maxRetryDelayMs,
        toolExecution: this.toolExecution,
        beforeToolCall: this.beforeToolCall,
        afterToolCall: this.afterToolCall,
        convertToLlm: this.convertToLlm,
        transformContext: this.transformContext,
        getApiKey: this.getApiKey,
        getSteeringMessages: async () => {
          if (skipInitialSteeringPoll) {
            skipInitialSteeringPoll = false;
            return [];
          }
          return this.steeringQueue.drain();
        },
        getFollowUpMessages: async () => this.followUpQueue.drain()
      };
    }
    async runWithLifecycle(executor) {
      if (this.activeRun) {
        throw new Error("Agent is already processing.");
      }
      const abortController = new AbortController();
      let resolvePromise = () => {
      };
      const promise = new Promise((resolve) => {
        resolvePromise = resolve;
      });
      this.activeRun = { promise, resolve: resolvePromise, abortController };
      this._state.isStreaming = true;
      this._state.streamingMessage = void 0;
      this._state.errorMessage = void 0;
      try {
        await executor(abortController.signal);
      } catch (error) {
        await this.handleRunFailure(error, abortController.signal.aborted);
      } finally {
        this.finishRun();
      }
    }
    async handleRunFailure(error, aborted) {
      const failureMessage = {
        role: "assistant",
        content: [{ type: "text", text: "" }],
        api: this._state.model.api,
        provider: this._state.model.provider,
        model: this._state.model.id,
        usage: EMPTY_USAGE,
        stopReason: aborted ? "aborted" : "error",
        errorMessage: error instanceof Error ? error.message : String(error),
        timestamp: Date.now()
      };
      this._state.messages.push(failureMessage);
      this._state.errorMessage = failureMessage.errorMessage;
      await this.processEvents({ type: "agent_end", messages: [failureMessage] });
    }
    finishRun() {
      this._state.isStreaming = false;
      this._state.streamingMessage = void 0;
      this._state.pendingToolCalls = /* @__PURE__ */ new Set();
      this.activeRun?.resolve();
      this.activeRun = void 0;
    }
    /**
     * Reduce internal state for a loop event, then await listeners.
     *
     * `agent_end` only means no further loop events will be emitted. The run is
     * considered idle later, after all awaited listeners for `agent_end` finish
     * and `finishRun()` clears runtime-owned state.
     */
    async processEvents(event) {
      switch (event.type) {
        case "message_start":
          this._state.streamingMessage = event.message;
          break;
        case "message_update":
          this._state.streamingMessage = event.message;
          break;
        case "message_end":
          this._state.streamingMessage = void 0;
          this._state.messages.push(event.message);
          break;
        case "tool_execution_start": {
          const pendingToolCalls = new Set(this._state.pendingToolCalls);
          pendingToolCalls.add(event.toolCallId);
          this._state.pendingToolCalls = pendingToolCalls;
          break;
        }
        case "tool_execution_end": {
          const pendingToolCalls = new Set(this._state.pendingToolCalls);
          pendingToolCalls.delete(event.toolCallId);
          this._state.pendingToolCalls = pendingToolCalls;
          break;
        }
        case "turn_end":
          if (event.message.role === "assistant" && event.message.errorMessage) {
            this._state.errorMessage = event.message.errorMessage;
          }
          break;
        case "agent_end":
          this._state.streamingMessage = void 0;
          break;
      }
      const signal = this.activeRun?.abortController.signal;
      if (!signal) {
        throw new Error("Agent listener invoked outside active run");
      }
      for (const listener of this.listeners) {
        await listener(event, signal);
      }
    }
  };

  // js/src/anthropic-stream.js
  var activeTurns = /* @__PURE__ */ new Map();
  function emptyUsage() {
    return {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      totalTokens: 0,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 }
    };
  }
  function toAnthropicMessages(context) {
    const messages = [];
    for (const m of context.messages) {
      if (m.role === "user") {
        messages.push({ role: "user", content: normalizeUserContent(m.content) });
      } else if (m.role === "assistant") {
        const content = [];
        for (const c of m.content) {
          if (c.type === "text") content.push({ type: "text", text: c.text });
          else if (c.type === "toolCall")
            content.push({ type: "tool_use", id: c.id, name: c.name, input: c.arguments ?? {} });
        }
        if (content.length) messages.push({ role: "assistant", content });
      } else if (m.role === "toolResult") {
        messages.push({
          role: "user",
          content: [
            {
              type: "tool_result",
              tool_use_id: m.toolCallId,
              content: normalizeToolResultContent(m.content),
              is_error: !!m.isError
            }
          ]
        });
      }
    }
    return messages;
  }
  function normalizeUserContent(content) {
    if (typeof content === "string") return [{ type: "text", text: content }];
    return content.map((c) => {
      if (c.type === "text") return { type: "text", text: c.text };
      if (c.type === "image")
        return {
          type: "image",
          source: { type: "base64", media_type: c.mimeType || "image/png", data: c.data }
        };
      return c;
    });
  }
  function normalizeToolResultContent(content) {
    if (typeof content === "string") return [{ type: "text", text: content }];
    return content.map((c) => {
      if (c.type === "text") return { type: "text", text: c.text };
      if (c.type === "image")
        return {
          type: "image",
          source: { type: "base64", media_type: c.mimeType || "image/png", data: c.data }
        };
      return { type: "text", text: JSON.stringify(c) };
    });
  }
  function toAnthropicTools(tools) {
    if (!tools || !tools.length) return void 0;
    return tools.map((t) => ({
      name: t.name,
      description: t.description || "",
      input_schema: t.parameters || { type: "object", properties: {} }
    }));
  }
  function anthropicStream(model, context, options) {
    const stream = new AssistantMessageEventStream();
    const partial = {
      role: "assistant",
      stopReason: "stop",
      content: [],
      api: model.api,
      provider: model.provider,
      model: model.id,
      usage: emptyUsage(),
      timestamp: Date.now()
    };
    const request = {
      url: (model.baseUrl || "https://api.anthropic.com") + "/v1/messages",
      apiKey: options.apiKey || model.apiKey || "",
      body: {
        model: model.id,
        max_tokens: options.maxTokens || model.maxTokens || 4096,
        system: context.systemPrompt || void 0,
        messages: toAnthropicMessages(context),
        tools: toAnthropicTools(context.tools),
        stream: true
      }
    };
    let turnId;
    try {
      turnId = host.http.start(JSON.stringify(request));
    } catch (e) {
      partial.stopReason = "error";
      partial.errorMessage = String(e && e.message ? e.message : e);
      stream.push({ type: "error", reason: "error", error: partial });
      stream.end();
      return stream;
    }
    const state = { started: false };
    activeTurns.set(turnId, {
      stream,
      partial,
      state,
      onAbort: () => {
        try {
          host.http.cancel(turnId);
        } catch {
        }
        partial.stopReason = "aborted";
        stream.push({ type: "error", reason: "aborted", error: partial });
        stream.end();
        activeTurns.delete(turnId);
      }
    });
    if (options.signal) {
      if (options.signal.aborted) {
        activeTurns.get(turnId).onAbort();
      } else {
        options.signal.addEventListener("abort", () => {
          const t = activeTurns.get(turnId);
          if (t) t.onAbort();
        });
      }
    }
    return stream;
  }
  function applyAnthropicEvent(turn, ev) {
    const { partial, stream, state } = turn;
    switch (ev.type) {
      case "message_start": {
        if (ev.message && ev.message.usage) {
          partial.usage.input = ev.message.usage.input_tokens || 0;
        }
        if (!state.started) {
          state.started = true;
          stream.push({ type: "start", partial });
        }
        return false;
      }
      case "content_block_start": {
        const i = ev.index;
        const b = ev.content_block;
        if (b.type === "text") {
          partial.content[i] = { type: "text", text: "" };
          stream.push({ type: "text_start", contentIndex: i, partial });
        } else if (b.type === "thinking") {
          partial.content[i] = { type: "thinking", thinking: "" };
          stream.push({ type: "thinking_start", contentIndex: i, partial });
        } else if (b.type === "tool_use") {
          partial.content[i] = {
            type: "toolCall",
            id: b.id,
            name: b.name,
            arguments: {},
            partialJson: ""
          };
          stream.push({ type: "toolcall_start", contentIndex: i, partial });
        }
        return false;
      }
      case "content_block_delta": {
        const i = ev.index;
        const d = ev.delta;
        const c = partial.content[i];
        if (d.type === "text_delta" && c && c.type === "text") {
          c.text += d.text;
          stream.push({ type: "text_delta", contentIndex: i, delta: d.text, partial });
        } else if (d.type === "thinking_delta" && c && c.type === "thinking") {
          c.thinking += d.thinking;
          stream.push({ type: "thinking_delta", contentIndex: i, delta: d.thinking, partial });
        } else if (d.type === "input_json_delta" && c && c.type === "toolCall") {
          c.partialJson += d.partial_json;
          c.arguments = safeParse(c.partialJson);
          partial.content[i] = { ...c };
          stream.push({ type: "toolcall_delta", contentIndex: i, delta: d.partial_json, partial });
        }
        return false;
      }
      case "content_block_stop": {
        const i = ev.index;
        const c = partial.content[i];
        if (c && c.type === "text") stream.push({ type: "text_end", contentIndex: i, content: c.text, partial });
        else if (c && c.type === "thinking")
          stream.push({ type: "thinking_end", contentIndex: i, content: c.thinking, partial });
        else if (c && c.type === "toolCall") {
          delete c.partialJson;
          stream.push({ type: "toolcall_end", contentIndex: i, toolCall: c, partial });
        }
        return false;
      }
      case "message_delta": {
        if (ev.usage) partial.usage.output = ev.usage.output_tokens || partial.usage.output;
        if (ev.delta && ev.delta.stop_reason) {
          partial.stopReason = ev.delta.stop_reason === "tool_use" ? "toolUse" : "stop";
        }
        return false;
      }
      case "message_stop": {
        partial.usage.totalTokens = partial.usage.input + partial.usage.output;
        stream.push({ type: "done", reason: partial.stopReason, message: partial });
        stream.end();
        return true;
      }
      case "error": {
        partial.stopReason = "error";
        partial.errorMessage = ev.error ? ev.error.message || String(ev.error) : "stream error";
        stream.push({ type: "error", reason: "error", error: partial });
        stream.end();
        return true;
      }
      default:
        return false;
    }
  }
  function safeParse(text) {
    try {
      return JSON.parse(text);
    } catch {
      return {};
    }
  }
  globalThis.__catpiPump = function() {
    if (activeTurns.size === 0) return;
    for (const [turnId, turn] of activeTurns) {
      let out;
      try {
        out = JSON.parse(host.http.drain(turnId));
      } catch (e) {
        turn.partial.stopReason = "error";
        turn.partial.errorMessage = String(e);
        turn.stream.push({ type: "error", reason: "error", error: turn.partial });
        turn.stream.end();
        activeTurns.delete(turnId);
        continue;
      }
      let finished = false;
      for (const line of out.lines) {
        let ev;
        try {
          ev = JSON.parse(line);
        } catch {
          continue;
        }
        if (applyAnthropicEvent(turn, ev)) {
          finished = true;
          break;
        }
      }
      if (out.error && !finished) {
        turn.partial.stopReason = "error";
        turn.partial.errorMessage = out.error;
        turn.stream.push({ type: "error", reason: "error", error: turn.partial });
        turn.stream.end();
        finished = true;
      } else if (out.done && !finished && !turn.stream.done) {
        turn.partial.stopReason = "error";
        turn.partial.errorMessage = "stream ended without message_stop";
        turn.stream.push({ type: "error", reason: "error", error: turn.partial });
        turn.stream.end();
        finished = true;
      }
      if (finished) activeTurns.delete(turnId);
    }
  };

  // js/src/scripted.js
  function makeScriptedStream(cfg) {
    const steps = cfg && cfg.steps || [{ text: "(scripted: no steps configured)" }];
    let cursor = 0;
    return function scriptedStream(model, _context, _options) {
      const stream = new AssistantMessageEventStream();
      const step = steps[Math.min(cursor, steps.length - 1)];
      cursor++;
      const partial = {
        role: "assistant",
        stopReason: "stop",
        content: [],
        api: model.api,
        provider: model.provider,
        model: model.id,
        usage: {
          input: 0,
          output: 0,
          cacheRead: 0,
          cacheWrite: 0,
          totalTokens: 0,
          cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 }
        },
        timestamp: Date.now()
      };
      Promise.resolve().then(() => {
        stream.push({ type: "start", partial });
        if (step.toolCall) {
          partial.content[0] = {
            type: "toolCall",
            id: "scripted-" + cursor,
            name: step.toolCall.name,
            arguments: step.toolCall.arguments || {},
            partialJson: JSON.stringify(step.toolCall.arguments || {})
          };
          stream.push({ type: "toolcall_start", contentIndex: 0, partial });
          const tc = partial.content[0];
          delete tc.partialJson;
          stream.push({ type: "toolcall_end", contentIndex: 0, toolCall: tc, partial });
          partial.stopReason = "toolUse";
        } else {
          partial.content[0] = { type: "text", text: "" };
          stream.push({ type: "text_start", contentIndex: 0, partial });
          const text = step.text || "";
          partial.content[0].text = text;
          stream.push({ type: "text_delta", contentIndex: 0, delta: text, partial });
          stream.push({ type: "text_end", contentIndex: 0, content: text, partial });
        }
        stream.push({ type: "done", reason: partial.stopReason, message: partial });
        stream.end();
      });
      return stream;
    };
  }

  // js/src/entry.js
  function buildModel(cfg) {
    return {
      id: cfg.model || "claude-opus-4-8",
      name: cfg.model || "claude-opus-4-8",
      api: "anthropic-messages",
      provider: "anthropic",
      baseUrl: cfg.baseUrl || "https://api.anthropic.com",
      apiKey: cfg.apiKey || "",
      reasoning: false,
      input: ["text", "image"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: cfg.contextWindow || 2e5,
      maxTokens: cfg.maxTokens || 4096
    };
  }
  function hostTool(decl) {
    return {
      name: decl.name,
      label: decl.name,
      description: decl.description || "",
      parameters: decl.parameters || { type: "object", properties: {} },
      execute: async (_toolCallId, params) => {
        const resultJson = host.tool(decl.name, JSON.stringify(params ?? {}));
        let result;
        try {
          result = JSON.parse(resultJson);
        } catch {
          result = { text: String(resultJson) };
        }
        const content = [];
        if (result.text != null) content.push({ type: "text", text: String(result.text) });
        if (result.image) content.push({ type: "image", data: result.image, mimeType: result.mimeType || "image/jpeg" });
        if (!content.length) content.push({ type: "text", text: "" });
        return { content, details: result.details ?? {}, terminate: !!result.terminate };
      }
    };
  }
  var agent = null;
  function forward(event) {
    let out = null;
    switch (event.type) {
      case "agent_start":
        out = { kind: "start" };
        break;
      case "message_update": {
        const e = event.assistantMessageEvent;
        if (e && e.type === "text_delta") out = { kind: "text", delta: e.delta };
        else if (e && e.type === "thinking_delta") out = { kind: "thinking", delta: e.delta };
        break;
      }
      case "message_end": {
        const m = event.message;
        if (m && m.role === "assistant") {
          const text = (m.content || []).filter((c) => c.type === "text").map((c) => c.text).join("");
          if (text) out = { kind: "assistant_text", text };
        }
        break;
      }
      case "tool_execution_start":
        out = { kind: "tool_start", name: event.toolName, args: event.args };
        break;
      case "tool_execution_end":
        out = { kind: "tool_end", name: event.toolName, isError: !!event.isError };
        break;
      case "agent_end":
        out = { kind: "end" };
        break;
    }
    if (out) host.emit(JSON.stringify(out));
  }
  globalThis.PocketPi = {
    boot(configJson) {
      const cfg = typeof configJson === "string" ? JSON.parse(configJson) : configJson;
      const model = buildModel(cfg);
      const tools = (cfg.tools || []).map(hostTool);
      const streamFn = cfg.scripted ? makeScriptedStream(cfg.scripted) : anthropicStream;
      agent = new Agent({
        initialState: {
          systemPrompt: cfg.systemPrompt || "",
          model,
          thinkingLevel: "off",
          tools
        },
        streamFn,
        getApiKey: () => model.apiKey,
        apiKey: model.apiKey
      });
      agent.subscribe(forward);
      host.emit(JSON.stringify({ kind: "booted" }));
      return true;
    },
    prompt(text) {
      if (!agent) throw new Error("PocketPi.boot() first");
      agent.prompt(text).catch((e) => {
        host.emit(JSON.stringify({ kind: "error", message: String(e && e.message ? e.message : e) }));
      });
      return true;
    },
    abort() {
      if (agent) agent.abort();
    },
    busy() {
      return !!(agent && agent.state && agent.state.isStreaming);
    }
  };
})();
