import { getOps } from "@pocketjs/framework";

// Dynamic App text is not visible to PocketJS's bundle-time font subsetter.
// Keep common English prose punctuation in every bundle that imports this
// module so model/provider text does not fall back to missing-glyph boxes.
export const DYNAMIC_TEXT_GLYPHS = "‘’“”–—…•";

const widthCache = new Map<string, number>();
const wrapCache = new Map<string, string[]>();

function width(text: string, fontSlot: number): number {
  if (!text) return 0;
  const key = fontSlot + "|" + text;
  const cached = widthCache.get(key);
  if (cached !== undefined) return cached;
  const measured = getOps().measureText(text, fontSlot);
  widthCache.set(key, measured);
  return measured;
}

function breakToken(token: string, fontSlot: number, maxWidth: number): string[] {
  const chunks: string[] = [];
  let chunk = "";
  let chunkWidth = 0;
  for (const character of token) {
    const characterWidth = width(character, fontSlot);
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
  const key = fontSlot + "|" + maxWidth + "|" + text;
  const cached = wrapCache.get(key);
  if (cached) return cached;

  const lines: string[] = [];
  const spaceWidth = width(" ", fontSlot);
  for (const paragraph of text.split("\n")) {
    const words = paragraph.split(" ").flatMap((token) =>
      token && width(token, fontSlot) > maxWidth
        ? breakToken(token, fontSlot, maxWidth)
        : [token],
    );
    let line = "";
    let lineWidth = 0;
    for (const word of words) {
      const wordWidth = width(word, fontSlot);
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
  wrapCache.set(key, lines);
  return lines;
}

export function wrapPreview(
  text: string,
  fontSlot: number,
  maxWidth: number,
  maxLines: number,
): string {
  const lines = wrapLines(text, fontSlot, maxWidth);
  if (lines.length <= maxLines) return lines.join("\n");
  const visible = lines.slice(0, maxLines);
  visible[maxLines - 1] = visible[maxLines - 1].replace(/[\s.]+$/, "") + "…";
  return visible.join("\n");
}
