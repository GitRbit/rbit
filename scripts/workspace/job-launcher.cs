// Developer-only Windows process ownership. The suspended target cannot escape its job
// before assignment. No breakaway is allowed; server restart workers inherit the job.
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using System.IO;
using System.Threading;

public static class WorkspaceJobLauncher {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct Startup {
        public int cb; public string reserved, desktop, title;
        public uint x, y, width, height, charsX, charsY, fill, flags;
        public short show, reservedSize; public IntPtr reservedBytes, input, output, error;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct ProcessInfo { public IntPtr process, thread; public uint pid, threadId; }
    [StructLayout(LayoutKind.Sequential)]
    private struct BasicLimits {
        public long processTime, jobTime; public uint flags;
        public UIntPtr minWorkingSet, maxWorkingSet; public uint activeLimit;
        public UIntPtr affinity; public uint priority, scheduling;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct IoCounters { public ulong readOps, writeOps, otherOps, readBytes, writeBytes, otherBytes; }
    [StructLayout(LayoutKind.Sequential)]
    private struct ExtendedLimits {
        public BasicLimits basic; public IoCounters io;
        public UIntPtr processMemory, jobMemory, peakProcessMemory, peakJobMemory;
    }
    [StructLayout(LayoutKind.Sequential)]
    private struct Accounting {
        public long userTime, kernelTime, periodUserTime, periodKernelTime;
        public uint faults, total, active, terminated;
    }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateJobObject(IntPtr attributes, string name);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetInformationJobObject(IntPtr job, int infoClass, ref ExtendedLimits info, uint size);
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool QueryInformationJobObject(IntPtr job, int infoClass, out Accounting info, uint size, IntPtr returned);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CreateProcess(string executable, StringBuilder command, IntPtr processAttributes,
        IntPtr threadAttributes, bool inherit, uint flags, IntPtr environment, string cwd, ref Startup startup, out ProcessInfo process);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool TerminateJobObject(IntPtr job, uint code);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool TerminateProcess(IntPtr process, uint code);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern uint WaitForSingleObject(IntPtr handle, uint timeout);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool GetExitCodeProcess(IntPtr process, out uint code);
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] private static extern IntPtr GetStdHandle(int kind);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);

    private static void Require(bool success) { if (!success) throw new Win32Exception(Marshal.GetLastWin32Error()); }

    private static string Quote(string value) {
        StringBuilder result = new StringBuilder("\"");
        int slashes = 0;
        foreach (char c in value) {
            if (c == '\\') { slashes++; continue; }
            result.Append('\\', c == '"' ? slashes * 2 + 1 : slashes);
            result.Append(c); slashes = 0;
        }
        result.Append('\\', slashes * 2); result.Append('"');
        return result.ToString();
    }

    private static bool WaitEmpty(IntPtr job) {
        for (int attempt = 0; attempt < 100; attempt++) {
            Accounting accounting;
            Require(QueryInformationJobObject(job, 1, out accounting, (uint)Marshal.SizeOf(typeof(Accounting)), IntPtr.Zero));
            if (accounting.active == 0) return true;
            Thread.Sleep(100);
        }
        return false;
    }

    public static int Run(string executable, string[] args, string cwd, string stop, string proof, string token) {
        IntPtr job = IntPtr.Zero;
        ProcessInfo process = new ProcessInfo();
        bool assigned = false, stopped = false;
        try {
            job = CreateJobObject(IntPtr.Zero, null);
            if (job == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            ExtendedLimits limits = new ExtendedLimits(); limits.basic.flags = 0x2000; // KILL_ON_JOB_CLOSE
            Require(SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf(typeof(ExtendedLimits))));
            Startup startup = new Startup(); startup.cb = Marshal.SizeOf(typeof(Startup)); startup.flags = 0x100;
            startup.input = GetStdHandle(-10); startup.output = GetStdHandle(-11); startup.error = GetStdHandle(-12);
            foreach (IntPtr handle in new IntPtr[] { startup.input, startup.output, startup.error }) {
                if (handle != IntPtr.Zero && handle != new IntPtr(-1)) Require(SetHandleInformation(handle, 1, 1));
            }
            StringBuilder command = new StringBuilder(Quote(executable));
            foreach (string arg in args) command.Append(" ").Append(Quote(arg));
            Require(CreateProcess(executable, command, IntPtr.Zero, IntPtr.Zero, true,
                0x08000004, IntPtr.Zero, cwd, ref startup, out process)); // NO_WINDOW | SUSPENDED
            Require(AssignProcessToJobObject(job, process.process)); assigned = true;
            if (ResumeThread(process.thread) == UInt32.MaxValue) throw new Win32Exception(Marshal.GetLastWin32Error());
            while (WaitForSingleObject(process.process, 100) == 258 && !File.Exists(stop)) { }
            uint code; Require(GetExitCodeProcess(process.process, out code));
            // A supervisor exit is not evidence that workers exited. Stop the entire job.
            Require(TerminateJobObject(job, 0));
            stopped = WaitEmpty(job);
            if (!stopped) throw new IOException("Owned process tree did not stop");
            return code == 259 ? 0 : unchecked((int)code);
        } finally {
            // Assignment failure must never leave an unowned suspended child behind.
            if (!stopped && process.process != IntPtr.Zero) {
                if (assigned) { TerminateJobObject(job, 125); stopped = WaitEmpty(job); }
                else { stopped = TerminateProcess(process.process, 125) && WaitForSingleObject(process.process, 10000) == 0; }
            }
            if (process.process == IntPtr.Zero) stopped = true;
            if (process.thread != IntPtr.Zero) CloseHandle(process.thread);
            if (process.process != IntPtr.Zero) CloseHandle(process.process);
            if (job != IntPtr.Zero) CloseHandle(job);
            if (stopped) File.WriteAllText(proof, token); // only verified-empty jobs acknowledge cleanup
        }
    }
}
