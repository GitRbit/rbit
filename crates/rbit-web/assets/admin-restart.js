// Loaded on confirmation before submitting, or as a fallback on the accepted page.
// Keeping the reconnect logic in the current document avoids requesting a script
// while the application is already offline.
const initialized = new WeakSet();

function reconnect(status) {
  if (initialized.has(status)) return;
  initialized.add(status);
  let controller;
  let timer;
  let stopped = false;
  let attempts = 0;

  async function poll() {
    if (stopped || !status.isConnected) return;
    if (document.hidden) { timer = setTimeout(poll, 3000); return; }
    controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);
    try {
      const response = await fetch("/admin/restart/status", {
        credentials: "same-origin", cache: "no-store", signal: controller.signal,
        headers: { Accept: "application/json" },
      });
      if (response.redirected || response.status === 401 || response.status === 403 || response.status === 404) {
        stopped = true;
        status.textContent = "Your session or permissions changed. Return to the dashboard to sign in again.";
        return;
      }
      if (!response.ok) throw new Error("Server unavailable");
      const result = await response.json();
      if (!stopped && result.restarting === false) { window.location.replace("/admin"); return; }
    } catch {
      // Connection failures are expected during the handover to the new application.
    } finally {
      clearTimeout(timeout);
    }
    attempts++;
    if (attempts >= 100) {
      status.textContent = "The server has not returned yet. Check its logs, then use Return to dashboard to try again.";
      return;
    }
    if (!stopped) timer = setTimeout(poll, 3000);
  }
  window.addEventListener("pagehide", () => { stopped = true; clearTimeout(timer); controller?.abort(); });
  poll();
}

function confirm(form) {
  if (initialized.has(form)) return;
  initialized.add(form);
  form.addEventListener("submit", async event => {
    event.preventDefault();
    const button = form.querySelector('button[type="submit"]');
    if (button.disabled) return;
    button.disabled = true;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 15000);
    try {
      const response = await fetch(form.action, {
        method: "POST", credentials: "same-origin", cache: "no-store",
        signal: controller.signal, body: new URLSearchParams(new FormData(form)),
        headers: { Accept: "text/html" },
      });
      if (response.redirected || response.status !== 202) throw new Error("Restart not accepted");
      const page = new DOMParser().parseFromString(await response.text(), "text/html");
      if (!form.isConnected) return;
      const status = page.querySelector("[data-restart-status]");
      const section = status?.closest("section");
      if (!section) throw new Error("Missing restart status");
      for (const script of section.querySelectorAll("script")) script.remove();
      form.closest("section").replaceWith(section);
      document.querySelector("h2").textContent = "Restarting server";
      reconnect(status);
    } catch {
      // A lost response may already have triggered shutdown. Never repeat a POST
      // automatically, or imply the restart succeeded when it was not confirmed.
      if (!form.isConnected) return;
      const message = document.createElement("p");
      message.className = "mt-3 text-xs text-ink-muted";
      message.setAttribute("role", "status");
      message.textContent = "The restart request could not be confirmed. Return to the dashboard to check the server before trying again.";
      form.append(message);
      form.querySelector("a").textContent = "Return to dashboard";
    } finally {
      clearTimeout(timeout);
    }
  });
}

function initialize() {
  const form = document.querySelector('form[action="/admin/restart"]');
  const status = document.querySelector("[data-restart-status]");
  if (form) confirm(form);
  if (status) reconnect(status);
}
initialize();
document.addEventListener("htmx:afterSettle", initialize);
