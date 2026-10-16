// Theme fallback, loaded synchronously in <head>. The server already renders the chosen
// theme (the `rustit-theme` cookie) into <html>, so on almost every page this does nothing.
// It covers responses rendered without the visitor's cookie (the offline page the service
// worker cached, error pages produced before the session is known), and moves a choice
// stored by older releases in localStorage into the cookie.
(() => {
  const root = document.documentElement;
  let theme = /(?:^|;\s*)rustit-theme=([a-z0-9-]{1,64})(?:;|$)/.exec(document.cookie)?.[1];
  if (!theme) {
    try {
      const legacy = localStorage.getItem("rustit-theme");
      if (legacy === "light" || legacy === "dark") {
        theme = legacy;
        const secure = location.protocol === "https:" ? "; Secure" : "";
        document.cookie = `rustit-theme=${legacy}; Path=/; Max-Age=31536000; SameSite=Lax${secure}`;
      }
      localStorage.removeItem("rustit-theme");
    } catch {
      /* storage unavailable */
    }
  }
  if (!theme || theme === (root.dataset.theme ?? "system")) return;
  if (theme === "system") {
    delete root.dataset.theme;
    delete root.dataset.scheme;
    return;
  }
  root.dataset.theme = theme;
  if (theme === "light" || theme === "dark") {
    root.dataset.scheme = theme;
    return;
  }
  delete root.dataset.scheme;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = `/assets/themes/${theme}.css`;
  link.dataset.themeStylesheet = "";
  document.head.append(link);
})();
