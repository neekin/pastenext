param([Parameter(Mandatory=$true)][string]$Action)
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NativeInput {
 [DllImport("user32.dll")] public static extern void keybd_event(byte key,byte scan,uint flags,UIntPtr extra);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string name,string title);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
}
'@
function Key([byte]$value) { [NativeInput]::keybd_event($value,0,0,[UIntPtr]::Zero); [NativeInput]::keybd_event($value,0,2,[UIntPtr]::Zero) }
function Shortcut([byte]$value) {
 [NativeInput]::keybd_event(0x11,0,0,[UIntPtr]::Zero);[NativeInput]::keybd_event(0x10,0,0,[UIntPtr]::Zero)
 Key $value
 [NativeInput]::keybd_event(0x10,0,2,[UIntPtr]::Zero);[NativeInput]::keybd_event(0x11,0,2,[UIntPtr]::Zero)
}
switch ($Action) {
 'capture' { Shortcut 0x41 }
 'record' { Shortcut 0x52 }
 'region' {
  $window=[NativeInput]::FindWindow($null,'PasteNext Capture Selection');if($window -eq [IntPtr]::Zero){throw 'Selection window not found'}
  [void][NativeInput]::SetForegroundWindow($window);Key 0x31
  [void][NativeInput]::SetCursorPos(80,80);Start-Sleep -Milliseconds 60
  [NativeInput]::mouse_event(2,0,0,0,[UIntPtr]::Zero);Start-Sleep -Milliseconds 60
  [void][NativeInput]::SetCursorPos(260,200);Start-Sleep -Milliseconds 80
  [NativeInput]::mouse_event(4,0,0,0,[UIntPtr]::Zero);Start-Sleep -Milliseconds 60;Key 0x0d
 }
 'screen' {
  $window=[NativeInput]::FindWindow($null,'PasteNext Capture Selection');if($window -eq [IntPtr]::Zero){throw 'Selection window not found'}
  [void][NativeInput]::SetForegroundWindow($window);Key 0x33;Key 0x0d
 }
 'cancel' { Key 0x1b }
 default { throw "Unknown native input action: $Action" }
}
