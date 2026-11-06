// Opt-in identical benchmark settings without inheriting the user's application config.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

export function serverEnvironment(options, inherited = process.env) {
  if (!options.isolated) return {
    ...inherited, RBIT_SERVER__BIND:`127.0.0.1:${options.port}`, RBIT_SERVER__PUBLIC_URL:options.baseUrl,
    RBIT_STORAGE__DATA_DIR:options.dataDir, RBIT_MAIL__TRANSPORT:"log", RBIT_SSH__ENABLED:inherited.RBIT_SSH__ENABLED ?? "false",
  };
  if (["rbit.toml", "rustit.toml"].some(file=>existsSync(join(options.sourceDir,file)))) throw new Error("isolated benchmark source contains a private application config");
  const clean=Object.fromEntries(Object.entries(inherited).filter(([key])=>!/^RBIT_/i.test(key)));
  const tokenFile=inherited.RBIT_PERF_SETUP_TOKEN_FILE;
  if (!tokenFile) throw new Error("isolated benchmark requires a private setup token file");
  const token=tokenFile ? readFileSync(tokenFile,"utf8").trim() : null;
  if (tokenFile && (!token || token.length>1024 || /[\r\n]/.test(token))) throw new Error("invalid private benchmark setup token");
  return {
    ...clean, RUST_LOG:"warn", RBIT_SERVER__BIND:`127.0.0.1:${options.port}`,
    RBIT_SERVER__PUBLIC_URL:options.baseUrl, RBIT_STORAGE__DATA_DIR:options.dataDir,
    RBIT_MAIL__TRANSPORT:"log", RBIT_SSH__ENABLED:"false", RBIT_ACTIONS__ENABLED:"false",
    RBIT_UPDATES__CHECK_FOR_UPDATES:"false", RBIT_SECURITY__RATE_LIMIT:"true",
    ...(token ? { RBIT_SECURITY__SETUP_TOKEN:token } : {}),
  };
}
