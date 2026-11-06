// Load the real navigation dependencies into each isolated browser fixture.
import { readFile } from "node:fs/promises";
import vm from "node:vm";

export async function navigationModules() {
  const assets = new URL("../../../crates/rbit-web/assets/", import.meta.url);
  const modules = [
    ["repo-navigation-cache.js", "clearNavigationResponses, hasNavigationResponse, isCachedNavigationResponse, rememberNavigationResponse, reuseNavigationResponse, trackNavigationPreload"],
    ["repo-navigation-bar.js", "reconcileRepositoryBar"],
    ["update-notice.js", "installUpdateNotice"],
    ["render-time.js", "installRenderTiming"],
    ["nav.js", "navigationEpoch, navigationHeaders"],
  ];
  const sources = await Promise.all(modules.map(async ([name, exports]) => ({
    exports,
    source: (await readFile(new URL(name, assets), "utf8"))
      .replace(/^import .*;\r?\n/gm, "").replace(/^export /gm, ""),
  })));
  return context => {
    for (const { source, exports } of sources) {
      Object.assign(context, vm.runInContext(`(() => { ${source}\nreturn { ${exports} }; })()`, context));
    }
  };
}
