// Private per-origin session acquisition; credentials/cookies never become sample metadata.
import { readFileSync } from "node:fs";
import http from "node:http";
import { renderHeaderMs } from "./http-timing.mjs";

function signInForm(credentialsFile) {
  const line = readFileSync(credentialsFile, "utf8").trim();
  const separator = line.indexOf(":");
  if (separator < 1 || !line.slice(separator + 1) || /[\r\n]/.test(line)) throw new Error("invalid private credentials file");
  return new URLSearchParams({ login: line.slice(0, separator), password: line.slice(separator + 1), next: "/" }).toString();
}

export async function sessionCookie(baseUrl, credentialsFile, timeoutMs) {
  const response = await fetch(new URL("/login", baseUrl), {
    method: "POST", redirect: "manual", signal: AbortSignal.timeout(timeoutMs),
    headers: { Origin: new URL(baseUrl).origin, "Content-Type": "application/x-www-form-urlencoded" },
    body: signInForm(credentialsFile),
  });
  await response.arrayBuffer();
  if (response.status !== 303 || response.headers.get("location") !== "/") throw new Error("benchmark login did not finish first-factor authentication");
  const cookie = response.headers.getSetCookie().map(value => value.split(";")[0]).find(value => value.startsWith("rbit_session="));
  if (!cookie || /[\r\n]/.test(cookie)) throw new Error("benchmark login returned no session cookie");
  return cookie;
}

/** One timed first-factor sign-in (password verification and a new session). The form
 *  body and the returned session cookie are never part of the sample. */
export function signInSample(url, credentialsFile, options, agent) {
  const body = signInForm(credentialsFile);
  const headers = { Origin: new URL(url).origin, "Content-Type": "application/x-www-form-urlencoded", "Content-Length": Buffer.byteLength(body), "Accept-Language": options.acceptLanguage ?? "en" };
  return new Promise((done, reject) => {
    const start = performance.now();
    const request = http.request(url, { method: "POST", agent, headers }, (response) => {
      const headersMs = performance.now() - start;
      let bytes = 0;
      response.on("data", (chunk) => { bytes += chunk.length; });
      response.once("error", (error) => { clearTimeout(timer); reject(error); });
      response.once("end", () => {
        clearTimeout(timer);
        const errors = [];
        if (response.statusCode !== 303 || response.headers.location !== "/") errors.push(`sign-in returned HTTP ${response.statusCode}`);
        if (!(response.headers["set-cookie"] ?? []).some((value) => value.startsWith("rbit_session="))) errors.push("sign-in returned no session cookie");
        done({ url: String(url), requestHeaders: { ...headers, body: "[private form]" }, status: response.statusCode, valid: errors.length === 0, errors,
          headersMs, totalMs: performance.now() - start, renderMs: renderHeaderMs(response.headers["server-timing"] ?? ""), viewMs: null, footerSsrMs: null, bodyBytes: bytes });
      });
    });
    const timer = setTimeout(() => request.destroy(new Error(`request exceeded ${options.timeoutMs} ms`)), options.timeoutMs);
    request.once("error", (error) => { clearTimeout(timer); reject(error); });
    request.end(body);
  });
}

export function publicConfiguration(config) {
  return { ...config, variants: config.variants.map(variant => ({ ...variant,
    ...(variant.credentialsFile ? { credentialsFile: "[private file]" } : {}),
  })) };
}
