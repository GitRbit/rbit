// Private file input and origin/path-bound Git credential protocol for local fixtures.
import { constants, openSync, closeSync, fstatSync, readSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function requirePrivateFileSupport() {
  if (process.platform === "win32") throw new Error("private fixture files require supported POSIX owner/mode verification; Windows ACL verification is unavailable");
  if (![constants.O_NOFOLLOW, constants.O_NONBLOCK].every((flag) => typeof flag === "number" && flag !== 0)) {
    throw new Error("private fixture files require supported no-follow and nonblocking open flags");
  }
}
export function privateText(path) {
  requirePrivateFileSupport();
  let fd;
  try {
    fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    const stat = fstatSync(fd);
    if (!stat.isFile() || stat.size > 4096 || ((stat.mode & 0o777) !== 0o600 || stat.uid !== process.getuid())) throw new Error();
    const bytes = Buffer.alloc(4097);
    let count = 0;
    while (count < bytes.length) { const n = readSync(fd, bytes, count, bytes.length - count, null); if (!n) break; count += n; }
    if (count > 4096) throw new Error();
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(0, count)).trim();
  } catch { throw new Error("private input must be an owned regular mode0600 UTF-8 file within 4096 bytes"); }
  finally { if (fd !== undefined) closeSync(fd); }
}
export function readCredentials(path) {
  const value = privateText(path);
  const match = /^perf:([^:\s\x00-\x1f\x7f]+)$/.exec(value);
  if (!match) throw new Error("malformed private fixture credentials");
  return { login: "perf", password: match[1] };
}
export function readSetupToken(path) {
  const token = privateText(path);
  if (!token || /[\x00-\x1f\x7f]/.test(token)) throw new Error("malformed private setup token");
  return token;
}
export function repositoryUrl(value) {
  // Match the original authority before URL canonicalization: shorthand/octal/hex
  // IPv4, DNS names, userinfo, escapes, whitespace and backslashes are not accepted.
  if (typeof value !== "string" || !/^https?:\/\/(?:127\.0\.0\.1|\[::1\])(?::[1-9][0-9]{0,4})?(?:\/[^?#\\\x00-\x20\x7f]*)?$/.test(value)) {
    throw new Error("fixture URL must use literal loopback 127.0.0.1 or [::1]");
  }
  const url = new URL(value);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.search || url.hash) throw new Error("invalid fixture repository URL");
  return url;
}
export function credentialReply(action, configured, credentialsFile, input) {
  if (action !== "get") return "";
  const expected = repositoryUrl(configured);
  if (Buffer.byteLength(input, "utf8") > 8192) return "";
  const fields = new Map();
  for (const line of input.split("\n")) {
    if (!line) break;
    const at = line.indexOf("=");
    if (at <= 0) return "";
    const key = line.slice(0, at), value = line.slice(at + 1);
    if (fields.has(key) || /[\r\0]/.test(value)) return "";
    fields.set(key, value);
  }
  if (fields.get("protocol") !== expected.protocol.slice(0, -1) || fields.get("path") !== expected.pathname.slice(1)) return "";
  let actual;
  try { actual = repositoryUrl(`${fields.get("protocol")}://${fields.get("host")}/`); } catch { return ""; }
  if (actual.origin !== expected.origin || actual.host !== fields.get("host")) return "";
  const { login, password } = readCredentials(credentialsFile);
  return `username=${login}\npassword=${password}\n\n`;
}
async function main() {
  const [configured, file, action] = process.argv.slice(2);
  let input = "";
  for await (const chunk of process.stdin) { input += chunk.toString("utf8"); if (Buffer.byteLength(input, "utf8") > 8192) throw new Error("credential input too large"); }
  process.stdout.write(credentialReply(action, configured, file, input));
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch(() => { console.error("fixture credential helper failed"); process.exitCode = 1; });
