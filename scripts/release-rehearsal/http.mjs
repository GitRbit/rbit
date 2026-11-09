// A browser stand-in for the rehearsal: a cookie jar, a same-origin Origin header on
// every request (rbit's CSRF check), no automatic redirects, plus TOTP codes and an
// SSH public key so the seeded accounts look like real ones.
import { createHmac, generateKeyPairSync } from "node:crypto";

export class Client {
  constructor(base) {
    this.base = base;
    this.cookies = new Map();
  }

  /** A second client sharing nothing but the address. */
  fresh() {
    return new Client(this.base);
  }

  /** Copies this client's cookies, e.g. to replay an old session later. */
  snapshotCookies() {
    return [...this.cookies];
  }

  restoreCookies(entries) {
    this.cookies = new Map(entries);
  }

  async request(method, path, { body, headers = {}, auth } = {}) {
    const all = { origin: this.base, ...headers };
    if (this.cookies.size) all.cookie = [...this.cookies].map(([name, value]) => `${name}=${value}`).join("; ");
    if (auth) all.authorization = `Basic ${Buffer.from(`${auth[0]}:${auth[1]}`).toString("base64")}`;
    const response = await fetch(new URL(path, this.base), { method, headers: all, body, redirect: "manual" });
    for (const line of response.headers.getSetCookie()) {
      const [pair, ...attributes] = line.split(";");
      const index = pair.indexOf("=");
      const name = pair.slice(0, index).trim();
      const value = pair.slice(index + 1).trim();
      const expired = attributes.some((attribute) => /^\s*max-age=0\s*$/i.test(attribute)) || value === "";
      if (expired) this.cookies.delete(name);
      else this.cookies.set(name, value);
    }
    const bytes = Buffer.from(await response.arrayBuffer());
    return { status: response.status, headers: response.headers, bytes, text: bytes.toString("utf8"), location: response.headers.get("location") };
  }

  get(path, options) {
    return this.request("GET", path, options);
  }

  form(path, fields, options = {}) {
    const body = new URLSearchParams(fields).toString();
    return this.request("POST", path, { ...options, body, headers: { "content-type": "application/x-www-form-urlencoded", ...options.headers } });
  }

  async json(method, path, value, token) {
    const headers = { accept: "application/json" };
    if (token) headers.authorization = `token ${token}`;
    if (value !== undefined) headers["content-type"] = "application/json";
    const response = await this.request(method, path, { body: value === undefined ? undefined : JSON.stringify(value), headers });
    let parsed = null;
    try { parsed = JSON.parse(response.text); } catch {}
    return { ...response, body: parsed };
  }
}

/** Fails with the response's start when `condition` is false. */
export function expect(condition, message, response) {
  if (condition) return;
  const detail = response ? ` (HTTP ${response.status} ${response.location ?? ""} ${response.text?.slice(0, 300) ?? ""})` : "";
  throw new Error(`${message}${detail}`);
}

/** The `value` attribute of the first element whose markup contains `marker`. */
export function attributeAfter(html, marker, name) {
  const start = html.indexOf(marker);
  if (start < 0) return null;
  const tagStart = html.lastIndexOf("<", start);
  const tagEnd = html.indexOf(">", start);
  const tag = html.slice(tagStart, tagEnd);
  const match = tag.match(new RegExp(`\\s${name}="([^"]*)"`));
  return match ? match[1].replaceAll("&#x2F;", "/").replaceAll("&amp;", "&") : null;
}

const BASE32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

export function base32Decode(text) {
  let bits = "";
  for (const char of text.replace(/[\s=]/g, "").toUpperCase()) {
    const value = BASE32.indexOf(char);
    if (value < 0) throw new Error(`invalid base32 character ${char}`);
    bits += value.toString(2).padStart(5, "0");
  }
  const bytes = [];
  for (let index = 0; index + 8 <= bits.length; index += 8) bytes.push(parseInt(bits.slice(index, index + 8), 2));
  return Buffer.from(bytes);
}

/** RFC 6238 TOTP (SHA-1, 30 s, 6 digits) for a base32 secret at time step `step`. */
export function totp(secret, step) {
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(step));
  const digest = createHmac("sha1", base32Decode(secret)).update(counter).digest();
  const offset = digest[digest.length - 1] & 0x0f;
  const code = (digest.readUInt32BE(offset) & 0x7fffffff) % 1_000_000;
  return String(code).padStart(6, "0");
}

export const currentStep = (now = Date.now()) => Math.floor(now / 30_000);

/**
 * A TOTP code for a step after `lastStep`: rbit refuses a code whose step was already
 * used, so this waits for the next step when needed. Returns `[code, step]`.
 */
export async function freshTotp(secret, lastStep = -1) {
  while (currentStep() <= lastStep) await new Promise((resolve) => setTimeout(resolve, 1000));
  const step = currentStep();
  return [totp(secret, step), step];
}

/** An OpenSSH `ssh-ed25519` public key line for a newly generated key. */
export function sshPublicKey(comment) {
  const { publicKey } = generateKeyPairSync("ed25519");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  const field = (buffer) => Buffer.concat([Buffer.from([0, 0, 0, buffer.length]), buffer]);
  const blob = Buffer.concat([field(Buffer.from("ssh-ed25519")), field(raw)]);
  return `ssh-ed25519 ${blob.toString("base64")} ${comment}`;
}
