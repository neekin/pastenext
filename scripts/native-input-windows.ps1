param([Parameter(Mandatory=$true)][string]$Action)
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NativeInput {
 [DllImport("user32.dll")] public static extern void keybd_event(byte key,byte scan,uint flags,UIntPtr extra);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string name,string title);
 // Keep null in C#: PowerShell converts a null string argument to an empty title.
 public static IntPtr SelectorWindow() { return FindWindow("PasteNextCaptureSelector",null); }
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
 public delegate bool EnumProc(IntPtr window,IntPtr data);
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc proc,IntPtr data);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window,StringBuilder value,int size);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window,StringBuilder value,int size);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
 [DllImport("user32.dll")] public static extern IntPtr GetProcessWindowStation();
 [DllImport("user32.dll")] public static extern IntPtr GetThreadDesktop(uint thread);
 [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern bool GetUserObjectInformation(IntPtr handle,int index,StringBuilder value,uint size,out uint needed);
 public static string ObjectName(IntPtr handle) { var b=new StringBuilder(256);uint n;GetUserObjectInformation(handle,2,b,512,out n);return b.ToString(); }
 public static string Windows() {
  var result=new StringBuilder();
  EnumWindows((h,d)=> {uint p;GetWindowThreadProcessId(h,out p);var c=new StringBuilder(256);var t=new StringBuilder(256);GetClassName(h,c,256);GetWindowText(h,t,256);result.AppendLine(h+" pid="+p+" class="+c+" title="+t);return true;},IntPtr.Zero);
  return "station="+ObjectName(GetProcessWindowStation())+" desktop="+ObjectName(GetThreadDesktop(GetCurrentThreadId()))+"\n"+result;
 }
}
'@
function Key([byte]$value) { [NativeInput]::keybd_event($value,0,0,[UIntPtr]::Zero); [NativeInput]::keybd_event($value,0,2,[UIntPtr]::Zero) }
function Shortcut([byte]$value) {
 [NativeInput]::keybd_event(0x11,0,0,[UIntPtr]::Zero);[NativeInput]::keybd_event(0x10,0,0,[UIntPtr]::Zero)
 Key $value
 [NativeInput]::keybd_event(0x10,0,2,[UIntPtr]::Zero);[NativeInput]::keybd_event(0x11,0,2,[UIntPtr]::Zero)
}
switch ($Action) {
 'diagnose' { [NativeInput]::Windows() }
 'probe' { [NativeInput]::SelectorWindow().ToInt64() }
 'capture' { Shortcut 0x41 }
 'record' { Shortcut 0x52 }
 'region' {
  $window=[NativeInput]::SelectorWindow();if($window -eq [IntPtr]::Zero){throw 'Selection window not found'}
  [void][NativeInput]::SetForegroundWindow($window);Key 0x31
  [void][NativeInput]::SetCursorPos(80,80);Start-Sleep -Milliseconds 60
  [NativeInput]::mouse_event(2,0,0,0,[UIntPtr]::Zero);Start-Sleep -Milliseconds 60
  [void][NativeInput]::SetCursorPos(260,200);Start-Sleep -Milliseconds 80
  [NativeInput]::mouse_event(4,0,0,0,[UIntPtr]::Zero);Start-Sleep -Milliseconds 60;Key 0x0d
 }
 'screen' {
  $window=[NativeInput]::SelectorWindow();if($window -eq [IntPtr]::Zero){throw 'Selection window not found'}
  [void][NativeInput]::SetForegroundWindow($window);Key 0x33;Key 0x0d
 }
 'cancel' { Key 0x1b }
 default { throw "Unknown native input action: $Action" }
}
