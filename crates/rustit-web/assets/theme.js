// Applies the stored colour-scheme choice before first paint (loaded synchronously in <head>).
// Without a stored choice the CSS follows the operating system via `light-dark()`.
(() => {
  try {
    const theme = localStorage.getItem("rustit-theme");
    if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
  } catch {
    /* storage unavailable: fall back to the system preference */
  }
})();
