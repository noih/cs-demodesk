using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// Development-only Win32 observer. No window titles or other apps' input is collected.
public sealed class DemoDeskWindowObserver : IDisposable {
    public sealed class WindowEvent {
        public double seconds; public string kind, hwnd, windowClass;
        public uint pid; public bool visible; public int[] rect;
    }
    [StructLayout(LayoutKind.Sequential)] struct Rect { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] struct Point { public int x, y; }
    [StructLayout(LayoutKind.Sequential)] struct Msg {
        public IntPtr hwnd; public uint message; public UIntPtr wParam; public IntPtr lParam;
        public uint time; public Point pt; public uint privateValue;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct Entry {
        public uint size, usage, pid; public UIntPtr heap; public uint module, threads, parent;
        public int priority; public uint flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst=260)] public string exe;
    }
    delegate void EventProc(IntPtr hook, uint ev, IntPtr hwnd, int obj, int child, uint thread, uint time);
    delegate bool EnumProc(IntPtr hwnd, IntPtr param);
    [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(uint min, uint max, IntPtr module, EventProc cb, uint pid, uint tid, uint flags);
    [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr hook);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder text, int length);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr param);
    [DllImport("user32.dll")] static extern bool PeekMessage(out Msg msg, IntPtr hwnd, uint min, uint max, uint remove);
    [DllImport("user32.dll")] static extern int GetMessage(out Msg msg, IntPtr hwnd, uint min, uint max);
    [DllImport("user32.dll")] static extern bool TranslateMessage(ref Msg msg);
    [DllImport("user32.dll")] static extern IntPtr DispatchMessage(ref Msg msg);
    [DllImport("user32.dll")] static extern bool PostThreadMessage(uint id, uint msg, UIntPtr wp, IntPtr lp);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint pid);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] static extern bool Process32First(IntPtr snap, ref Entry entry);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] static extern bool Process32Next(IntPtr snap, ref Entry entry);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool QueryFullProcessImageName(IntPtr handle, uint flags, StringBuilder name, ref int size);
    public static int[] Children(int parent) {
        var result = new List<int>();
        IntPtr snap = CreateToolhelp32Snapshot(2, 0);
        if (snap == new IntPtr(-1)) throw new System.ComponentModel.Win32Exception();
        try {
            Entry entry = new Entry(); entry.size = (uint)Marshal.SizeOf(typeof(Entry));
            bool ok = Process32First(snap, ref entry);
            while (ok) {
                if (entry.parent == parent && String.Equals(entry.exe, "cs2.exe", StringComparison.OrdinalIgnoreCase)) result.Add((int)entry.pid);
                ok = Process32Next(snap, ref entry);
            }
        } finally { CloseHandle(snap); }
        return result.ToArray();
    }
    public static string Image(Process process) {
        var name = new StringBuilder(32768); int size = name.Capacity;
        if (!QueryFullProcessImageName(process.Handle, 0, name, ref size)) throw new System.ComponentModel.Win32Exception();
        return name.ToString();
    }
    readonly List<WindowEvent> events = new List<WindowEvent>();
    readonly Stopwatch clock = Stopwatch.StartNew();
    readonly ManualResetEvent ready = new ManualResetEvent(false);
    readonly Thread worker; readonly uint target; uint threadId;
    public string Error { get; private set; }
    public bool Truncated { get; private set; }
    void Observe(string kind, IntPtr hwnd) {
        uint pid; GetWindowThreadProcessId(hwnd, out pid);
        if (pid != target || GetAncestor(hwnd, 2) != hwnd) return;
        lock (events) {
            if (events.Count >= 5000) { Truncated = true; return; }
            var name = new StringBuilder(256); Rect rect;
            GetClassName(hwnd, name, name.Capacity); GetWindowRect(hwnd, out rect);
            events.Add(new WindowEvent { seconds=clock.Elapsed.TotalSeconds, kind=kind, hwnd=hwnd.ToString("X"),
                windowClass=name.ToString(), pid=pid, visible=IsWindowVisible(hwnd), rect=new int[]{rect.left,rect.top,rect.right,rect.bottom} });
        }
    }
    public WindowEvent[] Snapshot() { lock(events) return events.ToArray(); }
    public DemoDeskWindowObserver(int pid) {
        target = (uint)pid;
        worker = new Thread(Run); worker.IsBackground = true; worker.Start();
        if (!ready.WaitOne(5000)) throw new TimeoutException("Window observer did not start.");
        if (Error != null) throw new InvalidOperationException(Error);
    }
    void Run() {
        IntPtr foreground=IntPtr.Zero, windows=IntPtr.Zero;
        EventProc cb = delegate(IntPtr h,uint ev,IntPtr hwnd,int obj,int child,uint tid,uint tick) {
            try { if (hwnd != IntPtr.Zero && (ev == 3 || (obj == 0 && child == 0))) Observe("0x"+ev.ToString("X"),hwnd); }
            catch(Exception e) { Error=e.Message; }
        };
        try {
            Msg msg; PeekMessage(out msg,IntPtr.Zero,0,0,0); threadId=GetCurrentThreadId();
            foreground=SetWinEventHook(3,3,IntPtr.Zero,cb,target,0,0);
            windows=SetWinEventHook(0x8000,0x800B,IntPtr.Zero,cb,target,0,0);
            if(foreground==IntPtr.Zero || windows==IntPtr.Zero) throw new InvalidOperationException("SetWinEventHook failed.");
            EnumWindows(delegate(IntPtr hwnd,IntPtr p) { Observe("initial",hwnd); return true; },IntPtr.Zero);
            ready.Set();
            int result;
            while((result=GetMessage(out msg,IntPtr.Zero,0,0))>0) { TranslateMessage(ref msg); DispatchMessage(ref msg); }
            if(result<0) Error="GetMessage failed.";
        } catch(Exception e) { Error=e.ToString(); }
        finally {
            if(foreground!=IntPtr.Zero) UnhookWinEvent(foreground);
            if(windows!=IntPtr.Zero) UnhookWinEvent(windows);
            ready.Set(); GC.KeepAlive(cb);
        }
    }
    public void Dispose() {
        if(worker.IsAlive) {
            if(!PostThreadMessage(threadId,0x12,UIntPtr.Zero,IntPtr.Zero)) throw new InvalidOperationException("Cannot stop observer.");
            if(!worker.Join(5000)) throw new TimeoutException("Observer shutdown timed out.");
        }
        ready.Dispose();
    }
}

// Drain redirected loader output without allowing either buffer to grow indefinitely.
public sealed class DemoDeskLoaderLog {
    readonly StringBuilder output = new StringBuilder();
    readonly System.Threading.Tasks.Task stdout, stderr;
    public bool Truncated { get; private set; }
    public DemoDeskLoaderLog(Process process) {
        stdout=System.Threading.Tasks.Task.Factory.StartNew(delegate { Drain(process.StandardOutput,"stdout"); });
        stderr=System.Threading.Tasks.Task.Factory.StartNew(delegate { Drain(process.StandardError,"stderr"); });
    }
    void Drain(System.IO.StreamReader reader,string channel) {
        try {
            char[] buffer=new char[4096]; int count;
            while((count=reader.Read(buffer,0,buffer.Length))>0) {
                lock(output) {
                    int available=1024*1024-output.Length;
                    if(available>0) output.Append(buffer,0,Math.Min(available,count));
                    if(count>available) Truncated=true;
                }
            }
        } catch(System.IO.IOException) { }
    }
    public string Finish() {
        if(!System.Threading.Tasks.Task.WaitAll(new[]{stdout,stderr},2000)) Truncated=true;
        lock(output) return output.ToString();
    }
}
