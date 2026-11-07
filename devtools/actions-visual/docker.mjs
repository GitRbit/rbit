import { spawn } from "node:child_process";
export function dockerClient(spawnProcess = spawn) {
  const active = new Set();
  function docker(args, timeout = 15000) {
    return new Promise((done, reject) => {
      const child = spawnProcess("docker", args, { stdio: ["ignore", "pipe", "inherit"], windowsHide: true });
      active.add(child);
      let output = "", escalation, oversized = false, processError;
      child.stdout.on("data", bytes => {
        if (output.length + bytes.length > 65536) { oversized = true; child.kill("SIGKILL"); }
        else output += bytes.toString("utf8");
      });
      const timer = setTimeout(() => {
        child.kill("SIGTERM");
        escalation = setTimeout(() => child.kill("SIGKILL"), 1000);
      }, timeout);
      const clean = () => { active.delete(child); clearTimeout(timer); clearTimeout(escalation); };
      // An error alone is not proof that an already-started client has closed.
      child.once("error", error => { processError = error; });
      child.once("close", (code, signal) => {
        clean();
        if (processError || code !== 0 || signal || oversized) reject(new Error("DOCKER_COMMAND_FAILED")); else done(output.trim());
      });
    });
  }
  async function interruptDocker() {
    await Promise.all([...active].map(child => new Promise(done => {
      child.once("close", done);
      child.kill("SIGTERM");
      const timer = setTimeout(() => child.kill("SIGKILL"), 1000);
      child.once("close", () => clearTimeout(timer));
    })));
  }
  return { docker, interruptDocker };
}
export const { docker, interruptDocker } = dockerClient();
