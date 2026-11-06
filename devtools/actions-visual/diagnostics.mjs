// Inspect launch diagnostics only in memory; retain fixed categories, never text.
const known = new Set([
  "SANDBOX_ROOT_USER", "SANDBOX_DISABLED_FLAGS", "SANDBOX_UNPROVED",
  "SANDBOX_CHECK_FAILED",
  "CONTAINER_CONTROLS_UNPROVED", "INITIAL_NAMESPACE_CHROOT_UNPROVED", "CHILD_NAMESPACE_CHROOT_UNPROVED",
  "BROWSER_VERSION_MISMATCH", "LINUX_NONROOT_REQUIRED", "OWNED_HOME_NOT_READY", "BUNDLED_EXECUTABLE_NOT_READY",
]);
const launch = [
  ["BROWSER_NAMESPACE_DENIED", /(?:failed to move to new namespace|failed to unshare|unshare.*failed)[\s\S]{0,1024}(?:operation not permitted|permission denied|EPERM)/i],
  ["BROWSER_NO_USABLE_SANDBOX", /no usable sandbox/i],
  ["BROWSER_ROOT_SANDBOX_REJECTED", /running as root without.*sandbox/i],
  ["BROWSER_EXECUTABLE_MISSING", /executable (?:doesn't|does not) exist|browser executable.*not found/i],
  ["BROWSER_LIBRARIES_MISSING", /error while loading shared libraries|host system is missing dependencies/i],
  ["BROWSER_READONLY_PATH", /read-only file system/i],
  ["BROWSER_DISPLAY_UNAVAILABLE", /missing X server|cannot open display/i],
  ["BROWSER_LAUNCH_TIMEOUT", /timeout \d+ms exceeded|browser.*launch.*timed out/i],
  ["BROWSER_SIGNAL_TRAP", /signal=SIGTRAP|signal SIGTRAP/i],
  ["BROWSER_ZYGOTE_PREMATURE_EXIT", /zygote process.{0,32}prematurely/i],
  ["BROWSER_ZYGOTE_BOOT_FAILED", /boot message.{0,32}zygote/i],
  ["BROWSER_ZYGOTE_HELLO_FAILED", /hello message.{0,32}zygote/i],
  ["BROWSER_ZYGOTE_LAUNCH_FAILED", /launch zygote process/i],
  ["BROWSER_EXITED_BEFORE_READY", /target page, context or browser has been closed|browser closed/i],
];
export function setupFailure(stage, error) {
  const message = typeof error?.message === "string" ? error.message : "";
  if (known.has(message)) return message;
  if (stage === "browser-launch") {
    // Bound classifier work without retaining either excerpt in the report.
    const bounded = message.slice(0, 65536) + message.slice(-65536);
    for (const [code, pattern] of launch) if (pattern.test(bounded)) return code;
  }
  return "SETUP_FAILED";
}
