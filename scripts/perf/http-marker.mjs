// A bounded literal marker can occur anywhere in streamed HTML, including across chunks.
const MAX_MARKER_BYTES = 4096;
export const DEFAULT_PAGE_MARKER = 'id="main"';

export function validateMarker(marker) {
  if (typeof marker !== "string" || !marker.trim() || Buffer.byteLength(marker,"utf8") > MAX_MARKER_BYTES) {
    throw new Error("page marker must be a nonempty string of at most 4096 UTF8 bytes");
  }
  return marker;
}

export function streamingMarker(marker) {
  validateMarker(marker);
  let found = false;
  let carry = "";
  return {
    push(text) {
      if (found) return;
      const combined = carry + text;
      found = combined.includes(marker);
      // Keep only a possible marker prefix; one-character markers need no carry.
      carry = found || marker.length === 1 ? "" : combined.slice(1 - marker.length);
    },
    get found() { return found; },
  };
}
