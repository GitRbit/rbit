// Developer-only Windows10+ process ownership. JOB_LIST associates the target atomically
// inside CreateProcess; even abrupt launcher death cannot strand an unowned target.
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
    private struct StartupEx { public Startup startup; public IntPtr attributes; }
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
        IntPtr threadAttributes, bool inherit, uint flags, IntPtr environment, string cwd, ref StartupEx startup, out ProcessInfo process);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool InitializeProcThreadAttributeList(IntPtr list, int count, uint flags, ref IntPtr size);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, UIntPtr attribute, IntPtr value, IntPtr size, IntPtr previous, IntPtr returned);
    [DllImport("kernel32.dll")] private static extern void DeleteProcThreadAttributeList(IntPtr list);
    [DllImport("kernel32.dll", SetLastError = true)] private static extern bool TerminateJobObject(IntPtr job, uint code);
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

    public static int Run(string executable, string[] args, string cwd, string stop, string proof, string token, string suspendedReady, string resumeGate) {
        IntPtr job = IntPtr.Zero;
        IntPtr attributes = IntPtr.Zero, jobValue = IntPtr.Zero;
        bool attributesReady = false;
        ProcessInfo process = new ProcessInfo();
        bool stopped = false;
        try {
            job = CreateJobObject(IntPtr.Zero, null);
            if (job == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            ExtendedLimits limits = new ExtendedLimits(); limits.basic.flags = 0x2000; // KILL_ON_JOB_CLOSE
            Require(SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf(typeof(ExtendedLimits))));
            IntPtr attributeSize = IntPtr.Zero;
            InitializeProcThreadAttributeList(IntPtr.Zero, 1, 0, ref attributeSize);
            attributes = Marshal.AllocHGlobal(attributeSize);
            Require(InitializeProcThreadAttributeList(attributes, 1, 0, ref attributeSize)); attributesReady = true;
            jobValue = Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(jobValue, job);
            Require(UpdateProcThreadAttribute(attributes, 0, new UIntPtr(0x0002000D), jobValue,
                new IntPtr(IntPtr.Size), IntPtr.Zero, IntPtr.Zero)); // PROC_THREAD_ATTRIBUTE_JOB_LIST
            StartupEx startup = new StartupEx(); startup.attributes = attributes;
            startup.startup.cb = Marshal.SizeOf(typeof(StartupEx)); startup.startup.flags = 0x100;
            startup.startup.input = GetStdHandle(-10); startup.startup.output = GetStdHandle(-11); startup.startup.error = GetStdHandle(-12);
            foreach (IntPtr handle in new IntPtr[] { startup.startup.input, startup.startup.output, startup.startup.error }) {
                if (handle != IntPtr.Zero && handle != new IntPtr(-1)) Require(SetHandleInformation(handle, 1, 1));
            }
            StringBuilder command = new StringBuilder(Quote(executable));
            foreach (string arg in args) command.Append(" ").Append(Quote(arg));
            Require(CreateProcess(executable, command, IntPtr.Zero, IntPtr.Zero, true,
                0x08080004, IntPtr.Zero, cwd, ref startup, out process)); // NO_WINDOW | EXTENDED_STARTUPINFO | SUSPENDED
            // Explicit deterministic test fence; normal launcher calls leave both empty.
            if (!String.IsNullOrEmpty(suspendedReady)) {
                File.WriteAllText(suspendedReady, process.pid.ToString());
                while (!File.Exists(resumeGate) && !File.Exists(stop)) Thread.Sleep(50);
            }
            if (ResumeThread(process.thread) == UInt32.MaxValue) throw new Win32Exception(Marshal.GetLastWin32Error());
            uint waitResult;
            while ((waitResult = WaitForSingleObject(process.process, 100)) == 258 && !File.Exists(stop)) { }
            if (waitResult == UInt32.MaxValue) throw new Win32Exception(Marshal.GetLastWin32Error());
            if (waitResult != 0 && waitResult != 258) throw new IOException("Unexpected owned process wait result");
            uint code; Require(GetExitCodeProcess(process.process, out code));
            // A supervisor exit is not evidence that workers exited. Stop the entire job.
            Require(TerminateJobObject(job, 0));
            stopped = WaitEmpty(job);
            if (!stopped) throw new IOException("Owned process tree did not stop");
            // A signaled process may legitimately exit 259; it is no longer STILL_ACTIVE.
            // Preserve the existing successful stop result for an unsignaled target.
            return code == 259 && waitResult != 0 ? 0 : unchecked((int)code);
        } finally {
            // Every created target is already in the job; no post-creation assignment gap.
            if (!stopped && process.process != IntPtr.Zero) {
                TerminateJobObject(job, 125); stopped = WaitEmpty(job);
            }
            if (process.process == IntPtr.Zero) stopped = true;
            if (process.thread != IntPtr.Zero) CloseHandle(process.thread);
            if (process.process != IntPtr.Zero) CloseHandle(process.process);
            if (attributesReady) DeleteProcThreadAttributeList(attributes);
            if (attributes != IntPtr.Zero) Marshal.FreeHGlobal(attributes);
            if (jobValue != IntPtr.Zero) Marshal.FreeHGlobal(jobValue);
            if (job != IntPtr.Zero) CloseHandle(job);
            if (stopped) File.WriteAllText(proof, token); // only verified-empty jobs acknowledge cleanup
        }
    }
}
