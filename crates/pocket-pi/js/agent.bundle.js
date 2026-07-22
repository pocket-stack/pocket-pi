(() => {
  var __create = Object.create;
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __getProtoOf = Object.getPrototypeOf;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __commonJS = (cb, mod) => function __require() {
    return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;
  };
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
    // If the importer is in node compatibility mode or this is not an ESM
    // file that has been converted to a CommonJS file using a Babel-
    // compatible transform (i.e. "__esModule" has not been set), then set
    // "default" to the CommonJS "module.exports" for node compatibility.
    isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
    mod
  ));

  // node_modules/@earendil-works/pi-agent-core/node_modules/ignore/index.js
  var require_ignore = __commonJS({
    "node_modules/@earendil-works/pi-agent-core/node_modules/ignore/index.js"(exports, module) {
      function makeArray(subject) {
        return Array.isArray(subject) ? subject : [subject];
      }
      var UNDEFINED = void 0;
      var EMPTY = "";
      var SPACE = " ";
      var ESCAPE = "\\";
      var REGEX_TEST_BLANK_LINE = /^\s+$/;
      var REGEX_INVALID_TRAILING_BACKSLASH = /(?:[^\\]|^)\\$/;
      var REGEX_REPLACE_LEADING_EXCAPED_EXCLAMATION = /^\\!/;
      var REGEX_REPLACE_LEADING_EXCAPED_HASH = /^\\#/;
      var REGEX_SPLITALL_CRLF = /\r?\n/g;
      var REGEX_TEST_INVALID_PATH = /^\.{0,2}\/|^\.{1,2}$/;
      var REGEX_TEST_TRAILING_SLASH = /\/$/;
      var SLASH = "/";
      var TMP_KEY_IGNORE = "node-ignore";
      if (typeof Symbol !== "undefined") {
        TMP_KEY_IGNORE = Symbol.for("node-ignore");
      }
      var KEY_IGNORE = TMP_KEY_IGNORE;
      var define = (object, key, value) => {
        Object.defineProperty(object, key, { value });
        return value;
      };
      var REGEX_REGEXP_RANGE = /([0-z])-([0-z])/g;
      var RETURN_FALSE = () => false;
      var sanitizeRange = (range) => range.replace(
        REGEX_REGEXP_RANGE,
        (match, from, to) => from.charCodeAt(0) <= to.charCodeAt(0) ? match : EMPTY
      );
      var cleanRangeBackSlash = (slashes) => {
        const { length } = slashes;
        return slashes.slice(0, length - length % 2);
      };
      var REPLACERS = [
        [
          // Remove BOM
          // TODO:
          // Other similar zero-width characters?
          /^\uFEFF/,
          () => EMPTY
        ],
        // > Trailing spaces are ignored unless they are quoted with backslash ("\")
        [
          // (a\ ) -> (a )
          // (a  ) -> (a)
          // (a ) -> (a)
          // (a \ ) -> (a  )
          /((?:\\\\)*?)(\\?\s+)$/,
          (_, m1, m2) => m1 + (m2.indexOf("\\") === 0 ? SPACE : EMPTY)
        ],
        // Replace (\ ) with ' '
        // (\ ) -> ' '
        // (\\ ) -> '\\ '
        // (\\\ ) -> '\\ '
        [
          /(\\+?)\s/g,
          (_, m1) => {
            const { length } = m1;
            return m1.slice(0, length - length % 2) + SPACE;
          }
        ],
        // Escape metacharacters
        // which is written down by users but means special for regular expressions.
        // > There are 12 characters with special meanings:
        // > - the backslash \,
        // > - the caret ^,
        // > - the dollar sign $,
        // > - the period or dot .,
        // > - the vertical bar or pipe symbol |,
        // > - the question mark ?,
        // > - the asterisk or star *,
        // > - the plus sign +,
        // > - the opening parenthesis (,
        // > - the closing parenthesis ),
        // > - and the opening square bracket [,
        // > - the opening curly brace {,
        // > These special characters are often called "metacharacters".
        [
          /[\\$.|*+(){^]/g,
          (match) => `\\${match}`
        ],
        [
          // > a question mark (?) matches a single character
          /(?!\\)\?/g,
          () => "[^/]"
        ],
        // leading slash
        [
          // > A leading slash matches the beginning of the pathname.
          // > For example, "/*.c" matches "cat-file.c" but not "mozilla-sha1/sha1.c".
          // A leading slash matches the beginning of the pathname
          /^\//,
          () => "^"
        ],
        // replace special metacharacter slash after the leading slash
        [
          /\//g,
          () => "\\/"
        ],
        [
          // > A leading "**" followed by a slash means match in all directories.
          // > For example, "**/foo" matches file or directory "foo" anywhere,
          // > the same as pattern "foo".
          // > "**/foo/bar" matches file or directory "bar" anywhere that is directly
          // >   under directory "foo".
          // Notice that the '*'s have been replaced as '\\*'
          /^\^*\\\*\\\*\\\//,
          // '**/foo' <-> 'foo'
          () => "^(?:.*\\/)?"
        ],
        // starting
        [
          // there will be no leading '/'
          //   (which has been replaced by section "leading slash")
          // If starts with '**', adding a '^' to the regular expression also works
          /^(?=[^^])/,
          function startingReplacer() {
            return !/\/(?!$)/.test(this) ? "(?:^|\\/)" : "^";
          }
        ],
        // two globstars
        [
          // Use lookahead assertions so that we could match more than one `'/**'`
          /\\\/\\\*\\\*(?=\\\/|$)/g,
          // Zero, one or several directories
          // should not use '*', or it will be replaced by the next replacer
          // Check if it is not the last `'/**'`
          (_, index, str) => index + 6 < str.length ? "(?:\\/[^\\/]+)*" : "\\/.+"
        ],
        // normal intermediate wildcards
        [
          // Never replace escaped '*'
          // ignore rule '\*' will match the path '*'
          // 'abc.*/' -> go
          // 'abc.*'  -> skip this rule,
          //    coz trailing single wildcard will be handed by [trailing wildcard]
          /(^|[^\\]+)(\\\*)+(?=.+)/g,
          // '*.js' matches '.js'
          // '*.js' doesn't match 'abc'
          (_, p1, p2) => {
            const unescaped = p2.replace(/\\\*/g, "[^\\/]*");
            return p1 + unescaped;
          }
        ],
        [
          // unescape, revert step 3 except for back slash
          // For example, if a user escape a '\\*',
          // after step 3, the result will be '\\\\\\*'
          /\\\\\\(?=[$.|*+(){^])/g,
          () => ESCAPE
        ],
        [
          // '\\\\' -> '\\'
          /\\\\/g,
          () => ESCAPE
        ],
        [
          // > The range notation, e.g. [a-zA-Z],
          // > can be used to match one of the characters in a range.
          // `\` is escaped by step 3
          /(\\)?\[([^\]/]*?)(\\*)($|\])/g,
          (match, leadEscape, range, endEscape, close) => leadEscape === ESCAPE ? `\\[${range}${cleanRangeBackSlash(endEscape)}${close}` : close === "]" ? endEscape.length % 2 === 0 ? `[${sanitizeRange(range)}${endEscape}]` : "[]" : "[]"
        ],
        // ending
        [
          // 'js' will not match 'js.'
          // 'ab' will not match 'abc'
          /(?:[^*])$/,
          // WTF!
          // https://git-scm.com/docs/gitignore
          // changes in [2.22.1](https://git-scm.com/docs/gitignore/2.22.1)
          // which re-fixes #24, #38
          // > If there is a separator at the end of the pattern then the pattern
          // > will only match directories, otherwise the pattern can match both
          // > files and directories.
          // 'js*' will not match 'a.js'
          // 'js/' will not match 'a.js'
          // 'js' will match 'a.js' and 'a.js/'
          (match) => /\/$/.test(match) ? `${match}$` : `${match}(?=$|\\/$)`
        ]
      ];
      var REGEX_REPLACE_TRAILING_WILDCARD = /(^|\\\/)?\\\*$/;
      var MODE_IGNORE = "regex";
      var MODE_CHECK_IGNORE = "checkRegex";
      var UNDERSCORE = "_";
      var TRAILING_WILD_CARD_REPLACERS = {
        [MODE_IGNORE](_, p1) {
          const prefix = p1 ? `${p1}[^/]+` : "[^/]*";
          return `${prefix}(?=$|\\/$)`;
        },
        [MODE_CHECK_IGNORE](_, p1) {
          const prefix = p1 ? `${p1}[^/]*` : "[^/]*";
          return `${prefix}(?=$|\\/$)`;
        }
      };
      var makeRegexPrefix = (pattern) => REPLACERS.reduce(
        (prev, [matcher, replacer]) => prev.replace(matcher, replacer.bind(pattern)),
        pattern
      );
      var isString = (subject) => typeof subject === "string";
      var checkPattern = (pattern) => pattern && isString(pattern) && !REGEX_TEST_BLANK_LINE.test(pattern) && !REGEX_INVALID_TRAILING_BACKSLASH.test(pattern) && pattern.indexOf("#") !== 0;
      var splitPattern = (pattern) => pattern.split(REGEX_SPLITALL_CRLF).filter(Boolean);
      var IgnoreRule = class {
        constructor(pattern, mark, body, ignoreCase, negative, prefix) {
          this.pattern = pattern;
          this.mark = mark;
          this.negative = negative;
          define(this, "body", body);
          define(this, "ignoreCase", ignoreCase);
          define(this, "regexPrefix", prefix);
        }
        get regex() {
          const key = UNDERSCORE + MODE_IGNORE;
          if (this[key]) {
            return this[key];
          }
          return this._make(MODE_IGNORE, key);
        }
        get checkRegex() {
          const key = UNDERSCORE + MODE_CHECK_IGNORE;
          if (this[key]) {
            return this[key];
          }
          return this._make(MODE_CHECK_IGNORE, key);
        }
        _make(mode, key) {
          const str = this.regexPrefix.replace(
            REGEX_REPLACE_TRAILING_WILDCARD,
            // It does not need to bind pattern
            TRAILING_WILD_CARD_REPLACERS[mode]
          );
          const regex = this.ignoreCase ? new RegExp(str, "i") : new RegExp(str);
          return define(this, key, regex);
        }
      };
      var createRule = ({
        pattern,
        mark
      }, ignoreCase) => {
        let negative = false;
        let body = pattern;
        if (body.indexOf("!") === 0) {
          negative = true;
          body = body.substr(1);
        }
        body = body.replace(REGEX_REPLACE_LEADING_EXCAPED_EXCLAMATION, "!").replace(REGEX_REPLACE_LEADING_EXCAPED_HASH, "#");
        const regexPrefix = makeRegexPrefix(body);
        return new IgnoreRule(
          pattern,
          mark,
          body,
          ignoreCase,
          negative,
          regexPrefix
        );
      };
      var RuleManager = class {
        constructor(ignoreCase) {
          this._ignoreCase = ignoreCase;
          this._rules = [];
        }
        _add(pattern) {
          if (pattern && pattern[KEY_IGNORE]) {
            this._rules = this._rules.concat(pattern._rules._rules);
            this._added = true;
            return;
          }
          if (isString(pattern)) {
            pattern = {
              pattern
            };
          }
          if (checkPattern(pattern.pattern)) {
            const rule = createRule(pattern, this._ignoreCase);
            this._added = true;
            this._rules.push(rule);
          }
        }
        // @param {Array<string> | string | Ignore} pattern
        add(pattern) {
          this._added = false;
          makeArray(
            isString(pattern) ? splitPattern(pattern) : pattern
          ).forEach(this._add, this);
          return this._added;
        }
        // Test one single path without recursively checking parent directories
        //
        // - checkUnignored `boolean` whether should check if the path is unignored,
        //   setting `checkUnignored` to `false` could reduce additional
        //   path matching.
        // - check `string` either `MODE_IGNORE` or `MODE_CHECK_IGNORE`
        // @returns {TestResult} true if a file is ignored
        test(path, checkUnignored, mode) {
          let ignored = false;
          let unignored = false;
          let matchedRule;
          this._rules.forEach((rule) => {
            const { negative } = rule;
            if (unignored === negative && ignored !== unignored || negative && !ignored && !unignored && !checkUnignored) {
              return;
            }
            const matched = rule[mode].test(path);
            if (!matched) {
              return;
            }
            ignored = !negative;
            unignored = negative;
            matchedRule = negative ? UNDEFINED : rule;
          });
          const ret = {
            ignored,
            unignored
          };
          if (matchedRule) {
            ret.rule = matchedRule;
          }
          return ret;
        }
      };
      var throwError = (message, Ctor) => {
        throw new Ctor(message);
      };
      var checkPath = (path, originalPath, doThrow) => {
        if (!isString(path)) {
          return doThrow(
            `path must be a string, but got \`${originalPath}\``,
            TypeError
          );
        }
        if (!path) {
          return doThrow(`path must not be empty`, TypeError);
        }
        if (checkPath.isNotRelative(path)) {
          const r = "`path.relative()`d";
          return doThrow(
            `path should be a ${r} string, but got "${originalPath}"`,
            RangeError
          );
        }
        return true;
      };
      var isNotRelative = (path) => REGEX_TEST_INVALID_PATH.test(path);
      checkPath.isNotRelative = isNotRelative;
      checkPath.convert = (p) => p;
      var Ignore = class {
        constructor({
          ignorecase = true,
          ignoreCase = ignorecase,
          allowRelativePaths = false
        } = {}) {
          define(this, KEY_IGNORE, true);
          this._rules = new RuleManager(ignoreCase);
          this._strictPathCheck = !allowRelativePaths;
          this._initCache();
        }
        _initCache() {
          this._ignoreCache = /* @__PURE__ */ Object.create(null);
          this._testCache = /* @__PURE__ */ Object.create(null);
        }
        add(pattern) {
          if (this._rules.add(pattern)) {
            this._initCache();
          }
          return this;
        }
        // legacy
        addPattern(pattern) {
          return this.add(pattern);
        }
        // @returns {TestResult}
        _test(originalPath, cache, checkUnignored, slices) {
          const path = originalPath && checkPath.convert(originalPath);
          checkPath(
            path,
            originalPath,
            this._strictPathCheck ? throwError : RETURN_FALSE
          );
          return this._t(path, cache, checkUnignored, slices);
        }
        checkIgnore(path) {
          if (!REGEX_TEST_TRAILING_SLASH.test(path)) {
            return this.test(path);
          }
          const slices = path.split(SLASH).filter(Boolean);
          slices.pop();
          if (slices.length) {
            const parent = this._t(
              slices.join(SLASH) + SLASH,
              this._testCache,
              true,
              slices
            );
            if (parent.ignored) {
              return parent;
            }
          }
          return this._rules.test(path, false, MODE_CHECK_IGNORE);
        }
        _t(path, cache, checkUnignored, slices) {
          if (path in cache) {
            return cache[path];
          }
          if (!slices) {
            slices = path.split(SLASH).filter(Boolean);
          }
          slices.pop();
          if (!slices.length) {
            return cache[path] = this._rules.test(path, checkUnignored, MODE_IGNORE);
          }
          const parent = this._t(
            slices.join(SLASH) + SLASH,
            cache,
            checkUnignored,
            slices
          );
          return cache[path] = parent.ignored ? parent : this._rules.test(path, checkUnignored, MODE_IGNORE);
        }
        ignores(path) {
          return this._test(path, this._ignoreCache, false).ignored;
        }
        createFilter() {
          return (path) => !this.ignores(path);
        }
        filter(paths) {
          return makeArray(paths).filter(this.createFilter());
        }
        // @returns {TestResult}
        test(path) {
          return this._test(path, this._testCache, true);
        }
      };
      var factory = (options) => new Ignore(options);
      var isPathValid = (path) => checkPath(path && checkPath.convert(path), path, RETURN_FALSE);
      var setupWindows = () => {
        const makePosix = (str) => /^\\\\\?\\/.test(str) || /["<>|\u0000-\u001F]+/u.test(str) ? str : str.replace(/\\/g, "/");
        checkPath.convert = makePosix;
        const REGEX_TEST_WINDOWS_PATH_ABSOLUTE = /^[a-z]:\//i;
        checkPath.isNotRelative = (path) => REGEX_TEST_WINDOWS_PATH_ABSOLUTE.test(path) || isNotRelative(path);
      };
      if (
        // Detect `process` so that it can run in browsers.
        typeof process !== "undefined" && process.platform === "win32"
      ) {
        setupWindows();
      }
      module.exports = factory;
      factory.default = factory;
      module.exports.isPathValid = isPathValid;
      define(module.exports, Symbol.for("setupWindows"), setupWindows);
    }
  });

  // src/trimmed/pi-ai-stub.ts
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
  function validateToolArguments(_tool, toolCall) {
    return toolCall.arguments ?? {};
  }

  // node_modules/@earendil-works/pi-agent-core/dist/stream-fn.js
  var defaultStreamFn;
  function getDefaultStreamFn() {
    if (!defaultStreamFn) {
      throw new Error("No default stream function configured. Pass streamFn explicitly or call setDefaultStreamFn().");
    }
    return defaultStreamFn;
  }

  // node_modules/@earendil-works/pi-agent-core/dist/agent-loop.js
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
    await runLoop(currentContext, newMessages, config, signal, emit, streamFn ?? getDefaultStreamFn());
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
    await runLoop(currentContext, newMessages, config, signal, emit, streamFn ?? getDefaultStreamFn());
    return newMessages;
  }
  async function runLoop(initialContext, newMessages, initialConfig, signal, emit, streamFunction) {
    let currentContext = initialContext;
    let config = initialConfig;
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
        const message = await streamAssistantResponse(currentContext, config, signal, emit, streamFunction);
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
          const executedToolBatch = message.stopReason === "length" ? await failToolCallsFromTruncatedMessage(toolCalls, emit) : await executeToolCalls(currentContext, message, config, signal, emit);
          toolResults.push(...executedToolBatch.messages);
          hasMoreToolCalls = !executedToolBatch.terminate;
          for (const result of toolResults) {
            currentContext.messages.push(result);
            newMessages.push(result);
          }
        }
        await emit({ type: "turn_end", message, toolResults });
        const nextTurnContext = {
          message,
          toolResults,
          context: currentContext,
          newMessages
        };
        const nextTurnSnapshot = await config.prepareNextTurn?.(nextTurnContext);
        if (nextTurnSnapshot) {
          currentContext = nextTurnSnapshot.context ?? currentContext;
          config = {
            ...config,
            model: nextTurnSnapshot.model ?? config.model,
            reasoning: nextTurnSnapshot.thinkingLevel === void 0 ? config.reasoning : nextTurnSnapshot.thinkingLevel === "off" ? void 0 : nextTurnSnapshot.thinkingLevel
          };
        }
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
  async function streamAssistantResponse(context, config, signal, emit, streamFunction) {
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
  async function failToolCallsFromTruncatedMessage(toolCalls, emit) {
    const messages = [];
    for (const toolCall of toolCalls) {
      await emit({
        type: "tool_execution_start",
        toolCallId: toolCall.id,
        toolName: toolCall.name,
        args: toolCall.arguments
      });
      const finalized = {
        toolCall,
        result: createErrorToolResult(`Tool call "${toolCall.name}" was not executed: the response hit the output token limit, so its arguments may be truncated. Re-issue the tool call with complete arguments.`),
        isError: true
      };
      await emitToolExecutionEnd(finalized, emit);
      const toolResultMessage = createToolResultMessage(finalized);
      await emitToolResultMessage(toolResultMessage, emit);
      messages.push(toolResultMessage);
    }
    return { messages, terminate: false };
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
      if (signal?.aborted) {
        break;
      }
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
        if (signal?.aborted) {
          break;
        }
        continue;
      }
      finalizedCalls.push(async () => {
        const executed = await executePreparedToolCall(preparation, signal, emit);
        const finalized = await finalizeExecutedToolCall(currentContext, assistantMessage, preparation, executed, config, signal);
        await emitToolExecutionEnd(finalized, emit);
        return finalized;
      });
      if (signal?.aborted) {
        break;
      }
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
        if (signal?.aborted) {
          return {
            kind: "immediate",
            result: createErrorToolResult("Operation aborted"),
            isError: true
          };
        }
        if (beforeResult?.block) {
          return {
            kind: "immediate",
            result: createErrorToolResult(beforeResult.reason || "Tool execution was blocked"),
            isError: true
          };
        }
      }
      if (signal?.aborted) {
        return {
          kind: "immediate",
          result: createErrorToolResult("Operation aborted"),
          isError: true
        };
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
    let acceptingUpdates = true;
    try {
      const result = await prepared.tool.execute(prepared.toolCall.id, prepared.args, signal, (partialResult) => {
        if (!acceptingUpdates)
          return;
        updateEvents.push(Promise.resolve(emit({
          type: "tool_execution_update",
          toolCallId: prepared.toolCall.id,
          toolName: prepared.toolCall.name,
          args: prepared.toolCall.arguments,
          partialResult
        })));
      });
      acceptingUpdates = false;
      await Promise.all(updateEvents);
      return { result, isError: false };
    } catch (error) {
      acceptingUpdates = false;
      await Promise.all(updateEvents);
      return {
        result: createErrorToolResult(error instanceof Error ? error.message : String(error)),
        isError: true
      };
    } finally {
      acceptingUpdates = false;
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
            ...result,
            content: afterResult.content ?? result.content,
            details: afterResult.details ?? result.details,
            usage: afterResult.usage ?? result.usage,
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
      // Untyped tools (JS extensions) can return results without content; normalize
      // so the null never enters session history or provider payloads.
      content: finalized.result.content ?? [],
      details: finalized.result.details,
      usage: finalized.result.usage,
      ...finalized.result.addedToolNames?.length ? { addedToolNames: finalized.result.addedToolNames } : {},
      isError: finalized.isError,
      timestamp: Date.now()
    };
  }
  async function emitToolResultMessage(toolResultMessage, emit) {
    await emit({ type: "message_start", message: toolResultMessage });
    await emit({ type: "message_end", message: toolResultMessage });
  }

  // node_modules/@earendil-works/pi-agent-core/dist/agent.js
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
    messages = [];
    mode;
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
    streamFunction;
    getApiKey;
    onPayload;
    onResponse;
    beforeToolCall;
    afterToolCall;
    prepareNextTurn;
    prepareNextTurnWithContext;
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
    constructor(options) {
      const runtimeOptions = options ?? {};
      this._state = createMutableAgentState(runtimeOptions.initialState);
      this.convertToLlm = runtimeOptions.convertToLlm ?? defaultConvertToLlm;
      this.transformContext = runtimeOptions.transformContext;
      this.streamFunction = runtimeOptions.streamFn ?? getDefaultStreamFn();
      this.getApiKey = runtimeOptions.getApiKey;
      this.onPayload = runtimeOptions.onPayload;
      this.onResponse = runtimeOptions.onResponse;
      this.beforeToolCall = runtimeOptions.beforeToolCall;
      this.afterToolCall = runtimeOptions.afterToolCall;
      this.prepareNextTurn = runtimeOptions.prepareNextTurn;
      this.prepareNextTurnWithContext = runtimeOptions.prepareNextTurnWithContext;
      this.steeringQueue = new PendingMessageQueue(runtimeOptions.steeringMode ?? "one-at-a-time");
      this.followUpQueue = new PendingMessageQueue(runtimeOptions.followUpMode ?? "one-at-a-time");
      this.sessionId = runtimeOptions.sessionId;
      this.thinkingBudgets = runtimeOptions.thinkingBudgets;
      this.transport = runtimeOptions.transport ?? "auto";
      this.maxRetryDelayMs = runtimeOptions.maxRetryDelayMs;
      this.toolExecution = runtimeOptions.toolExecution ?? "parallel";
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
        await runAgentLoop(messages, this.createContextSnapshot(), this.createLoopConfig(options), (event) => this.processEvents(event), signal, this.streamFunction);
      });
    }
    async runContinuation() {
      await this.runWithLifecycle(async (signal) => {
        await runAgentLoopContinue(this.createContextSnapshot(), this.createLoopConfig(), (event) => this.processEvents(event), signal, this.streamFunction);
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
        prepareNextTurn: this.prepareNextTurnWithContext || this.prepareNextTurn ? async (context) => {
          if (this.prepareNextTurnWithContext) {
            return await this.prepareNextTurnWithContext(context, this.signal);
          }
          return await this.prepareNextTurn?.(this.signal);
        } : void 0,
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
      await this.processEvents({ type: "message_start", message: failureMessage });
      await this.processEvents({ type: "message_end", message: failureMessage });
      await this.processEvents({ type: "turn_end", message: failureMessage, toolResults: [] });
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

  // node_modules/yaml/browser/dist/nodes/identity.js
  var ALIAS = Symbol.for("yaml.alias");
  var DOC = Symbol.for("yaml.document");
  var MAP = Symbol.for("yaml.map");
  var PAIR = Symbol.for("yaml.pair");
  var SCALAR = Symbol.for("yaml.scalar");
  var SEQ = Symbol.for("yaml.seq");
  var NODE_TYPE = Symbol.for("yaml.node.type");
  var isAlias = (node) => !!node && typeof node === "object" && node[NODE_TYPE] === ALIAS;
  var isDocument = (node) => !!node && typeof node === "object" && node[NODE_TYPE] === DOC;
  var isMap = (node) => !!node && typeof node === "object" && node[NODE_TYPE] === MAP;
  var isPair = (node) => !!node && typeof node === "object" && node[NODE_TYPE] === PAIR;
  var isScalar = (node) => !!node && typeof node === "object" && node[NODE_TYPE] === SCALAR;
  var isSeq = (node) => !!node && typeof node === "object" && node[NODE_TYPE] === SEQ;
  function isCollection(node) {
    if (node && typeof node === "object")
      switch (node[NODE_TYPE]) {
        case MAP:
        case SEQ:
          return true;
      }
    return false;
  }
  function isNode(node) {
    if (node && typeof node === "object")
      switch (node[NODE_TYPE]) {
        case ALIAS:
        case MAP:
        case SCALAR:
        case SEQ:
          return true;
      }
    return false;
  }
  var hasAnchor = (node) => (isScalar(node) || isCollection(node)) && !!node.anchor;

  // node_modules/yaml/browser/dist/visit.js
  var BREAK = Symbol("break visit");
  var SKIP = Symbol("skip children");
  var REMOVE = Symbol("remove node");
  function visit(node, visitor) {
    const visitor_ = initVisitor(visitor);
    if (isDocument(node)) {
      const cd = visit_(null, node.contents, visitor_, Object.freeze([node]));
      if (cd === REMOVE)
        node.contents = null;
    } else
      visit_(null, node, visitor_, Object.freeze([]));
  }
  visit.BREAK = BREAK;
  visit.SKIP = SKIP;
  visit.REMOVE = REMOVE;
  function visit_(key, node, visitor, path) {
    const ctrl = callVisitor(key, node, visitor, path);
    if (isNode(ctrl) || isPair(ctrl)) {
      replaceNode(key, path, ctrl);
      return visit_(key, ctrl, visitor, path);
    }
    if (typeof ctrl !== "symbol") {
      if (isCollection(node)) {
        path = Object.freeze(path.concat(node));
        for (let i = 0; i < node.items.length; ++i) {
          const ci = visit_(i, node.items[i], visitor, path);
          if (typeof ci === "number")
            i = ci - 1;
          else if (ci === BREAK)
            return BREAK;
          else if (ci === REMOVE) {
            node.items.splice(i, 1);
            i -= 1;
          }
        }
      } else if (isPair(node)) {
        path = Object.freeze(path.concat(node));
        const ck = visit_("key", node.key, visitor, path);
        if (ck === BREAK)
          return BREAK;
        else if (ck === REMOVE)
          node.key = null;
        const cv = visit_("value", node.value, visitor, path);
        if (cv === BREAK)
          return BREAK;
        else if (cv === REMOVE)
          node.value = null;
      }
    }
    return ctrl;
  }
  async function visitAsync(node, visitor) {
    const visitor_ = initVisitor(visitor);
    if (isDocument(node)) {
      const cd = await visitAsync_(null, node.contents, visitor_, Object.freeze([node]));
      if (cd === REMOVE)
        node.contents = null;
    } else
      await visitAsync_(null, node, visitor_, Object.freeze([]));
  }
  visitAsync.BREAK = BREAK;
  visitAsync.SKIP = SKIP;
  visitAsync.REMOVE = REMOVE;
  async function visitAsync_(key, node, visitor, path) {
    const ctrl = await callVisitor(key, node, visitor, path);
    if (isNode(ctrl) || isPair(ctrl)) {
      replaceNode(key, path, ctrl);
      return visitAsync_(key, ctrl, visitor, path);
    }
    if (typeof ctrl !== "symbol") {
      if (isCollection(node)) {
        path = Object.freeze(path.concat(node));
        for (let i = 0; i < node.items.length; ++i) {
          const ci = await visitAsync_(i, node.items[i], visitor, path);
          if (typeof ci === "number")
            i = ci - 1;
          else if (ci === BREAK)
            return BREAK;
          else if (ci === REMOVE) {
            node.items.splice(i, 1);
            i -= 1;
          }
        }
      } else if (isPair(node)) {
        path = Object.freeze(path.concat(node));
        const ck = await visitAsync_("key", node.key, visitor, path);
        if (ck === BREAK)
          return BREAK;
        else if (ck === REMOVE)
          node.key = null;
        const cv = await visitAsync_("value", node.value, visitor, path);
        if (cv === BREAK)
          return BREAK;
        else if (cv === REMOVE)
          node.value = null;
      }
    }
    return ctrl;
  }
  function initVisitor(visitor) {
    if (typeof visitor === "object" && (visitor.Collection || visitor.Node || visitor.Value)) {
      return Object.assign({
        Alias: visitor.Node,
        Map: visitor.Node,
        Scalar: visitor.Node,
        Seq: visitor.Node
      }, visitor.Value && {
        Map: visitor.Value,
        Scalar: visitor.Value,
        Seq: visitor.Value
      }, visitor.Collection && {
        Map: visitor.Collection,
        Seq: visitor.Collection
      }, visitor);
    }
    return visitor;
  }
  function callVisitor(key, node, visitor, path) {
    if (typeof visitor === "function")
      return visitor(key, node, path);
    if (isMap(node))
      return visitor.Map?.(key, node, path);
    if (isSeq(node))
      return visitor.Seq?.(key, node, path);
    if (isPair(node))
      return visitor.Pair?.(key, node, path);
    if (isScalar(node))
      return visitor.Scalar?.(key, node, path);
    if (isAlias(node))
      return visitor.Alias?.(key, node, path);
    return void 0;
  }
  function replaceNode(key, path, node) {
    const parent = path[path.length - 1];
    if (isCollection(parent)) {
      parent.items[key] = node;
    } else if (isPair(parent)) {
      if (key === "key")
        parent.key = node;
      else
        parent.value = node;
    } else if (isDocument(parent)) {
      parent.contents = node;
    } else {
      const pt = isAlias(parent) ? "alias" : "scalar";
      throw new Error(`Cannot replace node with ${pt} parent`);
    }
  }

  // node_modules/yaml/browser/dist/doc/directives.js
  var escapeChars = {
    "!": "%21",
    ",": "%2C",
    "[": "%5B",
    "]": "%5D",
    "{": "%7B",
    "}": "%7D"
  };
  var escapeTagName = (tn) => tn.replace(/[!,[\]{}]/g, (ch) => escapeChars[ch]);
  var Directives = class _Directives {
    constructor(yaml, tags) {
      this.docStart = null;
      this.docEnd = false;
      this.yaml = Object.assign({}, _Directives.defaultYaml, yaml);
      this.tags = Object.assign({}, _Directives.defaultTags, tags);
    }
    clone() {
      const copy = new _Directives(this.yaml, this.tags);
      copy.docStart = this.docStart;
      return copy;
    }
    /**
     * During parsing, get a Directives instance for the current document and
     * update the stream state according to the current version's spec.
     */
    atDocument() {
      const res = new _Directives(this.yaml, this.tags);
      switch (this.yaml.version) {
        case "1.1":
          this.atNextDocument = true;
          break;
        case "1.2":
          this.atNextDocument = false;
          this.yaml = {
            explicit: _Directives.defaultYaml.explicit,
            version: "1.2"
          };
          this.tags = Object.assign({}, _Directives.defaultTags);
          break;
      }
      return res;
    }
    /**
     * @param onError - May be called even if the action was successful
     * @returns `true` on success
     */
    add(line, onError) {
      if (this.atNextDocument) {
        this.yaml = { explicit: _Directives.defaultYaml.explicit, version: "1.1" };
        this.tags = Object.assign({}, _Directives.defaultTags);
        this.atNextDocument = false;
      }
      const parts = line.trim().split(/[ \t]+/);
      const name = parts.shift();
      switch (name) {
        case "%TAG": {
          if (parts.length !== 2) {
            onError(0, "%TAG directive should contain exactly two parts");
            if (parts.length < 2)
              return false;
          }
          const [handle, prefix] = parts;
          this.tags[handle] = prefix;
          return true;
        }
        case "%YAML": {
          this.yaml.explicit = true;
          if (parts.length !== 1) {
            onError(0, "%YAML directive should contain exactly one part");
            return false;
          }
          const [version] = parts;
          if (version === "1.1" || version === "1.2") {
            this.yaml.version = version;
            return true;
          } else {
            const isValid = /^\d+\.\d+$/.test(version);
            onError(6, `Unsupported YAML version ${version}`, isValid);
            return false;
          }
        }
        default:
          onError(0, `Unknown directive ${name}`, true);
          return false;
      }
    }
    /**
     * Resolves a tag, matching handles to those defined in %TAG directives.
     *
     * @returns Resolved tag, which may also be the non-specific tag `'!'` or a
     *   `'!local'` tag, or `null` if unresolvable.
     */
    tagName(source, onError) {
      if (source === "!")
        return "!";
      if (source[0] !== "!") {
        onError(`Not a valid tag: ${source}`);
        return null;
      }
      if (source[1] === "<") {
        const verbatim = source.slice(2, -1);
        if (verbatim === "!" || verbatim === "!!") {
          onError(`Verbatim tags aren't resolved, so ${source} is invalid.`);
          return null;
        }
        if (source[source.length - 1] !== ">")
          onError("Verbatim tags must end with a >");
        return verbatim;
      }
      const [, handle, suffix] = source.match(/^(.*!)([^!]*)$/s);
      if (!suffix)
        onError(`The ${source} tag has no suffix`);
      const prefix = this.tags[handle];
      if (prefix) {
        try {
          return prefix + decodeURIComponent(suffix);
        } catch (error) {
          onError(String(error));
          return null;
        }
      }
      if (handle === "!")
        return source;
      onError(`Could not resolve tag: ${source}`);
      return null;
    }
    /**
     * Given a fully resolved tag, returns its printable string form,
     * taking into account current tag prefixes and defaults.
     */
    tagString(tag) {
      for (const [handle, prefix] of Object.entries(this.tags)) {
        if (tag.startsWith(prefix))
          return handle + escapeTagName(tag.substring(prefix.length));
      }
      return tag[0] === "!" ? tag : `!<${tag}>`;
    }
    toString(doc) {
      const lines = this.yaml.explicit ? [`%YAML ${this.yaml.version || "1.2"}`] : [];
      const tagEntries = Object.entries(this.tags);
      let tagNames;
      if (doc && tagEntries.length > 0 && isNode(doc.contents)) {
        const tags = {};
        visit(doc.contents, (_key, node) => {
          if (isNode(node) && node.tag)
            tags[node.tag] = true;
        });
        tagNames = Object.keys(tags);
      } else
        tagNames = [];
      for (const [handle, prefix] of tagEntries) {
        if (handle === "!!" && prefix === "tag:yaml.org,2002:")
          continue;
        if (!doc || tagNames.some((tn) => tn.startsWith(prefix)))
          lines.push(`%TAG ${handle} ${prefix}`);
      }
      return lines.join("\n");
    }
  };
  Directives.defaultYaml = { explicit: false, version: "1.2" };
  Directives.defaultTags = { "!!": "tag:yaml.org,2002:" };

  // node_modules/yaml/browser/dist/doc/anchors.js
  function anchorIsValid(anchor) {
    if (/[\x00-\x19\s,[\]{}]/.test(anchor)) {
      const sa = JSON.stringify(anchor);
      const msg = `Anchor must not contain whitespace or control characters: ${sa}`;
      throw new Error(msg);
    }
    return true;
  }

  // node_modules/yaml/browser/dist/doc/applyReviver.js
  function applyReviver(reviver, obj, key, val) {
    if (val && typeof val === "object") {
      if (Array.isArray(val)) {
        for (let i = 0, len = val.length; i < len; ++i) {
          const v0 = val[i];
          const v1 = applyReviver(reviver, val, String(i), v0);
          if (v1 === void 0)
            delete val[i];
          else if (v1 !== v0)
            val[i] = v1;
        }
      } else if (val instanceof Map) {
        for (const k of Array.from(val.keys())) {
          const v0 = val.get(k);
          const v1 = applyReviver(reviver, val, k, v0);
          if (v1 === void 0)
            val.delete(k);
          else if (v1 !== v0)
            val.set(k, v1);
        }
      } else if (val instanceof Set) {
        for (const v0 of Array.from(val)) {
          const v1 = applyReviver(reviver, val, v0, v0);
          if (v1 === void 0)
            val.delete(v0);
          else if (v1 !== v0) {
            val.delete(v0);
            val.add(v1);
          }
        }
      } else {
        for (const [k, v0] of Object.entries(val)) {
          const v1 = applyReviver(reviver, val, k, v0);
          if (v1 === void 0)
            delete val[k];
          else if (v1 !== v0)
            val[k] = v1;
        }
      }
    }
    return reviver.call(obj, key, val);
  }

  // node_modules/yaml/browser/dist/nodes/toJS.js
  function toJS(value, arg, ctx) {
    if (Array.isArray(value))
      return value.map((v, i) => toJS(v, String(i), ctx));
    if (value && typeof value.toJSON === "function") {
      if (!ctx || !hasAnchor(value))
        return value.toJSON(arg, ctx);
      const data = { aliasCount: 0, count: 1, res: void 0 };
      ctx.anchors.set(value, data);
      ctx.onCreate = (res2) => {
        data.res = res2;
        delete ctx.onCreate;
      };
      const res = value.toJSON(arg, ctx);
      if (ctx.onCreate)
        ctx.onCreate(res);
      return res;
    }
    if (typeof value === "bigint" && !ctx?.keep)
      return Number(value);
    return value;
  }

  // node_modules/yaml/browser/dist/nodes/Node.js
  var NodeBase = class {
    constructor(type) {
      Object.defineProperty(this, NODE_TYPE, { value: type });
    }
    /** Create a copy of this node.  */
    clone() {
      const copy = Object.create(Object.getPrototypeOf(this), Object.getOwnPropertyDescriptors(this));
      if (this.range)
        copy.range = this.range.slice();
      return copy;
    }
    /** A plain JavaScript representation of this node. */
    toJS(doc, { mapAsMap, maxAliasCount, onAnchor, reviver } = {}) {
      if (!isDocument(doc))
        throw new TypeError("A document argument is required");
      const ctx = {
        anchors: /* @__PURE__ */ new Map(),
        doc,
        keep: true,
        mapAsMap: mapAsMap === true,
        mapKeyWarned: false,
        maxAliasCount: typeof maxAliasCount === "number" ? maxAliasCount : 100
      };
      const res = toJS(this, "", ctx);
      if (typeof onAnchor === "function")
        for (const { count, res: res2 } of ctx.anchors.values())
          onAnchor(res2, count);
      return typeof reviver === "function" ? applyReviver(reviver, { "": res }, "", res) : res;
    }
  };

  // node_modules/yaml/browser/dist/nodes/Alias.js
  var Alias = class extends NodeBase {
    constructor(source) {
      super(ALIAS);
      this.source = source;
      Object.defineProperty(this, "tag", {
        set() {
          throw new Error("Alias nodes cannot have tags");
        }
      });
    }
    /**
     * Resolve the value of this alias within `doc`, finding the last
     * instance of the `source` anchor before this node.
     */
    resolve(doc, ctx) {
      if (ctx?.maxAliasCount === 0)
        throw new ReferenceError("Alias resolution is disabled");
      let nodes;
      if (ctx?.aliasResolveCache) {
        nodes = ctx.aliasResolveCache;
      } else {
        nodes = [];
        visit(doc, {
          Node: (_key, node) => {
            if (isAlias(node) || hasAnchor(node))
              nodes.push(node);
          }
        });
        if (ctx)
          ctx.aliasResolveCache = nodes;
      }
      let found = void 0;
      for (const node of nodes) {
        if (node === this)
          break;
        if (node.anchor === this.source)
          found = node;
      }
      return found;
    }
    toJSON(_arg, ctx) {
      if (!ctx)
        return { source: this.source };
      const { anchors, doc, maxAliasCount } = ctx;
      const source = this.resolve(doc, ctx);
      if (!source) {
        const msg = `Unresolved alias (the anchor must be set before the alias): ${this.source}`;
        throw new ReferenceError(msg);
      }
      let data = anchors.get(source);
      if (!data) {
        toJS(source, null, ctx);
        data = anchors.get(source);
      }
      if (data?.res === void 0) {
        const msg = "This should not happen: Alias anchor was not resolved?";
        throw new ReferenceError(msg);
      }
      if (maxAliasCount >= 0) {
        data.count += 1;
        if (data.aliasCount === 0)
          data.aliasCount = getAliasCount(doc, source, anchors);
        if (data.count * data.aliasCount > maxAliasCount) {
          const msg = "Excessive alias count indicates a resource exhaustion attack";
          throw new ReferenceError(msg);
        }
      }
      return data.res;
    }
    toString(ctx, _onComment, _onChompKeep) {
      const src = `*${this.source}`;
      if (ctx) {
        anchorIsValid(this.source);
        if (ctx.options.verifyAliasOrder && !ctx.anchors.has(this.source)) {
          const msg = `Unresolved alias (the anchor must be set before the alias): ${this.source}`;
          throw new Error(msg);
        }
        if (ctx.implicitKey)
          return `${src} `;
      }
      return src;
    }
  };
  function getAliasCount(doc, node, anchors) {
    if (isAlias(node)) {
      const source = node.resolve(doc);
      const anchor = anchors && source && anchors.get(source);
      return anchor ? anchor.count * anchor.aliasCount : 0;
    } else if (isCollection(node)) {
      let count = 0;
      for (const item of node.items) {
        const c = getAliasCount(doc, item, anchors);
        if (c > count)
          count = c;
      }
      return count;
    } else if (isPair(node)) {
      const kc = getAliasCount(doc, node.key, anchors);
      const vc = getAliasCount(doc, node.value, anchors);
      return Math.max(kc, vc);
    }
    return 1;
  }

  // node_modules/yaml/browser/dist/nodes/Scalar.js
  var isScalarValue = (value) => !value || typeof value !== "function" && typeof value !== "object";
  var Scalar = class extends NodeBase {
    constructor(value) {
      super(SCALAR);
      this.value = value;
    }
    toJSON(arg, ctx) {
      return ctx?.keep ? this.value : toJS(this.value, arg, ctx);
    }
    toString() {
      return String(this.value);
    }
  };
  Scalar.BLOCK_FOLDED = "BLOCK_FOLDED";
  Scalar.BLOCK_LITERAL = "BLOCK_LITERAL";
  Scalar.PLAIN = "PLAIN";
  Scalar.QUOTE_DOUBLE = "QUOTE_DOUBLE";
  Scalar.QUOTE_SINGLE = "QUOTE_SINGLE";

  // node_modules/yaml/browser/dist/doc/createNode.js
  var defaultTagPrefix = "tag:yaml.org,2002:";
  function findTagObject(value, tagName, tags) {
    if (tagName) {
      const match = tags.filter((t) => t.tag === tagName);
      const tagObj = match.find((t) => !t.format) ?? match[0];
      if (!tagObj)
        throw new Error(`Tag ${tagName} not found`);
      return tagObj;
    }
    return tags.find((t) => t.identify?.(value) && !t.format);
  }
  function createNode(value, tagName, ctx) {
    if (isDocument(value))
      value = value.contents;
    if (isNode(value))
      return value;
    if (isPair(value)) {
      const map2 = ctx.schema[MAP].createNode?.(ctx.schema, null, ctx);
      map2.items.push(value);
      return map2;
    }
    if (value instanceof String || value instanceof Number || value instanceof Boolean || typeof BigInt !== "undefined" && value instanceof BigInt) {
      value = value.valueOf();
    }
    const { aliasDuplicateObjects, onAnchor, onTagObj, schema: schema4, sourceObjects } = ctx;
    let ref = void 0;
    if (aliasDuplicateObjects && value && typeof value === "object") {
      ref = sourceObjects.get(value);
      if (ref) {
        ref.anchor ?? (ref.anchor = onAnchor(value));
        return new Alias(ref.anchor);
      } else {
        ref = { anchor: null, node: null };
        sourceObjects.set(value, ref);
      }
    }
    if (tagName?.startsWith("!!"))
      tagName = defaultTagPrefix + tagName.slice(2);
    let tagObj = findTagObject(value, tagName, schema4.tags);
    if (!tagObj) {
      if (value && typeof value.toJSON === "function") {
        value = value.toJSON();
      }
      if (!value || typeof value !== "object") {
        const node2 = new Scalar(value);
        if (ref)
          ref.node = node2;
        return node2;
      }
      tagObj = value instanceof Map ? schema4[MAP] : Symbol.iterator in Object(value) ? schema4[SEQ] : schema4[MAP];
    }
    if (onTagObj) {
      onTagObj(tagObj);
      delete ctx.onTagObj;
    }
    const node = tagObj?.createNode ? tagObj.createNode(ctx.schema, value, ctx) : typeof tagObj?.nodeClass?.from === "function" ? tagObj.nodeClass.from(ctx.schema, value, ctx) : new Scalar(value);
    if (tagName)
      node.tag = tagName;
    else if (!tagObj.default)
      node.tag = tagObj.tag;
    if (ref)
      ref.node = node;
    return node;
  }

  // node_modules/yaml/browser/dist/nodes/Collection.js
  function collectionFromPath(schema4, path, value) {
    let v = value;
    for (let i = path.length - 1; i >= 0; --i) {
      const k = path[i];
      if (typeof k === "number" && Number.isInteger(k) && k >= 0) {
        const a = [];
        a[k] = v;
        v = a;
      } else {
        v = /* @__PURE__ */ new Map([[k, v]]);
      }
    }
    return createNode(v, void 0, {
      aliasDuplicateObjects: false,
      keepUndefined: false,
      onAnchor: () => {
        throw new Error("This should not happen, please report a bug.");
      },
      schema: schema4,
      sourceObjects: /* @__PURE__ */ new Map()
    });
  }
  var isEmptyPath = (path) => path == null || typeof path === "object" && !!path[Symbol.iterator]().next().done;
  var Collection = class extends NodeBase {
    constructor(type, schema4) {
      super(type);
      Object.defineProperty(this, "schema", {
        value: schema4,
        configurable: true,
        enumerable: false,
        writable: true
      });
    }
    /**
     * Create a copy of this collection.
     *
     * @param schema - If defined, overwrites the original's schema
     */
    clone(schema4) {
      const copy = Object.create(Object.getPrototypeOf(this), Object.getOwnPropertyDescriptors(this));
      if (schema4)
        copy.schema = schema4;
      copy.items = copy.items.map((it) => isNode(it) || isPair(it) ? it.clone(schema4) : it);
      if (this.range)
        copy.range = this.range.slice();
      return copy;
    }
    /**
     * Adds a value to the collection. For `!!map` and `!!omap` the value must
     * be a Pair instance or a `{ key, value }` object, which may not have a key
     * that already exists in the map.
     */
    addIn(path, value) {
      if (isEmptyPath(path))
        this.add(value);
      else {
        const [key, ...rest] = path;
        const node = this.get(key, true);
        if (isCollection(node))
          node.addIn(rest, value);
        else if (node === void 0 && this.schema)
          this.set(key, collectionFromPath(this.schema, rest, value));
        else
          throw new Error(`Expected YAML collection at ${key}. Remaining path: ${rest}`);
      }
    }
    /**
     * Removes a value from the collection.
     * @returns `true` if the item was found and removed.
     */
    deleteIn(path) {
      const [key, ...rest] = path;
      if (rest.length === 0)
        return this.delete(key);
      const node = this.get(key, true);
      if (isCollection(node))
        return node.deleteIn(rest);
      else
        throw new Error(`Expected YAML collection at ${key}. Remaining path: ${rest}`);
    }
    /**
     * Returns item at `key`, or `undefined` if not found. By default unwraps
     * scalar values from their surrounding node; to disable set `keepScalar` to
     * `true` (collections are always returned intact).
     */
    getIn(path, keepScalar) {
      const [key, ...rest] = path;
      const node = this.get(key, true);
      if (rest.length === 0)
        return !keepScalar && isScalar(node) ? node.value : node;
      else
        return isCollection(node) ? node.getIn(rest, keepScalar) : void 0;
    }
    hasAllNullValues(allowScalar) {
      return this.items.every((node) => {
        if (!isPair(node))
          return false;
        const n = node.value;
        return n == null || allowScalar && isScalar(n) && n.value == null && !n.commentBefore && !n.comment && !n.tag;
      });
    }
    /**
     * Checks if the collection includes a value with the key `key`.
     */
    hasIn(path) {
      const [key, ...rest] = path;
      if (rest.length === 0)
        return this.has(key);
      const node = this.get(key, true);
      return isCollection(node) ? node.hasIn(rest) : false;
    }
    /**
     * Sets a value in this collection. For `!!set`, `value` needs to be a
     * boolean to add/remove the item from the set.
     */
    setIn(path, value) {
      const [key, ...rest] = path;
      if (rest.length === 0) {
        this.set(key, value);
      } else {
        const node = this.get(key, true);
        if (isCollection(node))
          node.setIn(rest, value);
        else if (node === void 0 && this.schema)
          this.set(key, collectionFromPath(this.schema, rest, value));
        else
          throw new Error(`Expected YAML collection at ${key}. Remaining path: ${rest}`);
      }
    }
  };

  // node_modules/yaml/browser/dist/stringify/stringifyComment.js
  var stringifyComment = (str) => str.replace(/^(?!$)(?: $)?/gm, "#");
  function indentComment(comment, indent) {
    if (/^\n+$/.test(comment))
      return comment.substring(1);
    return indent ? comment.replace(/^(?! *$)/gm, indent) : comment;
  }
  var lineComment = (str, indent, comment) => str.endsWith("\n") ? indentComment(comment, indent) : comment.includes("\n") ? "\n" + indentComment(comment, indent) : (str.endsWith(" ") ? "" : " ") + comment;

  // node_modules/yaml/browser/dist/stringify/foldFlowLines.js
  var FOLD_FLOW = "flow";
  var FOLD_BLOCK = "block";
  var FOLD_QUOTED = "quoted";
  function foldFlowLines(text, indent, mode = "flow", { indentAtStart, lineWidth = 80, minContentWidth = 20, onFold, onOverflow } = {}) {
    if (!lineWidth || lineWidth < 0)
      return text;
    if (lineWidth < minContentWidth)
      minContentWidth = 0;
    const endStep = Math.max(1 + minContentWidth, 1 + lineWidth - indent.length);
    if (text.length <= endStep)
      return text;
    const folds = [];
    const escapedFolds = {};
    let end = lineWidth - indent.length;
    if (typeof indentAtStart === "number") {
      if (indentAtStart > lineWidth - Math.max(2, minContentWidth))
        folds.push(0);
      else
        end = lineWidth - indentAtStart;
    }
    let split = void 0;
    let prev = void 0;
    let overflow = false;
    let i = -1;
    let escStart = -1;
    let escEnd = -1;
    if (mode === FOLD_BLOCK) {
      i = consumeMoreIndentedLines(text, i, indent.length);
      if (i !== -1)
        end = i + endStep;
    }
    for (let ch; ch = text[i += 1]; ) {
      if (mode === FOLD_QUOTED && ch === "\\") {
        escStart = i;
        switch (text[i + 1]) {
          case "x":
            i += 3;
            break;
          case "u":
            i += 5;
            break;
          case "U":
            i += 9;
            break;
          default:
            i += 1;
        }
        escEnd = i;
      }
      if (ch === "\n") {
        if (mode === FOLD_BLOCK)
          i = consumeMoreIndentedLines(text, i, indent.length);
        end = i + indent.length + endStep;
        split = void 0;
      } else {
        if (ch === " " && prev && prev !== " " && prev !== "\n" && prev !== "	") {
          const next = text[i + 1];
          if (next && next !== " " && next !== "\n" && next !== "	")
            split = i;
        }
        if (i >= end) {
          if (split) {
            folds.push(split);
            end = split + endStep;
            split = void 0;
          } else if (mode === FOLD_QUOTED) {
            while (prev === " " || prev === "	") {
              prev = ch;
              ch = text[i += 1];
              overflow = true;
            }
            const j = i > escEnd + 1 ? i - 2 : escStart - 1;
            if (escapedFolds[j])
              return text;
            folds.push(j);
            escapedFolds[j] = true;
            end = j + endStep;
            split = void 0;
          } else {
            overflow = true;
          }
        }
      }
      prev = ch;
    }
    if (overflow && onOverflow)
      onOverflow();
    if (folds.length === 0)
      return text;
    if (onFold)
      onFold();
    let res = text.slice(0, folds[0]);
    for (let i2 = 0; i2 < folds.length; ++i2) {
      const fold = folds[i2];
      const end2 = folds[i2 + 1] || text.length;
      if (fold === 0)
        res = `
${indent}${text.slice(0, end2)}`;
      else {
        if (mode === FOLD_QUOTED && escapedFolds[fold])
          res += `${text[fold]}\\`;
        res += `
${indent}${text.slice(fold + 1, end2)}`;
      }
    }
    return res;
  }
  function consumeMoreIndentedLines(text, i, indent) {
    let end = i;
    let start = i + 1;
    let ch = text[start];
    while (ch === " " || ch === "	") {
      if (i < start + indent) {
        ch = text[++i];
      } else {
        do {
          ch = text[++i];
        } while (ch && ch !== "\n");
        end = i;
        start = i + 1;
        ch = text[start];
      }
    }
    return end;
  }

  // node_modules/yaml/browser/dist/stringify/stringifyString.js
  var getFoldOptions = (ctx, isBlock) => ({
    indentAtStart: isBlock ? ctx.indent.length : ctx.indentAtStart,
    lineWidth: ctx.options.lineWidth,
    minContentWidth: ctx.options.minContentWidth
  });
  var containsDocumentMarker = (str) => /^(%|---|\.\.\.)/m.test(str);
  function lineLengthOverLimit(str, lineWidth, indentLength) {
    if (!lineWidth || lineWidth < 0)
      return false;
    const limit = lineWidth - indentLength;
    const strLen = str.length;
    if (strLen <= limit)
      return false;
    for (let i = 0, start = 0; i < strLen; ++i) {
      if (str[i] === "\n") {
        if (i - start > limit)
          return true;
        start = i + 1;
        if (strLen - start <= limit)
          return false;
      }
    }
    return true;
  }
  function doubleQuotedString(value, ctx) {
    const json = JSON.stringify(value);
    if (ctx.options.doubleQuotedAsJSON)
      return json;
    const { implicitKey } = ctx;
    const minMultiLineLength = ctx.options.doubleQuotedMinMultiLineLength;
    const indent = ctx.indent || (containsDocumentMarker(value) ? "  " : "");
    let str = "";
    let start = 0;
    for (let i = 0, ch = json[i]; ch; ch = json[++i]) {
      if (ch === " " && json[i + 1] === "\\" && json[i + 2] === "n") {
        str += json.slice(start, i) + "\\ ";
        i += 1;
        start = i;
        ch = "\\";
      }
      if (ch === "\\")
        switch (json[i + 1]) {
          case "u":
            {
              str += json.slice(start, i);
              const code = json.substr(i + 2, 4);
              switch (code) {
                case "0000":
                  str += "\\0";
                  break;
                case "0007":
                  str += "\\a";
                  break;
                case "000b":
                  str += "\\v";
                  break;
                case "001b":
                  str += "\\e";
                  break;
                case "0085":
                  str += "\\N";
                  break;
                case "00a0":
                  str += "\\_";
                  break;
                case "2028":
                  str += "\\L";
                  break;
                case "2029":
                  str += "\\P";
                  break;
                default:
                  if (code.substr(0, 2) === "00")
                    str += "\\x" + code.substr(2);
                  else
                    str += json.substr(i, 6);
              }
              i += 5;
              start = i + 1;
            }
            break;
          case "n":
            if (implicitKey || json[i + 2] === '"' || json.length < minMultiLineLength) {
              i += 1;
            } else {
              str += json.slice(start, i) + "\n\n";
              while (json[i + 2] === "\\" && json[i + 3] === "n" && json[i + 4] !== '"') {
                str += "\n";
                i += 2;
              }
              str += indent;
              if (json[i + 2] === " ")
                str += "\\";
              i += 1;
              start = i + 1;
            }
            break;
          default:
            i += 1;
        }
    }
    str = start ? str + json.slice(start) : json;
    return implicitKey ? str : foldFlowLines(str, indent, FOLD_QUOTED, getFoldOptions(ctx, false));
  }
  function singleQuotedString(value, ctx) {
    if (ctx.options.singleQuote === false || ctx.implicitKey && value.includes("\n") || /[ \t]\n|\n[ \t]/.test(value))
      return doubleQuotedString(value, ctx);
    const indent = ctx.indent || (containsDocumentMarker(value) ? "  " : "");
    const res = "'" + value.replace(/'/g, "''").replace(/\n+/g, `$&
${indent}`) + "'";
    return ctx.implicitKey ? res : foldFlowLines(res, indent, FOLD_FLOW, getFoldOptions(ctx, false));
  }
  function quotedString(value, ctx) {
    const { singleQuote } = ctx.options;
    let qs;
    if (singleQuote === false)
      qs = doubleQuotedString;
    else {
      const hasDouble = value.includes('"');
      const hasSingle = value.includes("'");
      if (hasDouble && !hasSingle)
        qs = singleQuotedString;
      else if (hasSingle && !hasDouble)
        qs = doubleQuotedString;
      else
        qs = singleQuote ? singleQuotedString : doubleQuotedString;
    }
    return qs(value, ctx);
  }
  var blockEndNewlines;
  try {
    blockEndNewlines = new RegExp("(^|(?<!\n))\n+(?!\n|$)", "g");
  } catch {
    blockEndNewlines = /\n+(?!\n|$)/g;
  }
  function blockString({ comment, type, value }, ctx, onComment, onChompKeep) {
    const { blockQuote, commentString, lineWidth } = ctx.options;
    if (!blockQuote || /\n[\t ]+$/.test(value)) {
      return quotedString(value, ctx);
    }
    const indent = ctx.indent || (ctx.forceBlockIndent || containsDocumentMarker(value) ? "  " : "");
    const literal = blockQuote === "literal" ? true : blockQuote === "folded" || type === Scalar.BLOCK_FOLDED ? false : type === Scalar.BLOCK_LITERAL ? true : !lineLengthOverLimit(value, lineWidth, indent.length);
    if (!value)
      return literal ? "|\n" : ">\n";
    let chomp;
    let endStart;
    for (endStart = value.length; endStart > 0; --endStart) {
      const ch = value[endStart - 1];
      if (ch !== "\n" && ch !== "	" && ch !== " ")
        break;
    }
    let end = value.substring(endStart);
    const endNlPos = end.indexOf("\n");
    if (endNlPos === -1) {
      chomp = "-";
    } else if (value === end || endNlPos !== end.length - 1) {
      chomp = "+";
      if (onChompKeep)
        onChompKeep();
    } else {
      chomp = "";
    }
    if (end) {
      value = value.slice(0, -end.length);
      if (end[end.length - 1] === "\n")
        end = end.slice(0, -1);
      end = end.replace(blockEndNewlines, `$&${indent}`);
    }
    let startWithSpace = false;
    let startEnd;
    let startNlPos = -1;
    for (startEnd = 0; startEnd < value.length; ++startEnd) {
      const ch = value[startEnd];
      if (ch === " ")
        startWithSpace = true;
      else if (ch === "\n")
        startNlPos = startEnd;
      else
        break;
    }
    let start = value.substring(0, startNlPos < startEnd ? startNlPos + 1 : startEnd);
    if (start) {
      value = value.substring(start.length);
      start = start.replace(/\n+/g, `$&${indent}`);
    }
    const indentSize = indent ? "2" : "1";
    let header = (startWithSpace ? indentSize : "") + chomp;
    if (comment) {
      header += " " + commentString(comment.replace(/ ?[\r\n]+/g, " "));
      if (onComment)
        onComment();
    }
    if (!literal) {
      const foldedValue = value.replace(/\n+/g, "\n$&").replace(/(?:^|\n)([\t ].*)(?:([\n\t ]*)\n(?![\n\t ]))?/g, "$1$2").replace(/\n+/g, `$&${indent}`);
      let literalFallback = false;
      const foldOptions = getFoldOptions(ctx, true);
      if (blockQuote !== "folded" && type !== Scalar.BLOCK_FOLDED) {
        foldOptions.onOverflow = () => {
          literalFallback = true;
        };
      }
      const body = foldFlowLines(`${start}${foldedValue}${end}`, indent, FOLD_BLOCK, foldOptions);
      if (!literalFallback)
        return `>${header}
${indent}${body}`;
    }
    value = value.replace(/\n+/g, `$&${indent}`);
    return `|${header}
${indent}${start}${value}${end}`;
  }
  function plainString(item, ctx, onComment, onChompKeep) {
    const { type, value } = item;
    const { actualString, implicitKey, indent, indentStep, inFlow } = ctx;
    if (implicitKey && value.includes("\n") || inFlow && /[[\]{},]/.test(value)) {
      return quotedString(value, ctx);
    }
    if (/^[\n\t ,[\]{}#&*!|>'"%@`]|^[?-]$|^[?-][ \t]|[\n:][ \t]|[ \t]\n|[\n\t ]#|[\n\t :]$/.test(value)) {
      return implicitKey || inFlow || !value.includes("\n") ? quotedString(value, ctx) : blockString(item, ctx, onComment, onChompKeep);
    }
    if (!implicitKey && !inFlow && type !== Scalar.PLAIN && value.includes("\n")) {
      return blockString(item, ctx, onComment, onChompKeep);
    }
    if (containsDocumentMarker(value)) {
      if (indent === "") {
        ctx.forceBlockIndent = true;
        return blockString(item, ctx, onComment, onChompKeep);
      } else if (implicitKey && indent === indentStep) {
        return quotedString(value, ctx);
      }
    }
    const str = value.replace(/\n+/g, `$&
${indent}`);
    if (actualString) {
      const test = (tag) => tag.default && tag.tag !== "tag:yaml.org,2002:str" && tag.test?.test(str);
      const { compat, tags } = ctx.doc.schema;
      if (tags.some(test) || compat?.some(test))
        return quotedString(value, ctx);
    }
    return implicitKey ? str : foldFlowLines(str, indent, FOLD_FLOW, getFoldOptions(ctx, false));
  }
  function stringifyString(item, ctx, onComment, onChompKeep) {
    const { implicitKey, inFlow } = ctx;
    const ss = typeof item.value === "string" ? item : Object.assign({}, item, { value: String(item.value) });
    let { type } = item;
    if (type !== Scalar.QUOTE_DOUBLE) {
      if (/[\x00-\x08\x0b-\x1f\x7f-\x9f\u{D800}-\u{DFFF}]/u.test(ss.value))
        type = Scalar.QUOTE_DOUBLE;
    }
    const _stringify = (_type) => {
      switch (_type) {
        case Scalar.BLOCK_FOLDED:
        case Scalar.BLOCK_LITERAL:
          return implicitKey || inFlow ? quotedString(ss.value, ctx) : blockString(ss, ctx, onComment, onChompKeep);
        case Scalar.QUOTE_DOUBLE:
          return doubleQuotedString(ss.value, ctx);
        case Scalar.QUOTE_SINGLE:
          return singleQuotedString(ss.value, ctx);
        case Scalar.PLAIN:
          return plainString(ss, ctx, onComment, onChompKeep);
        default:
          return null;
      }
    };
    let res = _stringify(type);
    if (res === null) {
      const { defaultKeyType, defaultStringType } = ctx.options;
      const t = implicitKey && defaultKeyType || defaultStringType;
      res = _stringify(t);
      if (res === null)
        throw new Error(`Unsupported default string type ${t}`);
    }
    return res;
  }

  // node_modules/yaml/browser/dist/stringify/stringify.js
  function createStringifyContext(doc, options) {
    const opt = Object.assign({
      blockQuote: true,
      commentString: stringifyComment,
      defaultKeyType: null,
      defaultStringType: "PLAIN",
      directives: null,
      doubleQuotedAsJSON: false,
      doubleQuotedMinMultiLineLength: 40,
      falseStr: "false",
      flowCollectionPadding: true,
      indentSeq: true,
      lineWidth: 80,
      minContentWidth: 20,
      nullStr: "null",
      simpleKeys: false,
      singleQuote: null,
      trailingComma: false,
      trueStr: "true",
      verifyAliasOrder: true
    }, doc.schema.toStringOptions, options);
    let inFlow;
    switch (opt.collectionStyle) {
      case "block":
        inFlow = false;
        break;
      case "flow":
        inFlow = true;
        break;
      default:
        inFlow = null;
    }
    return {
      anchors: /* @__PURE__ */ new Set(),
      doc,
      flowCollectionPadding: opt.flowCollectionPadding ? " " : "",
      indent: "",
      indentStep: typeof opt.indent === "number" ? " ".repeat(opt.indent) : "  ",
      inFlow,
      options: opt
    };
  }
  function getTagObject(tags, item) {
    if (item.tag) {
      const match = tags.filter((t) => t.tag === item.tag);
      if (match.length > 0)
        return match.find((t) => t.format === item.format) ?? match[0];
    }
    let tagObj = void 0;
    let obj;
    if (isScalar(item)) {
      obj = item.value;
      let match = tags.filter((t) => t.identify?.(obj));
      if (match.length > 1) {
        const testMatch = match.filter((t) => t.test);
        if (testMatch.length > 0)
          match = testMatch;
      }
      tagObj = match.find((t) => t.format === item.format) ?? match.find((t) => !t.format);
    } else {
      obj = item;
      tagObj = tags.find((t) => t.nodeClass && obj instanceof t.nodeClass);
    }
    if (!tagObj) {
      const name = obj?.constructor?.name ?? (obj === null ? "null" : typeof obj);
      throw new Error(`Tag not resolved for ${name} value`);
    }
    return tagObj;
  }
  function stringifyProps(node, tagObj, { anchors, doc }) {
    if (!doc.directives)
      return "";
    const props = [];
    const anchor = (isScalar(node) || isCollection(node)) && node.anchor;
    if (anchor && anchorIsValid(anchor)) {
      anchors.add(anchor);
      props.push(`&${anchor}`);
    }
    const tag = node.tag ?? (tagObj.default ? null : tagObj.tag);
    if (tag)
      props.push(doc.directives.tagString(tag));
    return props.join(" ");
  }
  function stringify(item, ctx, onComment, onChompKeep) {
    if (isPair(item))
      return item.toString(ctx, onComment, onChompKeep);
    if (isAlias(item)) {
      if (ctx.doc.directives)
        return item.toString(ctx);
      if (ctx.resolvedAliases?.has(item)) {
        throw new TypeError(`Cannot stringify circular structure without alias nodes`);
      } else {
        if (ctx.resolvedAliases)
          ctx.resolvedAliases.add(item);
        else
          ctx.resolvedAliases = /* @__PURE__ */ new Set([item]);
        item = item.resolve(ctx.doc);
      }
    }
    let tagObj = void 0;
    const node = isNode(item) ? item : ctx.doc.createNode(item, { onTagObj: (o) => tagObj = o });
    tagObj ?? (tagObj = getTagObject(ctx.doc.schema.tags, node));
    const props = stringifyProps(node, tagObj, ctx);
    if (props.length > 0)
      ctx.indentAtStart = (ctx.indentAtStart ?? 0) + props.length + 1;
    const str = typeof tagObj.stringify === "function" ? tagObj.stringify(node, ctx, onComment, onChompKeep) : isScalar(node) ? stringifyString(node, ctx, onComment, onChompKeep) : node.toString(ctx, onComment, onChompKeep);
    if (!props)
      return str;
    return isScalar(node) || str[0] === "{" || str[0] === "[" ? `${props} ${str}` : `${props}
${ctx.indent}${str}`;
  }

  // node_modules/yaml/browser/dist/stringify/stringifyPair.js
  function stringifyPair({ key, value }, ctx, onComment, onChompKeep) {
    const { allNullValues, doc, indent, indentStep, options: { commentString, indentSeq, simpleKeys } } = ctx;
    let keyComment = isNode(key) && key.comment || null;
    if (simpleKeys) {
      if (keyComment) {
        throw new Error("With simple keys, key nodes cannot have comments");
      }
      if (isCollection(key) || !isNode(key) && typeof key === "object") {
        const msg = "With simple keys, collection cannot be used as a key value";
        throw new Error(msg);
      }
    }
    let explicitKey = !simpleKeys && (!key || keyComment && value == null && !ctx.inFlow || isCollection(key) || (isScalar(key) ? key.type === Scalar.BLOCK_FOLDED || key.type === Scalar.BLOCK_LITERAL : typeof key === "object"));
    ctx = Object.assign({}, ctx, {
      allNullValues: false,
      implicitKey: !explicitKey && (simpleKeys || !allNullValues),
      indent: indent + indentStep
    });
    let keyCommentDone = false;
    let chompKeep = false;
    let str = stringify(key, ctx, () => keyCommentDone = true, () => chompKeep = true);
    if (!explicitKey && !ctx.inFlow && str.length > 1024) {
      if (simpleKeys)
        throw new Error("With simple keys, single line scalar must not span more than 1024 characters");
      explicitKey = true;
    }
    if (ctx.inFlow) {
      if (allNullValues || value == null) {
        if (keyCommentDone && onComment)
          onComment();
        return str === "" ? "?" : explicitKey ? `? ${str}` : str;
      }
    } else if (allNullValues && !simpleKeys || value == null && explicitKey) {
      str = `? ${str}`;
      if (keyComment && !keyCommentDone) {
        str += lineComment(str, ctx.indent, commentString(keyComment));
      } else if (chompKeep && onChompKeep)
        onChompKeep();
      return str;
    }
    if (keyCommentDone)
      keyComment = null;
    if (explicitKey) {
      if (keyComment)
        str += lineComment(str, ctx.indent, commentString(keyComment));
      str = `? ${str}
${indent}:`;
    } else {
      str = `${str}:`;
      if (keyComment)
        str += lineComment(str, ctx.indent, commentString(keyComment));
    }
    let vsb, vcb, valueComment;
    if (isNode(value)) {
      vsb = !!value.spaceBefore;
      vcb = value.commentBefore;
      valueComment = value.comment;
    } else {
      vsb = false;
      vcb = null;
      valueComment = null;
      if (value && typeof value === "object")
        value = doc.createNode(value);
    }
    ctx.implicitKey = false;
    if (!explicitKey && !keyComment && isScalar(value))
      ctx.indentAtStart = str.length + 1;
    chompKeep = false;
    if (!indentSeq && indentStep.length >= 2 && !ctx.inFlow && !explicitKey && isSeq(value) && !value.flow && !value.tag && !value.anchor) {
      ctx.indent = ctx.indent.substring(2);
    }
    let valueCommentDone = false;
    const valueStr = stringify(value, ctx, () => valueCommentDone = true, () => chompKeep = true);
    let ws = " ";
    if (keyComment || vsb || vcb) {
      ws = vsb ? "\n" : "";
      if (vcb) {
        const cs = commentString(vcb);
        ws += `
${indentComment(cs, ctx.indent)}`;
      }
      if (valueStr === "" && !ctx.inFlow) {
        if (ws === "\n" && valueComment)
          ws = "\n\n";
      } else {
        ws += `
${ctx.indent}`;
      }
    } else if (!explicitKey && isCollection(value)) {
      const vs0 = valueStr[0];
      const nl0 = valueStr.indexOf("\n");
      const hasNewline = nl0 !== -1;
      const flow = ctx.inFlow ?? value.flow ?? value.items.length === 0;
      if (hasNewline || !flow) {
        let hasPropsLine = false;
        if (hasNewline && (vs0 === "&" || vs0 === "!")) {
          let sp0 = valueStr.indexOf(" ");
          if (vs0 === "&" && sp0 !== -1 && sp0 < nl0 && valueStr[sp0 + 1] === "!") {
            sp0 = valueStr.indexOf(" ", sp0 + 1);
          }
          if (sp0 === -1 || nl0 < sp0)
            hasPropsLine = true;
        }
        if (!hasPropsLine)
          ws = `
${ctx.indent}`;
      }
    } else if (valueStr === "" || valueStr[0] === "\n") {
      ws = "";
    }
    str += ws + valueStr;
    if (ctx.inFlow) {
      if (valueCommentDone && onComment)
        onComment();
    } else if (valueComment && !valueCommentDone) {
      str += lineComment(str, ctx.indent, commentString(valueComment));
    } else if (chompKeep && onChompKeep) {
      onChompKeep();
    }
    return str;
  }

  // node_modules/yaml/browser/dist/log.js
  function warn(logLevel, warning) {
    if (logLevel === "debug" || logLevel === "warn") {
      console.warn(warning);
    }
  }

  // node_modules/yaml/browser/dist/schema/yaml-1.1/merge.js
  var MERGE_KEY = "<<";
  var merge = {
    identify: (value) => value === MERGE_KEY || typeof value === "symbol" && value.description === MERGE_KEY,
    default: "key",
    tag: "tag:yaml.org,2002:merge",
    test: /^<<$/,
    resolve: () => Object.assign(new Scalar(Symbol(MERGE_KEY)), {
      addToJSMap: addMergeToJSMap
    }),
    stringify: () => MERGE_KEY
  };
  var isMergeKey = (ctx, key) => (merge.identify(key) || isScalar(key) && (!key.type || key.type === Scalar.PLAIN) && merge.identify(key.value)) && ctx?.doc.schema.tags.some((tag) => tag.tag === merge.tag && tag.default);
  function addMergeToJSMap(ctx, map2, value) {
    const source = resolveAliasValue(ctx, value);
    if (isSeq(source))
      for (const it of source.items)
        mergeValue(ctx, map2, it);
    else if (Array.isArray(source))
      for (const it of source)
        mergeValue(ctx, map2, it);
    else
      mergeValue(ctx, map2, source);
  }
  function mergeValue(ctx, map2, value) {
    const source = resolveAliasValue(ctx, value);
    if (!isMap(source))
      throw new Error("Merge sources must be maps or map aliases");
    const srcMap = source.toJSON(null, ctx, Map);
    for (const [key, value2] of srcMap) {
      if (map2 instanceof Map) {
        if (!map2.has(key))
          map2.set(key, value2);
      } else if (map2 instanceof Set) {
        map2.add(key);
      } else if (!Object.prototype.hasOwnProperty.call(map2, key)) {
        Object.defineProperty(map2, key, {
          value: value2,
          writable: true,
          enumerable: true,
          configurable: true
        });
      }
    }
    return map2;
  }
  function resolveAliasValue(ctx, value) {
    return ctx && isAlias(value) ? value.resolve(ctx.doc, ctx) : value;
  }

  // node_modules/yaml/browser/dist/nodes/addPairToJSMap.js
  function addPairToJSMap(ctx, map2, { key, value }) {
    if (isNode(key) && key.addToJSMap)
      key.addToJSMap(ctx, map2, value);
    else if (isMergeKey(ctx, key))
      addMergeToJSMap(ctx, map2, value);
    else {
      const jsKey = toJS(key, "", ctx);
      if (map2 instanceof Map) {
        map2.set(jsKey, toJS(value, jsKey, ctx));
      } else if (map2 instanceof Set) {
        map2.add(jsKey);
      } else {
        const stringKey = stringifyKey(key, jsKey, ctx);
        const jsValue = toJS(value, stringKey, ctx);
        if (stringKey in map2)
          Object.defineProperty(map2, stringKey, {
            value: jsValue,
            writable: true,
            enumerable: true,
            configurable: true
          });
        else
          map2[stringKey] = jsValue;
      }
    }
    return map2;
  }
  function stringifyKey(key, jsKey, ctx) {
    if (jsKey === null)
      return "";
    if (typeof jsKey !== "object")
      return String(jsKey);
    if (isNode(key) && ctx?.doc) {
      const strCtx = createStringifyContext(ctx.doc, {});
      strCtx.anchors = /* @__PURE__ */ new Set();
      for (const node of ctx.anchors.keys())
        strCtx.anchors.add(node.anchor);
      strCtx.inFlow = true;
      strCtx.inStringifyKey = true;
      const strKey = key.toString(strCtx);
      if (!ctx.mapKeyWarned) {
        let jsonStr = JSON.stringify(strKey);
        if (jsonStr.length > 40)
          jsonStr = jsonStr.substring(0, 36) + '..."';
        warn(ctx.doc.options.logLevel, `Keys with collection values will be stringified due to JS Object restrictions: ${jsonStr}. Set mapAsMap: true to use object keys.`);
        ctx.mapKeyWarned = true;
      }
      return strKey;
    }
    return JSON.stringify(jsKey);
  }

  // node_modules/yaml/browser/dist/nodes/Pair.js
  function createPair(key, value, ctx) {
    const k = createNode(key, void 0, ctx);
    const v = createNode(value, void 0, ctx);
    return new Pair(k, v);
  }
  var Pair = class _Pair {
    constructor(key, value = null) {
      Object.defineProperty(this, NODE_TYPE, { value: PAIR });
      this.key = key;
      this.value = value;
    }
    clone(schema4) {
      let { key, value } = this;
      if (isNode(key))
        key = key.clone(schema4);
      if (isNode(value))
        value = value.clone(schema4);
      return new _Pair(key, value);
    }
    toJSON(_, ctx) {
      const pair = ctx?.mapAsMap ? /* @__PURE__ */ new Map() : {};
      return addPairToJSMap(ctx, pair, this);
    }
    toString(ctx, onComment, onChompKeep) {
      return ctx?.doc ? stringifyPair(this, ctx, onComment, onChompKeep) : JSON.stringify(this);
    }
  };

  // node_modules/yaml/browser/dist/stringify/stringifyCollection.js
  function stringifyCollection(collection, ctx, options) {
    const flow = ctx.inFlow ?? collection.flow;
    const stringify4 = flow ? stringifyFlowCollection : stringifyBlockCollection;
    return stringify4(collection, ctx, options);
  }
  function stringifyBlockCollection({ comment, items }, ctx, { blockItemPrefix, flowChars, itemIndent, onChompKeep, onComment }) {
    const { indent, options: { commentString } } = ctx;
    const itemCtx = Object.assign({}, ctx, { indent: itemIndent, type: null });
    let chompKeep = false;
    const lines = [];
    for (let i = 0; i < items.length; ++i) {
      const item = items[i];
      let comment2 = null;
      if (isNode(item)) {
        if (!chompKeep && item.spaceBefore)
          lines.push("");
        addCommentBefore(ctx, lines, item.commentBefore, chompKeep);
        if (item.comment)
          comment2 = item.comment;
      } else if (isPair(item)) {
        const ik = isNode(item.key) ? item.key : null;
        if (ik) {
          if (!chompKeep && ik.spaceBefore)
            lines.push("");
          addCommentBefore(ctx, lines, ik.commentBefore, chompKeep);
        }
      }
      chompKeep = false;
      let str2 = stringify(item, itemCtx, () => comment2 = null, () => chompKeep = true);
      if (comment2)
        str2 += lineComment(str2, itemIndent, commentString(comment2));
      if (chompKeep && comment2)
        chompKeep = false;
      lines.push(blockItemPrefix + str2);
    }
    let str;
    if (lines.length === 0) {
      str = flowChars.start + flowChars.end;
    } else {
      str = lines[0];
      for (let i = 1; i < lines.length; ++i) {
        const line = lines[i];
        str += line ? `
${indent}${line}` : "\n";
      }
    }
    if (comment) {
      str += "\n" + indentComment(commentString(comment), indent);
      if (onComment)
        onComment();
    } else if (chompKeep && onChompKeep)
      onChompKeep();
    return str;
  }
  function stringifyFlowCollection({ items }, ctx, { flowChars, itemIndent }) {
    const { indent, indentStep, flowCollectionPadding: fcPadding, options: { commentString } } = ctx;
    itemIndent += indentStep;
    const itemCtx = Object.assign({}, ctx, {
      indent: itemIndent,
      inFlow: true,
      type: null
    });
    let reqNewline = false;
    let linesAtValue = 0;
    const lines = [];
    for (let i = 0; i < items.length; ++i) {
      const item = items[i];
      let comment = null;
      if (isNode(item)) {
        if (item.spaceBefore)
          lines.push("");
        addCommentBefore(ctx, lines, item.commentBefore, false);
        if (item.comment)
          comment = item.comment;
      } else if (isPair(item)) {
        const ik = isNode(item.key) ? item.key : null;
        if (ik) {
          if (ik.spaceBefore)
            lines.push("");
          addCommentBefore(ctx, lines, ik.commentBefore, false);
          if (ik.comment)
            reqNewline = true;
        }
        const iv = isNode(item.value) ? item.value : null;
        if (iv) {
          if (iv.comment)
            comment = iv.comment;
          if (iv.commentBefore)
            reqNewline = true;
        } else if (item.value == null && ik?.comment) {
          comment = ik.comment;
        }
      }
      if (comment)
        reqNewline = true;
      let str = stringify(item, itemCtx, () => comment = null);
      reqNewline || (reqNewline = lines.length > linesAtValue || str.includes("\n"));
      if (i < items.length - 1) {
        str += ",";
      } else if (ctx.options.trailingComma) {
        if (ctx.options.lineWidth > 0) {
          reqNewline || (reqNewline = lines.reduce((sum, line) => sum + line.length + 2, 2) + (str.length + 2) > ctx.options.lineWidth);
        }
        if (reqNewline) {
          str += ",";
        }
      }
      if (comment)
        str += lineComment(str, itemIndent, commentString(comment));
      lines.push(str);
      linesAtValue = lines.length;
    }
    const { start, end } = flowChars;
    if (lines.length === 0) {
      return start + end;
    } else {
      if (!reqNewline) {
        const len = lines.reduce((sum, line) => sum + line.length + 2, 2);
        reqNewline = ctx.options.lineWidth > 0 && len > ctx.options.lineWidth;
      }
      if (reqNewline) {
        let str = start;
        for (const line of lines)
          str += line ? `
${indentStep}${indent}${line}` : "\n";
        return `${str}
${indent}${end}`;
      } else {
        return `${start}${fcPadding}${lines.join(" ")}${fcPadding}${end}`;
      }
    }
  }
  function addCommentBefore({ indent, options: { commentString } }, lines, comment, chompKeep) {
    if (comment && chompKeep)
      comment = comment.replace(/^\n+/, "");
    if (comment) {
      const ic = indentComment(commentString(comment), indent);
      lines.push(ic.trimStart());
    }
  }

  // node_modules/yaml/browser/dist/nodes/YAMLMap.js
  function findPair(items, key) {
    const k = isScalar(key) ? key.value : key;
    for (const it of items) {
      if (isPair(it)) {
        if (it.key === key || it.key === k)
          return it;
        if (isScalar(it.key) && it.key.value === k)
          return it;
      }
    }
    return void 0;
  }
  var YAMLMap = class extends Collection {
    static get tagName() {
      return "tag:yaml.org,2002:map";
    }
    constructor(schema4) {
      super(MAP, schema4);
      this.items = [];
    }
    /**
     * A generic collection parsing method that can be extended
     * to other node classes that inherit from YAMLMap
     */
    static from(schema4, obj, ctx) {
      const { keepUndefined, replacer } = ctx;
      const map2 = new this(schema4);
      const add = (key, value) => {
        if (typeof replacer === "function")
          value = replacer.call(obj, key, value);
        else if (Array.isArray(replacer) && !replacer.includes(key))
          return;
        if (value !== void 0 || keepUndefined)
          map2.items.push(createPair(key, value, ctx));
      };
      if (obj instanceof Map) {
        for (const [key, value] of obj)
          add(key, value);
      } else if (obj && typeof obj === "object") {
        for (const key of Object.keys(obj))
          add(key, obj[key]);
      }
      if (typeof schema4.sortMapEntries === "function") {
        map2.items.sort(schema4.sortMapEntries);
      }
      return map2;
    }
    /**
     * Adds a value to the collection.
     *
     * @param overwrite - If not set `true`, using a key that is already in the
     *   collection will throw. Otherwise, overwrites the previous value.
     */
    add(pair, overwrite) {
      let _pair;
      if (isPair(pair))
        _pair = pair;
      else if (!pair || typeof pair !== "object" || !("key" in pair)) {
        _pair = new Pair(pair, pair?.value);
      } else
        _pair = new Pair(pair.key, pair.value);
      const prev = findPair(this.items, _pair.key);
      const sortEntries = this.schema?.sortMapEntries;
      if (prev) {
        if (!overwrite)
          throw new Error(`Key ${_pair.key} already set`);
        if (isScalar(prev.value) && isScalarValue(_pair.value))
          prev.value.value = _pair.value;
        else
          prev.value = _pair.value;
      } else if (sortEntries) {
        const i = this.items.findIndex((item) => sortEntries(_pair, item) < 0);
        if (i === -1)
          this.items.push(_pair);
        else
          this.items.splice(i, 0, _pair);
      } else {
        this.items.push(_pair);
      }
    }
    delete(key) {
      const it = findPair(this.items, key);
      if (!it)
        return false;
      const del = this.items.splice(this.items.indexOf(it), 1);
      return del.length > 0;
    }
    get(key, keepScalar) {
      const it = findPair(this.items, key);
      const node = it?.value;
      return (!keepScalar && isScalar(node) ? node.value : node) ?? void 0;
    }
    has(key) {
      return !!findPair(this.items, key);
    }
    set(key, value) {
      this.add(new Pair(key, value), true);
    }
    /**
     * @param ctx - Conversion context, originally set in Document#toJS()
     * @param {Class} Type - If set, forces the returned collection type
     * @returns Instance of Type, Map, or Object
     */
    toJSON(_, ctx, Type) {
      const map2 = Type ? new Type() : ctx?.mapAsMap ? /* @__PURE__ */ new Map() : {};
      if (ctx?.onCreate)
        ctx.onCreate(map2);
      for (const item of this.items)
        addPairToJSMap(ctx, map2, item);
      return map2;
    }
    toString(ctx, onComment, onChompKeep) {
      if (!ctx)
        return JSON.stringify(this);
      for (const item of this.items) {
        if (!isPair(item))
          throw new Error(`Map items must all be pairs; found ${JSON.stringify(item)} instead`);
      }
      if (!ctx.allNullValues && this.hasAllNullValues(false))
        ctx = Object.assign({}, ctx, { allNullValues: true });
      return stringifyCollection(this, ctx, {
        blockItemPrefix: "",
        flowChars: { start: "{", end: "}" },
        itemIndent: ctx.indent || "",
        onChompKeep,
        onComment
      });
    }
  };

  // node_modules/yaml/browser/dist/schema/common/map.js
  var map = {
    collection: "map",
    default: true,
    nodeClass: YAMLMap,
    tag: "tag:yaml.org,2002:map",
    resolve(map2, onError) {
      if (!isMap(map2))
        onError("Expected a mapping for this tag");
      return map2;
    },
    createNode: (schema4, obj, ctx) => YAMLMap.from(schema4, obj, ctx)
  };

  // node_modules/yaml/browser/dist/nodes/YAMLSeq.js
  var YAMLSeq = class extends Collection {
    static get tagName() {
      return "tag:yaml.org,2002:seq";
    }
    constructor(schema4) {
      super(SEQ, schema4);
      this.items = [];
    }
    add(value) {
      this.items.push(value);
    }
    /**
     * Removes a value from the collection.
     *
     * `key` must contain a representation of an integer for this to succeed.
     * It may be wrapped in a `Scalar`.
     *
     * @returns `true` if the item was found and removed.
     */
    delete(key) {
      const idx = asItemIndex(key);
      if (typeof idx !== "number")
        return false;
      const del = this.items.splice(idx, 1);
      return del.length > 0;
    }
    get(key, keepScalar) {
      const idx = asItemIndex(key);
      if (typeof idx !== "number")
        return void 0;
      const it = this.items[idx];
      return !keepScalar && isScalar(it) ? it.value : it;
    }
    /**
     * Checks if the collection includes a value with the key `key`.
     *
     * `key` must contain a representation of an integer for this to succeed.
     * It may be wrapped in a `Scalar`.
     */
    has(key) {
      const idx = asItemIndex(key);
      return typeof idx === "number" && idx < this.items.length;
    }
    /**
     * Sets a value in this collection. For `!!set`, `value` needs to be a
     * boolean to add/remove the item from the set.
     *
     * If `key` does not contain a representation of an integer, this will throw.
     * It may be wrapped in a `Scalar`.
     */
    set(key, value) {
      const idx = asItemIndex(key);
      if (typeof idx !== "number")
        throw new Error(`Expected a valid index, not ${key}.`);
      const prev = this.items[idx];
      if (isScalar(prev) && isScalarValue(value))
        prev.value = value;
      else
        this.items[idx] = value;
    }
    toJSON(_, ctx) {
      const seq2 = [];
      if (ctx?.onCreate)
        ctx.onCreate(seq2);
      let i = 0;
      for (const item of this.items)
        seq2.push(toJS(item, String(i++), ctx));
      return seq2;
    }
    toString(ctx, onComment, onChompKeep) {
      if (!ctx)
        return JSON.stringify(this);
      return stringifyCollection(this, ctx, {
        blockItemPrefix: "- ",
        flowChars: { start: "[", end: "]" },
        itemIndent: (ctx.indent || "") + "  ",
        onChompKeep,
        onComment
      });
    }
    static from(schema4, obj, ctx) {
      const { replacer } = ctx;
      const seq2 = new this(schema4);
      if (obj && Symbol.iterator in Object(obj)) {
        let i = 0;
        for (let it of obj) {
          if (typeof replacer === "function") {
            const key = obj instanceof Set ? it : String(i++);
            it = replacer.call(obj, key, it);
          }
          seq2.items.push(createNode(it, void 0, ctx));
        }
      }
      return seq2;
    }
  };
  function asItemIndex(key) {
    let idx = isScalar(key) ? key.value : key;
    if (idx && typeof idx === "string")
      idx = Number(idx);
    return typeof idx === "number" && Number.isInteger(idx) && idx >= 0 ? idx : null;
  }

  // node_modules/yaml/browser/dist/schema/common/seq.js
  var seq = {
    collection: "seq",
    default: true,
    nodeClass: YAMLSeq,
    tag: "tag:yaml.org,2002:seq",
    resolve(seq2, onError) {
      if (!isSeq(seq2))
        onError("Expected a sequence for this tag");
      return seq2;
    },
    createNode: (schema4, obj, ctx) => YAMLSeq.from(schema4, obj, ctx)
  };

  // node_modules/yaml/browser/dist/schema/json/schema.js
  function intIdentify(value) {
    return typeof value === "bigint" || Number.isInteger(value);
  }
  var stringifyJSON = ({ value }) => JSON.stringify(value);
  var jsonScalars = [
    {
      identify: (value) => typeof value === "string",
      default: true,
      tag: "tag:yaml.org,2002:str",
      resolve: (str) => str,
      stringify: stringifyJSON
    },
    {
      identify: (value) => value == null,
      createNode: () => new Scalar(null),
      default: true,
      tag: "tag:yaml.org,2002:null",
      test: /^null$/,
      resolve: () => null,
      stringify: stringifyJSON
    },
    {
      identify: (value) => typeof value === "boolean",
      default: true,
      tag: "tag:yaml.org,2002:bool",
      test: /^true$|^false$/,
      resolve: (str) => str === "true",
      stringify: stringifyJSON
    },
    {
      identify: intIdentify,
      default: true,
      tag: "tag:yaml.org,2002:int",
      test: /^-?(?:0|[1-9][0-9]*)$/,
      resolve: (str, _onError, { intAsBigInt }) => intAsBigInt ? BigInt(str) : parseInt(str, 10),
      stringify: ({ value }) => intIdentify(value) ? value.toString() : JSON.stringify(value)
    },
    {
      identify: (value) => typeof value === "number",
      default: true,
      tag: "tag:yaml.org,2002:float",
      test: /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]*)?(?:[eE][-+]?[0-9]+)?$/,
      resolve: (str) => parseFloat(str),
      stringify: stringifyJSON
    }
  ];
  var jsonError = {
    default: true,
    tag: "",
    test: /^/,
    resolve(str, onError) {
      onError(`Unresolved plain scalar ${JSON.stringify(str)}`);
      return str;
    }
  };
  var schema = [map, seq].concat(jsonScalars, jsonError);

  // node_modules/yaml/browser/dist/schema/yaml-1.1/pairs.js
  function createPairs(schema4, iterable, ctx) {
    const { replacer } = ctx;
    const pairs2 = new YAMLSeq(schema4);
    pairs2.tag = "tag:yaml.org,2002:pairs";
    let i = 0;
    if (iterable && Symbol.iterator in Object(iterable))
      for (let it of iterable) {
        if (typeof replacer === "function")
          it = replacer.call(iterable, String(i++), it);
        let key, value;
        if (Array.isArray(it)) {
          if (it.length === 2) {
            key = it[0];
            value = it[1];
          } else
            throw new TypeError(`Expected [key, value] tuple: ${it}`);
        } else if (it && it instanceof Object) {
          const keys = Object.keys(it);
          if (keys.length === 1) {
            key = keys[0];
            value = it[key];
          } else {
            throw new TypeError(`Expected tuple with one key, not ${keys.length} keys`);
          }
        } else {
          key = it;
        }
        pairs2.items.push(createPair(key, value, ctx));
      }
    return pairs2;
  }

  // node_modules/yaml/browser/dist/schema/yaml-1.1/omap.js
  var YAMLOMap = class _YAMLOMap extends YAMLSeq {
    constructor() {
      super();
      this.add = YAMLMap.prototype.add.bind(this);
      this.delete = YAMLMap.prototype.delete.bind(this);
      this.get = YAMLMap.prototype.get.bind(this);
      this.has = YAMLMap.prototype.has.bind(this);
      this.set = YAMLMap.prototype.set.bind(this);
      this.tag = _YAMLOMap.tag;
    }
    /**
     * If `ctx` is given, the return type is actually `Map<unknown, unknown>`,
     * but TypeScript won't allow widening the signature of a child method.
     */
    toJSON(_, ctx) {
      if (!ctx)
        return super.toJSON(_);
      const map2 = /* @__PURE__ */ new Map();
      if (ctx?.onCreate)
        ctx.onCreate(map2);
      for (const pair of this.items) {
        let key, value;
        if (isPair(pair)) {
          key = toJS(pair.key, "", ctx);
          value = toJS(pair.value, key, ctx);
        } else {
          key = toJS(pair, "", ctx);
        }
        if (map2.has(key))
          throw new Error("Ordered maps must not include duplicate keys");
        map2.set(key, value);
      }
      return map2;
    }
    static from(schema4, iterable, ctx) {
      const pairs2 = createPairs(schema4, iterable, ctx);
      const omap2 = new this();
      omap2.items = pairs2.items;
      return omap2;
    }
  };
  YAMLOMap.tag = "tag:yaml.org,2002:omap";

  // node_modules/yaml/browser/dist/schema/yaml-1.1/set.js
  var YAMLSet = class _YAMLSet extends YAMLMap {
    constructor(schema4) {
      super(schema4);
      this.tag = _YAMLSet.tag;
    }
    add(key) {
      let pair;
      if (isPair(key))
        pair = key;
      else if (key && typeof key === "object" && "key" in key && "value" in key && key.value === null)
        pair = new Pair(key.key, null);
      else
        pair = new Pair(key, null);
      const prev = findPair(this.items, pair.key);
      if (!prev)
        this.items.push(pair);
    }
    /**
     * If `keepPair` is `true`, returns the Pair matching `key`.
     * Otherwise, returns the value of that Pair's key.
     */
    get(key, keepPair) {
      const pair = findPair(this.items, key);
      return !keepPair && isPair(pair) ? isScalar(pair.key) ? pair.key.value : pair.key : pair;
    }
    set(key, value) {
      if (typeof value !== "boolean")
        throw new Error(`Expected boolean value for set(key, value) in a YAML set, not ${typeof value}`);
      const prev = findPair(this.items, key);
      if (prev && !value) {
        this.items.splice(this.items.indexOf(prev), 1);
      } else if (!prev && value) {
        this.items.push(new Pair(key));
      }
    }
    toJSON(_, ctx) {
      return super.toJSON(_, ctx, Set);
    }
    toString(ctx, onComment, onChompKeep) {
      if (!ctx)
        return JSON.stringify(this);
      if (this.hasAllNullValues(true))
        return super.toString(Object.assign({}, ctx, { allNullValues: true }), onComment, onChompKeep);
      else
        throw new Error("Set items must all have null values");
    }
    static from(schema4, iterable, ctx) {
      const { replacer } = ctx;
      const set2 = new this(schema4);
      if (iterable && Symbol.iterator in Object(iterable))
        for (let value of iterable) {
          if (typeof replacer === "function")
            value = replacer.call(iterable, value, value);
          set2.items.push(createPair(value, null, ctx));
        }
      return set2;
    }
  };
  YAMLSet.tag = "tag:yaml.org,2002:set";

  // node_modules/yaml/browser/dist/schema/yaml-1.1/timestamp.js
  function parseSexagesimal(str, asBigInt) {
    const sign = str[0];
    const parts = sign === "-" || sign === "+" ? str.substring(1) : str;
    const num = (n) => asBigInt ? BigInt(n) : Number(n);
    const res = parts.replace(/_/g, "").split(":").reduce((res2, p) => res2 * num(60) + num(p), num(0));
    return sign === "-" ? num(-1) * res : res;
  }
  var timestamp = {
    identify: (value) => value instanceof Date,
    default: true,
    tag: "tag:yaml.org,2002:timestamp",
    // If the time zone is omitted, the timestamp is assumed to be specified in UTC. The time part
    // may be omitted altogether, resulting in a date format. In such a case, the time part is
    // assumed to be 00:00:00Z (start of day, UTC).
    test: RegExp("^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})(?:(?:t|T|[ \\t]+)([0-9]{1,2}):([0-9]{1,2}):([0-9]{1,2}(\\.[0-9]+)?)(?:[ \\t]*(Z|[-+][012]?[0-9](?::[0-9]{2})?))?)?$"),
    resolve(str) {
      const match = str.match(timestamp.test);
      if (!match)
        throw new Error("!!timestamp expects a date, starting with yyyy-mm-dd");
      const [, year, month, day, hour, minute, second] = match.map(Number);
      const millisec = match[7] ? Number((match[7] + "00").substr(1, 3)) : 0;
      let date = Date.UTC(year, month - 1, day, hour || 0, minute || 0, second || 0, millisec);
      const tz = match[8];
      if (tz && tz !== "Z") {
        let d = parseSexagesimal(tz, false);
        if (Math.abs(d) < 30)
          d *= 60;
        date -= 6e4 * d;
      }
      return new Date(date);
    },
    stringify: ({ value }) => value?.toISOString().replace(/(T00:00:00)?\.000Z$/, "") ?? ""
  };

  // node_modules/yaml/browser/dist/parse/cst-visit.js
  var BREAK2 = Symbol("break visit");
  var SKIP2 = Symbol("skip children");
  var REMOVE2 = Symbol("remove item");
  function visit2(cst, visitor) {
    if ("type" in cst && cst.type === "document")
      cst = { start: cst.start, value: cst.value };
    _visit(Object.freeze([]), cst, visitor);
  }
  visit2.BREAK = BREAK2;
  visit2.SKIP = SKIP2;
  visit2.REMOVE = REMOVE2;
  visit2.itemAtPath = (cst, path) => {
    let item = cst;
    for (const [field, index] of path) {
      const tok = item?.[field];
      if (tok && "items" in tok) {
        item = tok.items[index];
      } else
        return void 0;
    }
    return item;
  };
  visit2.parentCollection = (cst, path) => {
    const parent = visit2.itemAtPath(cst, path.slice(0, -1));
    const field = path[path.length - 1][0];
    const coll = parent?.[field];
    if (coll && "items" in coll)
      return coll;
    throw new Error("Parent collection not found");
  };
  function _visit(path, item, visitor) {
    let ctrl = visitor(item, path);
    if (typeof ctrl === "symbol")
      return ctrl;
    for (const field of ["key", "value"]) {
      const token = item[field];
      if (token && "items" in token) {
        for (let i = 0; i < token.items.length; ++i) {
          const ci = _visit(Object.freeze(path.concat([[field, i]])), token.items[i], visitor);
          if (typeof ci === "number")
            i = ci - 1;
          else if (ci === BREAK2)
            return BREAK2;
          else if (ci === REMOVE2) {
            token.items.splice(i, 1);
            i -= 1;
          }
        }
        if (typeof ctrl === "function" && field === "key")
          ctrl = ctrl(item, path);
      }
    }
    return typeof ctrl === "function" ? ctrl(item, path) : ctrl;
  }

  // node_modules/yaml/browser/dist/parse/lexer.js
  var hexDigits = new Set("0123456789ABCDEFabcdef");
  var tagChars = new Set("0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-#;/?:@&=+$_.!~*'()");
  var flowIndicatorChars = new Set(",[]{}");
  var invalidAnchorChars = new Set(" ,[]{}\n\r	");

  // node_modules/@earendil-works/pi-agent-core/dist/harness/skills.js
  var import_ignore = __toESM(require_ignore(), 1);

  // node_modules/@earendil-works/pi-agent-core/dist/harness/utils/truncate.js
  var DEFAULT_MAX_BYTES = 50 * 1024;
  var runtimeBuffer = globalThis.Buffer;

  // src/trimmed/stream-core.ts
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
  function safeParse(text) {
    try {
      return JSON.parse(text);
    } catch {
      return {};
    }
  }
  function beginTurn(stream, partial, request, apply, signal) {
    let turnId;
    try {
      turnId = host.http.start(JSON.stringify(request));
    } catch (e) {
      partial.stopReason = "error";
      partial.errorMessage = String(e && e.message ? e.message : e);
      stream.push({ type: "error", reason: "error", error: partial });
      stream.end();
      return;
    }
    const turn = { stream, partial, apply, done: false };
    activeTurns.set(turnId, turn);
    const abort = () => {
      try {
        host.http.cancel(turnId);
      } catch {
      }
      if (!turn.done) {
        partial.stopReason = "aborted";
        stream.push({ type: "error", reason: "aborted", error: partial });
        stream.end();
      }
      activeTurns.delete(turnId);
    };
    if (signal) {
      if (signal.aborted) abort();
      else signal.addEventListener("abort", () => activeTurns.has(turnId) && abort());
    }
  }
  globalThis.__catpiPump = function() {
    if (activeTurns.size === 0) return;
    for (const [turnId, turn] of activeTurns) {
      let out;
      try {
        out = JSON.parse(host.http.drain(turnId));
      } catch (e) {
        fail(turn, String(e));
        activeTurns.delete(turnId);
        continue;
      }
      let finished = false;
      for (const line of out.lines) {
        const ev = safeParseOrNull(line);
        if (ev === null) continue;
        if (turn.apply(turn, ev)) {
          finished = true;
          break;
        }
      }
      if (out.error && !finished) {
        fail(turn, out.error);
        finished = true;
      } else if (out.done && !finished && !turn.stream.done) {
        fail(turn, "stream ended without a terminal event");
        finished = true;
      }
      if (finished) activeTurns.delete(turnId);
    }
  };
  function fail(turn, message) {
    if (turn.done) return;
    turn.done = true;
    turn.partial.stopReason = "error";
    turn.partial.errorMessage = message;
    turn.stream.push({ type: "error", reason: "error", error: turn.partial });
    turn.stream.end();
  }
  function safeParseOrNull(line) {
    try {
      return JSON.parse(line);
    } catch {
      return null;
    }
  }

  // src/trimmed/anthropic-stream.ts
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
              content: normalizeBlocks(m.content),
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
    return normalizeBlocks(content);
  }
  function normalizeBlocks(content) {
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
      auth: "x-api-key",
      body: {
        model: model.id,
        max_tokens: options.maxTokens || model.maxTokens || 4096,
        system: context.systemPrompt || void 0,
        messages: toAnthropicMessages(context),
        tools: toAnthropicTools(context.tools),
        stream: true
      }
    };
    beginTurn(stream, partial, request, applyAnthropicEvent, options.signal);
    return stream;
  }
  function applyAnthropicEvent(turn, ev) {
    const { partial, stream } = turn;
    switch (ev.type) {
      case "message_start":
        if (ev.message && ev.message.usage) partial.usage.input = ev.message.usage.input_tokens || 0;
        if (!turn.started) {
          turn.started = true;
          stream.push({ type: "start", partial });
        }
        return false;
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
          partial.content[i] = { type: "toolCall", id: b.id, name: b.name, arguments: {}, partialJson: "" };
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
      case "message_delta":
        if (ev.usage) partial.usage.output = ev.usage.output_tokens || partial.usage.output;
        if (ev.delta && ev.delta.stop_reason)
          partial.stopReason = ev.delta.stop_reason === "tool_use" ? "toolUse" : "stop";
        return false;
      case "message_stop":
        partial.usage.totalTokens = partial.usage.input + partial.usage.output;
        stream.push({ type: "done", reason: partial.stopReason, message: partial });
        stream.end();
        return true;
      case "error":
        partial.stopReason = "error";
        partial.errorMessage = ev.error ? ev.error.message || String(ev.error) : "stream error";
        stream.push({ type: "error", reason: "error", error: partial });
        stream.end();
        return true;
      default:
        return false;
    }
  }

  // src/trimmed/openai-stream.ts
  function toOpenAIMessages(context) {
    const messages = [];
    if (context.systemPrompt) messages.push({ role: "system", content: context.systemPrompt });
    for (const m of context.messages) {
      if (m.role === "user") {
        messages.push({ role: "user", content: userContent(m.content) });
      } else if (m.role === "assistant") {
        const text = m.content.filter((c) => c.type === "text").map((c) => c.text).join("");
        const toolCalls = m.content.filter((c) => c.type === "toolCall").map((c) => ({
          id: c.id,
          type: "function",
          function: { name: c.name, arguments: JSON.stringify(c.arguments ?? {}) }
        }));
        const msg = { role: "assistant", content: text || null };
        if (toolCalls.length) msg.tool_calls = toolCalls;
        messages.push(msg);
      } else if (m.role === "toolResult") {
        const parts = Array.isArray(m.content) ? m.content : [{ type: "text", text: String(m.content) }];
        const text = parts.filter((c) => c.type === "text").map((c) => c.text).join("\n") || "(done)";
        messages.push({ role: "tool", tool_call_id: m.toolCallId, content: text });
        const images = parts.filter((c) => c.type === "image");
        if (images.length) {
          messages.push({
            role: "user",
            content: images.map((c) => ({
              type: "image_url",
              image_url: { url: `data:${c.mimeType || "image/jpeg"};base64,${c.data}` }
            }))
          });
        }
      }
    }
    return messages;
  }
  function userContent(content) {
    if (typeof content === "string") return content;
    return content.map((c) => {
      if (c.type === "text") return { type: "text", text: c.text };
      if (c.type === "image")
        return {
          type: "image_url",
          image_url: { url: `data:${c.mimeType || "image/png"};base64,${c.data}` }
        };
      return { type: "text", text: JSON.stringify(c) };
    });
  }
  function toOpenAITools(tools) {
    if (!tools || !tools.length) return void 0;
    return tools.map((t) => ({
      type: "function",
      function: {
        name: t.name,
        description: t.description || "",
        parameters: t.parameters || { type: "object", properties: {} }
      }
    }));
  }
  function openaiStream(model, context, options) {
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
    const body = {
      model: model.id,
      messages: toOpenAIMessages(context),
      tools: toOpenAITools(context.tools),
      stream: true,
      max_completion_tokens: options.maxTokens || model.maxTokens || 1024
    };
    const reasoning = /^(gpt-5|o[134])/.test((model.id || "").toLowerCase());
    if (reasoning) {
      const effort = model.reasoningEffort || (body.tools ? "none" : null);
      if (effort) body.reasoning_effort = effort;
    }
    const request = {
      url: (model.baseUrl || "https://api.openai.com") + "/v1/chat/completions",
      apiKey: options.apiKey || model.apiKey || "",
      auth: "bearer",
      body
    };
    const scratch = { textIdx: -1, tools: /* @__PURE__ */ new Map() };
    beginTurn(stream, partial, request, (turn, ev) => applyOpenAIEvent(turn, ev, scratch), options.signal);
    return stream;
  }
  function applyOpenAIEvent(turn, ev, scratch) {
    const { partial, stream } = turn;
    if (ev.error) {
      partial.stopReason = "error";
      partial.errorMessage = ev.error.message || JSON.stringify(ev.error);
      stream.push({ type: "error", reason: "error", error: partial });
      stream.end();
      return true;
    }
    if (ev.usage) {
      partial.usage.input = ev.usage.prompt_tokens || partial.usage.input;
      partial.usage.output = ev.usage.completion_tokens || partial.usage.output;
    }
    const choice = ev.choices && ev.choices[0];
    if (!choice) return false;
    if (!turn.started) {
      turn.started = true;
      stream.push({ type: "start", partial });
    }
    const d = choice.delta || {};
    if (typeof d.content === "string" && d.content.length) {
      if (scratch.textIdx < 0) {
        scratch.textIdx = partial.content.length;
        partial.content[scratch.textIdx] = { type: "text", text: "" };
        stream.push({ type: "text_start", contentIndex: scratch.textIdx, partial });
      }
      const c = partial.content[scratch.textIdx];
      c.text += d.content;
      stream.push({ type: "text_delta", contentIndex: scratch.textIdx, delta: d.content, partial });
    }
    for (const tc of d.tool_calls || []) {
      let entry = scratch.tools.get(tc.index);
      if (!entry) {
        const piIdx = partial.content.length;
        partial.content[piIdx] = {
          type: "toolCall",
          id: tc.id || `call_${tc.index}`,
          name: tc.function && tc.function.name || "",
          arguments: {},
          partialJson: ""
        };
        entry = { piIdx };
        scratch.tools.set(tc.index, entry);
        stream.push({ type: "toolcall_start", contentIndex: piIdx, partial });
      }
      const block = partial.content[entry.piIdx];
      if (tc.function && tc.function.name) block.name = tc.function.name;
      if (tc.function && tc.function.arguments) {
        block.partialJson += tc.function.arguments;
        block.arguments = safeParse(block.partialJson);
        partial.content[entry.piIdx] = { ...block };
        stream.push({ type: "toolcall_delta", contentIndex: entry.piIdx, delta: tc.function.arguments, partial });
      }
    }
    if (choice.finish_reason) {
      if (scratch.textIdx >= 0) {
        const c = partial.content[scratch.textIdx];
        stream.push({ type: "text_end", contentIndex: scratch.textIdx, content: c.text, partial });
      }
      for (const entry of scratch.tools.values()) {
        const block = partial.content[entry.piIdx];
        delete block.partialJson;
        stream.push({ type: "toolcall_end", contentIndex: entry.piIdx, toolCall: block, partial });
      }
      partial.stopReason = choice.finish_reason === "tool_calls" ? "toolUse" : "stop";
      partial.usage.totalTokens = partial.usage.input + partial.usage.output;
      stream.push({ type: "done", reason: partial.stopReason, message: partial });
      stream.end();
      return true;
    }
    return false;
  }

  // src/trimmed/scripted.ts
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

  // src/trimmed/entry.ts
  function resolveProvider(cfg) {
    if (cfg.provider) return cfg.provider;
    const id = (cfg.model || "").toLowerCase();
    if (id.startsWith("gpt") || id.startsWith("o1") || id.startsWith("o3") || id.startsWith("o4"))
      return "openai";
    return "anthropic";
  }
  function buildModel(cfg, provider) {
    const openai = provider === "openai";
    return {
      id: cfg.model || (openai ? "gpt-4o" : "claude-opus-4-8"),
      name: cfg.model || (openai ? "gpt-4o" : "claude-opus-4-8"),
      api: openai ? "openai-completions" : "anthropic-messages",
      provider,
      baseUrl: cfg.baseUrl || (openai ? "https://api.openai.com" : "https://api.anthropic.com"),
      apiKey: cfg.apiKey || "",
      reasoning: false,
      reasoningEffort: cfg.reasoningEffort,
      // openai reasoning models; undefined = auto
      input: ["text", "image"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: cfg.contextWindow || 128e3,
      maxTokens: cfg.maxTokens || 1024
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
          if (m.stopReason === "error" || m.errorMessage) {
            out = { kind: "error", message: m.errorMessage || "stream error" };
          } else {
            const text = (m.content || []).filter((c) => c.type === "text").map((c) => c.text).join("");
            if (text) out = { kind: "assistant_text", text };
          }
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
      const provider = resolveProvider(cfg);
      const model = buildModel(cfg, provider);
      const tools = (cfg.tools || []).map(hostTool);
      const streamFn = cfg.scripted ? makeScriptedStream(cfg.scripted) : provider === "openai" ? openaiStream : anthropicStream;
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
      if (cfg.selfExtend) {
        agent.state.tools = [...agent.state.tools, definePluginTool()];
      }
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
    },
    /// Load an agent-authored TypeScript plugin at runtime. `tsSource` is
    /// transpiled natively (host.transpile → oxc) and its default export — a
    /// factory `(api) => void` — is invoked with a small plugin API that can
    /// register tools and subscribe to events on the running agent.
    loadPlugin(name, tsSource) {
      if (!agent) throw new Error("PocketPi.boot() first");
      const js = host.transpile(name, tsSource);
      const body = js.replace(/export\s+default\s+/, "return ");
      let factory;
      try {
        factory = new Function(body)();
      } catch (e) {
        throw new Error(`plugin '${name}' failed to evaluate: ${e && e.message ? e.message : e}`);
      }
      if (typeof factory !== "function") {
        throw new Error(`plugin '${name}' must 'export default' a function (api) => void`);
      }
      factory(makePluginApi(name));
      host.emit(JSON.stringify({ kind: "plugin_loaded", name }));
      return true;
    }
  };
  function makePluginApi(name) {
    return {
      addTool(def) {
        if (!def || !def.name || typeof def.execute !== "function") {
          throw new Error(`plugin '${name}': addTool needs { name, execute }`);
        }
        const tool = {
          name: def.name,
          label: def.name,
          description: def.description || "",
          parameters: def.parameters || { type: "object", properties: {} },
          execute: async (toolCallId, args) => {
            const r = await def.execute(args ?? {}, { toolCallId });
            return normalizePluginResult(r);
          }
        };
        agent.state.tools = [...agent.state.tools, tool];
      },
      onEvent(fn) {
        if (agent) agent.subscribe(fn);
      },
      callHostTool(hostName, args) {
        return host.tool(hostName, JSON.stringify(args ?? {}));
      },
      log(msg) {
        host.emit(JSON.stringify({ kind: "plugin_log", name, message: String(msg) }));
      }
    };
  }
  function definePluginTool() {
    return {
      name: "define_plugin",
      label: "define_plugin",
      description: "Give yourself a new tool by writing a TypeScript plugin. The source must `export default (api) => { api.addTool({ name, description, parameters, execute }) }`, where execute(args) returns a string or { content }. It is compiled and loaded immediately \u2014 call the new tool on your next turn.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "A short id for the plugin." },
          typescript: { type: "string", description: "The plugin's TypeScript source." }
        },
        required: ["name", "typescript"]
      },
      execute: async (_id, args) => {
        try {
          globalThis.PocketPi.loadPlugin(args.name, args.typescript);
          return { content: [{ type: "text", text: `Plugin '${args.name}' compiled and loaded.` }], details: {} };
        } catch (e) {
          return {
            content: [{ type: "text", text: `Plugin failed: ${e && e.message ? e.message : e}` }],
            details: {}
          };
        }
      }
    };
  }
  function normalizePluginResult(r) {
    if (r == null) return { content: [{ type: "text", text: "" }], details: {} };
    if (typeof r === "string") return { content: [{ type: "text", text: r }], details: {} };
    if (Array.isArray(r.content)) return { content: r.content, details: r.details ?? {} };
    if (r.text != null) return { content: [{ type: "text", text: String(r.text) }], details: {} };
    return { content: [{ type: "text", text: JSON.stringify(r) }], details: {} };
  }
})();
