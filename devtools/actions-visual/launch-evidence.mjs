// Only allowlisted categories/numbers leave memory; never return log excerpts.
const signals = new Set(["SIGABRT", "SIGBUS", "SIGFPE", "SIGHUP", "SIGILL", "SIGINT",
  "SIGKILL", "SIGPIPE", "SIGQUIT", "SIGSEGV", "SIGTERM", "SIGTRAP", "SIGSYS"]);
const sources = new Map([
  ["platform_thread_posix.cc", "THREAD_PLATFORM"], ["thread.cc", "THREAD"],
  ["thread_pool_instance.cc", "THREAD_POOL"], ["zygote_host_impl_linux.cc", "ZYGOTE_HOST"],
  ["zygote_linux.cc", "ZYGOTE"], ["sandbox_linux.cc", "SANDBOX_LINUX"],
  ["credentials.cc", "SANDBOX_CREDENTIALS"], ["process_singleton_posix.cc", "PROFILE_SINGLETON"],
  ["crashpad_client_linux.cc", "CRASHPAD"], ["chrome_browser_main_linux.cc", "BROWSER_MAIN_LINUX"],
  ["chrome_main_delegate.cc", "BROWSER_MAIN_DELEGATE"], ["gpu_process_host.cc", "GPU_HOST"], ["bus.cc", "DBUS"],
]);
const reasons = [
  ["THREAD_CREATE_EPERM", /pthread_create[^\r\n]{0,256}(?:operation not permitted|EPERM)/i],
  ["THREAD_CREATE_EAGAIN", /pthread_create[^\r\n]{0,256}(?:resource temporarily unavailable|EAGAIN)/i],
  ["CLONE3_EPERM", /clone3[^\r\n]{0,256}(?:operation not permitted|EPERM)/i],
  ["SPAWN_EPERM", /posix_spawn[^\r\n]{0,256}(?:operation not permitted|EPERM)/i],
  ["NAMESPACE_DENIED", /(?:new namespace|unshare)[^\r\n]{0,512}(?:operation not permitted|permission denied|EPERM)/i],
  ["NO_USABLE_SANDBOX", /no usable sandbox/i],
  ["SUID_HELPER_INVALID", /SUID sandbox helper[^\r\n]{0,256}not configured correctly/i],
  ["ZYGOTE_PREMATURE_EXIT", /zygote process.{0,32}prematurely/i],
  ["ZYGOTE_BOOT_FAILED", /boot message.{0,32}zygote/i],
  ["ZYGOTE_HELLO_FAILED", /hello message.{0,32}zygote/i],
  ["ZYGOTE_LAUNCH_FAILED", /launch zygote process/i],
  ["ZYGOTE_PID_INVALID", /invalid process ID.{0,32}zygote/i],
  ["SUID_HELPER_NOT_ROOT", /setuid sandbox.{0,32}not running as root/i],
  ["SUID_NO_NEW_PRIVILEGES", /(?:parent process|setuid sandbox).{0,128}PR_SET_NO_NEW_PRIVS/i],
  ["ZYGOTE_FORK_FAILED", /zygote[^\r\n]{0,128}(?:could not fork|failed to fork)/i],
  ["PROFILE_SINGLETON_FAILED", /failed to create a ProcessSingleton/i],
  ["PROFILE_CREATE_FAILED", /failed to create[^\r\n]{0,128}(?:user data directory|SingletonLock)/i],
  ["CRASHPAD_DATABASE_REQUIRED", /chrome_crashpad_handler:[^\r\n]{0,128}database is required/i],
  ["READONLY_FILESYSTEM", /read-only file system/i],
  ["LIBRARY_LOAD_FAILED", /error while loading shared libraries/i],
  ["DISK_FULL", /no space left on device/i],
  ["DBUS_CONNECTION_FAILED", /failed to connect to the bus/i],
];
export function launchEvidence(error) {
  const message = typeof error?.message === "string" ? error.message : "";
  const bounded = message.length <= 131072 ? message : message.slice(0, 65536) + "\n" + message.slice(-65536);
  let exit = { parsed: false };
  for (const match of bounded.matchAll(/<process did exit: exitCode=(null|\d{1,3}), signal=(null|SIG[A-Z0-9]+)>/g)) {
    const code = match[1] === "null" ? null : Number(match[1]);
    const signal = match[2] === "null" ? null : match[2];
    if ((code === null || code <= 255) && (signal === null || signals.has(signal))) {
      exit = { parsed: true, code, signal };
    }
  }
  const seenSources = new Set(), seenReasons = new Set(), locations = new Map();
  let zygoteExit = null;
  const stderr = bounded.split(/\r?\n/).filter(line => /\[pid=\d+\]\[err\]/.test(line));
  for (const line of stderr) {
    for (const [file, category] of sources) {
      const location = line.match(new RegExp(`(?:^|[\\/:\\s])${file.replaceAll(".", "\\.")}:(\\d{1,6})\\]`));
      if (location) {
        seenSources.add(category);
        const severity = line.match(/:(INFO|WARNING|ERROR|FATAL):/)?.[1] ?? null;
        const number = Number(location[1]);
        if (locations.size < 32) locations.set(`${category}:${number}:${severity}`, { source: category, line: number, severity });
      }
    }
    for (const [reason, pattern] of reasons) if (pattern.test(line)) seenReasons.add(reason);
    const child = line.match(/zygote process.{0,32}prematurely.{0,32}exit code (\d{1,3}) \((namespace|setuid) sandbox\b/i);
    if (child && Number(child[1]) <= 255) zygoteExit = { code: Number(child[1]), sandbox: child[2].toLowerCase() };
  }
  return { exit, stderrSources: [...seenSources].sort(), stderrReasons: [...seenReasons].sort(),
    stderrLinesObserved: stderr.length, stderrLocations: [...locations.values()], zygoteExit };
}
