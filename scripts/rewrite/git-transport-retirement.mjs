/** Retired Git transport decision paths must not return; see docs/design/rewrite/repositories-git-transport-acceptance.md. */

const TRANSPORT = "crates/rbit-transport/src/";
const GIT_HTTP = /^crates\/rbit-web\/src\/routes\/git_http(?:\.rs|\/)/u;
// The smart HTTP access adapter only opens repositories for the transport's gate.
const GIT_HTTP_ADAPTER = "crates/rbit-web/src/routes/git_http/repositories.rs";
const SSH = "crates/rbit-ssh/src/";
// The SSH access module owns per-command authorization and its services adapter.
const SSH_ACCESS = /^crates\/rbit-ssh\/src\/access(?:\.rs|\/)/u;

const RULES = [
  // Protocol framing shared by both transports.
  { where: (path) => !path.startsWith(TRANSPORT), pattern: /"ERR \{|format!\("\{:04x\}/u,
    message: "ERR packet framing belongs to rbit_transport::pkt_line::error" },
  { where: (path) => !path.startsWith(TRANSPORT), pattern: /\brefusal_message\s*\(|\breceive::refusal\s*\(/u,
    message: "refused-push answers belong to rbit_transport::refused_push" },
  { where: (path) => !path.startsWith(TRANSPORT), pattern: /dumb HTTP protocol is not supported|"git-protocol"/u,
    message: "smart HTTP protocol values belong to rbit_transport::smart_http" },
  // Smart HTTP decisions in the web routes.
  { where: (path) => GIT_HTTP.test(path), pattern: /\bSTANDARD\s*\.\s*decode\b|strip_prefix\("Basic |Basic realm=|eq_ignore_ascii_case\("bearer"\)/u,
    message: "Git credential parsing and challenges belong to rbit_transport::smart_http::credentials" },
  { where: (path) => GIT_HTTP.test(path), pattern: /eq_ignore_ascii_case\(b"gzip"\)|"no-cache"|"repository not found"|"you do not have permission to push to this repository"/u,
    message: "smart HTTP protocol values belong to rbit_transport::smart_http" },
  { where: (path) => GIT_HTTP.test(path) && path !== GIT_HTTP_ADAPTER,
    pattern: /\bopen_following_renames\b|\bopen_for_task\b|\bis_mirror\b|\bscope\s*\.\s*limit\b/u,
    message: "Git HTTP access decisions belong to rbit_transport::smart_http::access; open repositories in the RepositoryGate adapter" },
  // SSH per-command authorization.
  { where: (path) => path.startsWith(SSH) && !SSH_ACCESS.test(path),
    pattern: /\bopen_following_renames\b|\bopen_for_deploy_key\b|\bis_mirror\b|\bis_suspended\b|\bfn\s+same_key_holder\b/u,
    message: "SSH per-command authorization belongs to rbit_ssh::access" },
  { where: (path) => path.startsWith(SSH) && !SSH_ACCESS.test(path),
    pattern: /not found, or you do not have access to it|deploy key is read-only|you do not have permission to push to|no longer authorized; reconnect/u,
    message: "SSH authorization text belongs to rbit_ssh::access" },
];

/** Comments and the trailing inline test module (which asserts the bytes) are not checked. */
function code(source) {
  const inlineTests = source.search(/#\[cfg\(test\)\]\s*mod\s+tests\s*\{/u);
  const body = inlineTests < 0 ? source : source.slice(0, inlineTests);
  return body.replace(/\/\*[\s\S]*?\*\//gu, "").replace(/\/\/[^\n]*/gu, "");
}

/** Production code only: suites assert the protocol bytes and refusal text. */
function production(path) {
  return /^crates\/[^/]+\/src\//u.test(path) && path.endsWith(".rs") && !/(?:^|\/)tests(?:\.rs|\/)/u.test(path.slice(path.indexOf("/src/")));
}

/** Takes authored [path, source] pairs; returns violations naming each path and rule. */
export function checkGitTransportRetirement(sources) {
  const violations = [];
  for (const [path, source] of sources) {
    if (!production(path)) continue;
    const text = code(source);
    for (const rule of RULES) {
      if (rule.where(path) && rule.pattern.test(text)) violations.push(`${path}: ${rule.message}`);
    }
  }
  return violations;
}
