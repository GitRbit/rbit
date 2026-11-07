// Helper controls belong to the management process, not Cargo/build scripts/tests/apps.
// Keep real application configuration and compiler/toolchain environment untouched.
export function childEnvironment(input) {
  return Object.fromEntries(Object.entries(input).filter(([name]) => !/^RBIT_(BUILD|WORKSPACE)_/i.test(name)));
}
