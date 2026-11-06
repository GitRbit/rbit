import { createHash } from "node:crypto";

export function canonicalMarkup(html) {
  let code = false;
  const tokens = html.match(/<[^>]+>|[^<]+/g) ?? [];
  return tokens.map((token) => {
    if (!token.startsWith("<")) return code ? token : token.replace(/\s+/g, " ").trim();
    if (token.startsWith("</code")) code = false;
    let quote = null, space = false, output = "";
    for (const char of token) {
      if (quote) { output += char; if (char === quote) quote = null; }
      else if (char === '"' || char === "'") { if (space) output += " "; space = false; quote = char; output += char; }
      else if (/\s/.test(char)) space = true;
      else { if (space && char !== ">") output += " "; space = false; output += char; }
    }
    if (/^<code(?:\s|>)/.test(output)) code = true;
    return output;
  }).filter(Boolean).join("");
}
export const markupDigest = (html) => createHash("sha256").update(canonicalMarkup(html)).digest("hex");
export const cases = [
  ...["overview", "run", "job"].flatMap((screen) => ["failed", "success", "cancelled", "running", "approval", "deployment", "loading", "empty", "error", "disabled", "denied", "disconnected", "truncated", "expired", "queued"].map((state) => ({ screen, state }))),
  { screen: "job", earlier: "1" }, { screen: "job", q: "assertion" },
  { screen: "overview", workflow: "1" }, { screen: "overview", branch: "<script>雪</script>" },
  { screen: "run", state: "approval", viewer: "reader" }, { screen: "run", state: "deployment", viewer: "reviewer" },
  { screen: "job", dir: "rtl", theme: "dna-dark", long: "1" },
];
