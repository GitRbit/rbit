// Theme fallback, loaded synchronously in <head>. The server already renders the chosen
// theme (the `rbit-theme` cookie) into <html>, so on almost every page this does nothing.
// It covers responses rendered without the visitor's cookie (the offline page the service
// worker cached, error pages produced before the session is known), and moves a choice
// stored by older releases (in localStorage, or the `rustit-theme` cookie from before the
// rename to rbit) into the cookie.
(() => {
  const root = document.documentElement;
  const cookie = (name) => new RegExp(`(?:^|;\\s*)${name}=([a-z0-9-]{1,64})(?:;|$)`).exec(document.cookie)?.[1];
  const secure = location.protocol === "https:" ? "; Secure" : "";
  let theme = cookie("rbit-theme");
  const legacyCookie = theme ? undefined : cookie("rustit-theme");
  if (legacyCookie) {
    theme = legacyCookie;
    document.cookie = `rbit-theme=${theme}; Path=/; Max-Age=31536000; SameSite=Lax${secure}`;
    document.cookie = "rustit-theme=; Path=/; Max-Age=0";
  }
  if (!theme) {
    try {
      const legacy = localStorage.getItem("rustit-theme");
      if (legacy === "light" || legacy === "dark") {
        theme = legacy;
        document.cookie = `rbit-theme=${legacy}; Path=/; Max-Age=31536000; SameSite=Lax${secure}`;
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
  const builtIn = { light: "light", dark: "dark", "dna-day": "light", "dna-dark": "dark" };
  if (theme in builtIn) {
    root.dataset.scheme = builtIn[theme];
    return;
  }
  delete root.dataset.scheme;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = `/assets/themes/${theme}.css`;
  link.dataset.themeStylesheet = "";
  document.head.append(link);
})();
