// Private per-origin session acquisition; credentials/cookies never become sample metadata.
import { readFileSync } from "node:fs";

export async function sessionCookie(baseUrl, credentialsFile, timeoutMs) {
  const line = readFileSync(credentialsFile, "utf8").trim();
  const separator = line.indexOf(":");
  if (separator < 1 || !line.slice(separator + 1) || /[\r\n]/.test(line)) throw new Error("invalid private credentials file");
  const response = await fetch(new URL("/login", baseUrl), {
    method: "POST", redirect: "manual", signal: AbortSignal.timeout(timeoutMs),
    headers: { Origin: new URL(baseUrl).origin, "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({ login: line.slice(0, separator), password: line.slice(separator + 1), next: "/" }),
  });
  await response.arrayBuffer();
  if (response.status !== 303 || response.headers.get("location") !== "/") throw new Error("benchmark login did not finish first-factor authentication");
  const cookie = response.headers.getSetCookie().map(value => value.split(";")[0]).find(value => value.startsWith("rbit_session="));
  if (!cookie || /[\r\n]/.test(cookie)) throw new Error("benchmark login returned no session cookie");
  return cookie;
}

export function publicConfiguration(config) {
  return { ...config, variants: config.variants.map(variant => ({ ...variant,
    ...(variant.credentialsFile ? { credentialsFile: "[private file]" } : {}),
  })) };
}
