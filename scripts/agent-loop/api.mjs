// A metered HTTP client for the agent-loop benchmark: every call an agent recipe makes is
// counted with the bytes it read, so a recipe's cost is measured the same way each time.
// The token is sent in a header and never logged; nothing here prints a response body.

/** Counts calls and response bytes, per label, for one scenario. */
export class Meter {
  constructor() {
    this.calls = 0;
    this.bytes = 0;
    this.byLabel = {};
  }

  record(label, bytes) {
    this.calls += 1;
    this.bytes += bytes;
    const entry = (this.byLabel[label] ??= { calls: 0, bytes: 0 });
    entry.calls += 1;
    entry.bytes += bytes;
  }

  snapshot() {
    return { calls: this.calls, bytes: this.bytes, byLabel: structuredClone(this.byLabel) };
  }
}

/** A client for one rbit instance and one token. `meter` may be swapped per scenario. */
export class Api {
  constructor(base, token, { meter = new Meter(), timeoutMs = 30_000 } = {}) {
    if (!/^https?:\/\//.test(base)) throw new Error("the API base must be an http(s) URL");
    this.base = base.replace(/\/$/, "");
    this.token = token;
    this.meter = meter;
    this.timeoutMs = timeoutMs;
  }

  async request(method, path, { body, label = path, accept = "application/json" } = {}) {
    const headers = { Accept: accept, Authorization: `token ${this.token}` };
    if (body !== undefined) headers["Content-Type"] = "application/json";
    const response = await fetch(`${this.base}/api/v1${path}`, {
      method, headers, body: body === undefined ? undefined : JSON.stringify(body),
      redirect: "manual", signal: AbortSignal.timeout(this.timeoutMs),
    });
    const bytes = Buffer.from(await response.arrayBuffer());
    this.meter.record(label, bytes.length);
    return { status: response.status, headers: response.headers, text: bytes.toString("utf8") };
  }

  /** `GET` a JSON document; anything but 2xx is an error naming the status. */
  async json(path, options = {}) {
    const response = await this.request("GET", path, options);
    if (response.status < 200 || response.status > 299) throw new HttpError("GET", path, response.status);
    return JSON.parse(response.text);
  }

  /** `GET` a text document (a log); 404 is `null`. */
  async text(path, options = {}) {
    const response = await this.request("GET", path, { accept: "text/plain", ...options });
    if (response.status === 404) return null;
    if (response.status < 200 || response.status > 299) throw new HttpError("GET", path, response.status);
    return response.text;
  }

  async post(path, body, options = {}) {
    const response = await this.request("POST", path, { body, ...options });
    if (response.status < 200 || response.status > 299) throw new HttpError("POST", path, response.status);
    return response.text ? JSON.parse(response.text) : null;
  }
}

export class HttpError extends Error {
  constructor(method, path, status) {
    super(`${method} ${path} answered HTTP ${status}`);
    this.status = status;
  }
}

/**
 * A browser stand-in for the few setup steps that exist only as web forms (sign-up and
 * token creation): a cookie jar and the same-origin `Origin` header the CSRF check wants.
 */
export class FormClient {
  constructor(base) {
    this.base = base.replace(/\/$/, "");
    this.cookies = new Map();
  }

  async post(path, form) {
    const headers = { Origin: this.base, "Content-Type": "application/x-www-form-urlencoded" };
    if (this.cookies.size) headers.Cookie = [...this.cookies].map(([key, value]) => `${key}=${value}`).join("; ");
    const response = await fetch(this.base + path, {
      method: "POST", headers, body: new URLSearchParams(form), redirect: "manual",
      signal: AbortSignal.timeout(30_000),
    });
    for (const line of response.headers.getSetCookie()) {
      const [pair] = line.split(";");
      const at = pair.indexOf("=");
      this.cookies.set(pair.slice(0, at), pair.slice(at + 1));
    }
    return { status: response.status, location: response.headers.get("location"), text: await response.text() };
  }
}

/** The value of attribute `name` on the tag that carries `marker`, as the e2e harness reads it. */
export function attributeAfter(html, marker, name) {
  const start = html.indexOf(marker);
  if (start < 0) return null;
  const tagStart = html.lastIndexOf("<", start);
  const tagEnd = html.indexOf(">", start);
  if (tagStart < 0 || tagEnd < 0) return null;
  const match = html.slice(tagStart, tagEnd).match(new RegExp(` ${name}="([^"]*)"`));
  return match ? match[1] : null;
}
