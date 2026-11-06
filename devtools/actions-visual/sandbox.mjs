import { sandboxProof, browserVersion } from "./policy.mjs";
// This internal diagnostic is not a network navigation or screenshot fixture.
export async function verifySandbox(browser) {
  if (browser.version() !== browserVersion) throw new Error("BROWSER_VERSION_MISMATCH");
  const session = await browser.newBrowserCDPSession();
  const { arguments: args } = await session.send("Browser.getBrowserCommandLine");
  await session.detach();
  const context = await browser.newContext({ serviceWorkers: "block" });
  try {
    const page = await context.newPage();
    await page.goto("chrome://sandbox");
    const rows = await page.locator("tr").evaluateAll(nodes => nodes.map(node => {
      const cells = [...node.querySelectorAll("td,th")].map(cell => cell.textContent.trim());
      return { label: cells[0] ?? "", value: cells[1] ?? "" };
    }));
    return sandboxProof({ uid: process.getuid?.(), arguments: args, rows });
  } finally { await context.close(); }
}
