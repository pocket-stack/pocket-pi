import { getOps } from "@pocketjs/framework";

// Dynamic App text is not visible to PocketJS's bundle-time font subsetter.
// Keep common English prose punctuation in every bundle that imports this
// module so model/provider text does not fall back to missing-glyph boxes.
export const DYNAMIC_TEXT_GLYPHS = "‘’“”–—…•";

const PREVIEW_SOURCE_CHARACTERS = 512;

function breakToken(token: string, width: (text: string) => number, maxWidth: number): string[] {
  const chunks: string[] = [];
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

// PocketJS Text only breaks on explicit newlines. Measure once per unique
// string and insert those newlines before the DrawList reaches native UI.
export function wrapLines(text: string, fontSlot: number, maxWidth: number): string[] {
  void DYNAMIC_TEXT_GLYPHS;
  const widths = new Map<string, number>();
  const width = (value: string) => {
    let measured = widths.get(value);
    if (measured === undefined) {
      measured = getOps().measureText(value, fontSlot);
      widths.set(value, measured);
    }
    return measured;
  };
  const lines: string[] = [];
  const spaceWidth = width(" ");
  for (const paragraph of text.split("\n")) {
    const words = paragraph.split(" ").flatMap((token) =>
      token && width(token) > maxWidth
        ? breakToken(token, width, maxWidth)
        : [token],
    );
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

export function wrapPreview(
  text: string,
  fontSlot: number,
  maxWidth: number,
  maxLines: number,
): string {
  const source = text.slice(0, PREVIEW_SOURCE_CHARACTERS);
  const lines = wrapLines(source, fontSlot, maxWidth);
  if (source.length === text.length && lines.length <= maxLines) return lines.join("\n");
  const visible = lines.slice(0, maxLines);
  const last = visible.length - 1;
  visible[last] = visible[last].replace(/[\s.]+$/, "") + "…";
  return visible.join("\n");
}
