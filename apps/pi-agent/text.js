(() => {
  const PREVIEW_SOURCE_CHARACTERS = 512;

  function breakToken(token, width, maxWidth) {
    const chunks = [];
    let chunk = "";
    let chunkWidth = 0;
    for (const character of token) {
      const characterWidth = width(character);
      if (chunk && chunkWidth + characterWidth > maxWidth) {
        chunks.push(chunk);
        chunk = "";
        chunkWidth = 0;
      }
      chunk += character;
      chunkWidth += characterWidth;
    }
    if (chunk) chunks.push(chunk);
    return chunks;
  }

  function wrapLines(text, style, maxWidth) {
    const widths = new Map();
    const width = (value) => {
      if (!widths.has(value)) widths.set(value, View.measureText(value, style));
      return widths.get(value);
    };
    const lines = [];
    const spaceWidth = width(" ");
    for (const paragraph of String(text).split("\n")) {
      const words = paragraph.split(" ").flatMap((token) =>
        token && width(token) > maxWidth ? breakToken(token, width, maxWidth) : [token]);
      let line = "";
      let lineWidth = 0;
      for (const word of words) {
        const wordWidth = width(word);
        if (!line) {
          line = word;
          lineWidth = wordWidth;
        } else if (lineWidth + spaceWidth + wordWidth <= maxWidth) {
          line += " " + word;
          lineWidth += spaceWidth + wordWidth;
        } else {
          lines.push(line);
          line = word;
          lineWidth = wordWidth;
        }
      }
      lines.push(line);
    }
    return lines;
  }

  function wrapPreview(text, style, maxWidth, maxLines) {
    const source = String(text).slice(0, PREVIEW_SOURCE_CHARACTERS);
    const lines = wrapLines(source, style, maxWidth);
    if (source.length === String(text).length && lines.length <= maxLines) return lines.join("\n");
    const visible = lines.slice(0, maxLines);
    const last = visible.length - 1;
    visible[last] = visible[last].replace(/[\s.]+$/, "") + "…";
    return visible.join("\n");
  }

  function nextVisualLine(text, maxWidth, startOffset, width) {
    let offset = startOffset;
    while (offset < text.length && text[offset] === " ") offset += 1;
    const lineStart = offset;
    let line = "";
    let lineWidth = 0;
    let lastSpaceOffset = -1;
    let lastSpaceIndex = -1;

    while (offset < text.length) {
      const character = text[offset];
      if (character === "\n" || character === "\r") {
        const nextOffset = character === "\r" && text[offset + 1] === "\n" ? offset + 2 : offset + 1;
        return { text: line.replace(/\s+$/, ""), nextOffset, sourceLineEnded: true };
      }
      const characterWidth = width(character);
      if (line && lineWidth + characterWidth > maxWidth) {
        if (lastSpaceOffset > lineStart) {
          return {
            text: line.slice(0, lastSpaceIndex).replace(/\s+$/, ""),
            nextOffset: lastSpaceOffset,
            sourceLineEnded: false,
          };
        }
        return { text: line, nextOffset: offset, sourceLineEnded: false };
      }
      line += character;
      lineWidth += characterWidth;
      offset += 1;
      if (character === " ") {
        lastSpaceOffset = offset;
        lastSpaceIndex = line.length - 1;
      }
    }
    return { text: line.replace(/\s+$/, ""), nextOffset: offset, sourceLineEnded: false };
  }

  function wrapTextPage(text, style, maxWidth, startOffset, startSourceLine, maxLines) {
    const lineLimit = Math.max(0, Math.floor(maxLines));
    const lines = [];
    let offset = Math.max(0, Math.min(Math.floor(startOffset), text.length));
    let sourceLine = Math.max(0, Math.floor(startSourceLine));
    let lastSourceLine = sourceLine;
    const widths = new Map();
    const width = (value) => {
      if (!widths.has(value)) widths.set(value, View.measureText(value, style));
      return widths.get(value);
    };

    while (offset < text.length && lines.length < lineLimit) {
      const visualLine = nextVisualLine(text, maxWidth, offset, width);
      lastSourceLine = sourceLine;
      lines.push(visualLine.text);
      offset = visualLine.nextOffset;
      if (visualLine.sourceLineEnded) sourceLine += 1;
    }
    return {
      text: lines.join("\n"),
      nextOffset: offset,
      startSourceLine: Math.max(0, Math.floor(startSourceLine)),
      nextSourceLine: sourceLine,
      lastSourceLine,
      hasMore: offset < text.length,
    };
  }

  globalThis.PiText = Object.freeze({ wrapLines, wrapPreview, wrapTextPage });
})();
