# End-to-end check of kbswitch: starts the exe, opens a window with a plain multiline Edit control (what classic
# Notepad uses; Windows 11 Notepad only adds a tab to a running instance and can't be focused from here), types with
# real unmarked input, converts, reads the text back and stops the exe. Run from WSL:
#   powershell.exe -ExecutionPolicy Bypass -File "$(wslpath -w scripts/e2e-notepad.ps1)"
# Don't touch the keyboard or mouse while it runs (about 15 s). A running kbswitch is stopped first.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\x86_64-pc-windows-gnu\release\kbswitch.exe'),
    # progress and results are also appended here line by line (the WSL pipe loses output when a run is killed)
    [string]$LogFile = (Join-Path $PSScriptRoot '..\var\e2e.log'),
    # 'Controls', 'Notepad' or 'All'
    [string]$Only = 'All'
)
$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Native {
    [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit, Size = 40)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public KEYBDINPUT ki; [FieldOffset(8)] public MOUSEINPUT mi; }
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    public static void Mouse(uint flags) {
        INPUT i = new INPUT(); i.type = 0; i.mi.dwFlags = flags;
        SendInput(1, new INPUT[] { i }, Marshal.SizeOf(typeof(INPUT)));
    }
    [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    public static void Key(ushort vk, ushort scan, bool up) {
        INPUT i = new INPUT(); i.type = 1; i.ki.wVk = vk; i.ki.wScan = scan;
        i.ki.dwFlags = (uint)((scan != 0 ? 8 : 0) | (up ? 2 : 0) | (vk == 0x24 || vk == 0x25 ? 1 : 0));
        SendInput(1, new INPUT[] { i }, Marshal.SizeOf(typeof(INPUT)));
    }
}
'@

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
function Wait([int]$ms) {
    $end = (Get-Date).AddMilliseconds($ms)
    while ((Get-Date) -lt $end) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 10 }
}
function Tap([int]$vk, [int]$scan = 0, [int]$holdMs = 30) {
    [Native]::Key($vk, $scan, $false); Wait $holdMs; [Native]::Key($vk, $scan, $true)
}
function Chord([int]$mod, [int]$vk) {
    [Native]::Key($mod, 0, $false); Wait 30; Tap $vk; [Native]::Key($mod, 0, $true); Wait 100
}
# scan codes of the US keys g h b d t n, so the active layout decides the characters
$scans = @{ g = 0x22; h = 0x23; b = 0x30; d = 0x20; t = 0x14; n = 0x31; v = 0x2F; c = 0x2E; ' ' = 0x39 }
function TypeKeys([string]$text) {
    foreach ($c in $text.ToCharArray()) { Tap 0 $scans[[string]$c]; Wait 60 }
    Wait 300
}
function DoubleShift { Tap 0xA0; Wait 100; Tap 0xA0; Wait 1500 }
function PauseKey { Tap 0x13; Wait 1500 }
function ShiftPause {
    [Native]::Key(0xA0, 0, $false); Wait 50; Tap 0x13; Wait 50
    [Native]::Key(0xA0, 0, $true); Wait 1500
}
function SelectLeft([int]$n) { [Native]::Key(0xA0, 0, $false); Wait 30; foreach ($i in 1..$n) { Tap 0x25; Wait 20 }; [Native]::Key(0xA0, 0, $true); Wait 200 }
# unmarked mouse input, so kbswitch's mouse hook sees the clicks like the user's
function MouseAt([int]$x, [int]$y) { [Native]::SetCursorPos($x, $y) | Out-Null; Wait 50 }
function Click { [Native]::Mouse(2); Wait 40; [Native]::Mouse(4); Wait 60 }   # LEFTDOWN, LEFTUP
function DoubleClickWord($box) { $p = $box.PointToScreen([System.Drawing.Point]::new(12, 14)); MouseAt $p.X $p.Y; Click; Click; Wait 300 }
function DragSelect($box) {
    $a = $box.PointToScreen([System.Drawing.Point]::new(2, 14)); $b = $box.PointToScreen([System.Drawing.Point]::new(90, 14))
    MouseAt $a.X $a.Y; [Native]::Mouse(2); Wait 60; MouseAt (($a.X + $b.X) / 2) $a.Y; MouseAt $b.X $b.Y; Wait 60; [Native]::Mouse(4); Wait 300
}
function SelectLine { [Native]::Key(0xA0, 0, $false); Wait 30; Tap 0x24; [Native]::Key(0xA0, 0, $true); Wait 200 }
function ReadAll {
    [System.Windows.Forms.Application]::DoEvents()
    if ($box -is [System.Windows.Forms.TextBoxBase]) { return $box.Text.Trim() }
    Set-Clipboard -Value 'E2E-EMPTY'  # an empty document copies nothing
    Chord 0x11 0x41; Chord 0x11 0x43; Wait 200; Tap 0x23; Wait 100  # Notepad: copy all, caret back to the end
    $t = (Get-Clipboard -Raw).Trim(); if ($t -eq 'E2E-EMPTY') { '' } else { $t }
}
# clears the control; the Escape is a real key, so kbswitch resets its phrase like after Enter or a click
function ClearAll { if ($box -is [System.Windows.Forms.TextBoxBase]) { $box.Clear() } else { Chord 0x11 0x41; Tap 0x2E }; Tap 0x1B; Wait 200 }
function ReleaseModifiers { foreach ($vk in 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5) { [Native]::Key($vk, 0, $true) } }
function Out([string]$line) { $line; if ($LogFile) { Add-Content -Encoding UTF8 $LogFile $line } }
function Expect([string]$name, [string]$got, [string[]]$ok) {
    if ($ok -contains $got) { Out "PASS  $name`: '$got'" } else { Out "FAIL  $name`: got '$got', expected $($ok -join ' or ')"; $script:failed = $true }
}

$script:failed = $false
if ($LogFile) { New-Item -ItemType Directory -Force (Split-Path $LogFile) | Out-Null; Set-Content -Encoding UTF8 $LogFile "e2e $(Get-Date -Format s)" }
ReleaseModifiers  # a killed earlier run may have left a Shift down
Get-Process kbswitch -ErrorAction SilentlyContinue | Stop-Process -Force
Wait 300
$local = Join-Path $env:LOCALAPPDATA 'kbswitch\e2e\kbswitch.exe'
New-Item -ItemType Directory -Force (Split-Path $local) | Out-Null
Copy-Item $Exe $local -Force; Unblock-File $local  # a local, unblocked copy: no security prompt
$appLog = Join-Path (Split-Path $local) 'kbswitch.log'  # a debug exe prints its debug_log! lines there
Remove-Item $appLog -ErrorAction SilentlyContinue
$app = Start-Process -FilePath $local -PassThru -RedirectStandardError $appLog -WindowStyle Hidden
Start-Sleep -Seconds 1
$form = New-Object System.Windows.Forms.Form
$form.Text = 'kbswitch e2e'; $form.Width = 500; $form.Height = 300; $form.TopMost = $true
$edit = New-Object System.Windows.Forms.TextBox
$edit.Multiline = $true; $edit.Dock = 'Top'; $edit.Height = 110; $edit.Font = New-Object System.Drawing.Font('Consolas', 14)
$rich = New-Object System.Windows.Forms.RichTextBox  # a RichEdit control, closer to Notepad 11
$rich.Dock = 'Fill'; $rich.Font = New-Object System.Drawing.Font('Consolas', 14)
$form.Controls.Add($rich); $form.Controls.Add($edit)
Tap 0xA4  # an Alt tap (while the console is in front) lets this process take the foreground; sent to a shown
          # form it would start the system-menu mode, whose modal loop never returns to DoEvents
$form.Show(); $edit.Focus() | Out-Null; Wait 300
[Native]::SetForegroundWindow($form.Handle) | Out-Null
$form.Activate(); Wait 500
Out "app $($app.Id), window $($form.Handle), foreground $([Native]::GetForegroundWindow())"
if ([Native]::GetForegroundWindow() -ne $form.Handle) { Out 'WARN  the test window is not in the foreground; results may be wrong' }
function Other([string]$t) { if ($t -match '[а-я]') { 'ghbdtn vbh ntcn' } else { 'привет мир тест' } }

try {
    $controls = if ($Only -eq 'Notepad') { @() } else { @($edit, $rich) }
    foreach ($box in $controls) {
        $kind = $box.GetType().Name
        $box.Focus() | Out-Null; Wait 300; Out "--- $kind"
        # 1. empty document, double Shift: nothing inserted
        DoubleShift
        Expect "$kind empty document stays empty" (ReadAll) @('')
        # 2. three words, converted back and forth four times
        TypeKeys 'ghbdtn vbh ntcn'
        $text = ReadAll
        Expect "$kind typed three words" $text @('ghbdtn vbh ntcn', 'привет мир тест')
        foreach ($i in 1..4) {
            $want = Other $text; DoubleShift; $text = ReadAll
            Expect "$kind double Shift $i" $text @($want)
        }
        ClearAll
        # 3. one word, typo, Backspace, go on, double Shift
        TypeKeys 'ghbdtn'; Tap 0x08; Wait 100; TypeKeys 'n'; DoubleShift
        Expect "$kind Backspace in the phrase" (ReadAll) @('привет', 'ghbdtn')
        ClearAll
        # 3b. one word, Pause alone: converts the typed phrase like double Shift
        TypeKeys 'ghbdtn'; $typed = ReadAll; PauseKey
        Expect "$kind Pause converts the typed phrase" (ReadAll) @((Other $typed).Split(' ')[0])
        ClearAll
        # 4. selection + Shift+Pause, with something else on the clipboard that must survive
        Set-Clipboard -Value 'KEEP ME'
        TypeKeys 'ghbdtn'; $typed = ReadAll; SelectLine; ShiftPause
        Expect "$kind clipboard kept after Shift+Pause" ((Get-Clipboard -Raw).Trim()) @('KEEP ME')
        Expect "$kind Shift+Pause converts the selection" (ReadAll) @((Other $typed).Split(' ')[0])
        ClearAll
        # 6. the typed text selected, then double Shift: the selection path, not a retype
        TypeKeys 'ghbdtn'; $typed = ReadAll; SelectLine; DoubleShift
        Expect "$kind Shift+Home selection, double Shift" (ReadAll) @((Other $typed).Split(' ')[0])
        ClearAll
        TypeKeys 'ghbdtn'; $typed = ReadAll; SelectLeft 6; DoubleShift
        Expect "$kind Shift+Left selection, double Shift" (ReadAll) @((Other $typed).Split(' ')[0])
        ClearAll
        if ($kind -eq 'RichTextBox') {  # Ctrl+A does not select all in a multiline TextBox
            TypeKeys 'ghbdtn'; $typed = ReadAll; Chord 0x11 0x41; Wait 200; DoubleShift
            Expect "$kind Ctrl+A selection, double Shift" (ReadAll) @((Other $typed).Split(' ')[0])
            ClearAll
        }
        # 7. mouse selections: the mouse hook resets the phrase, then the selection path
        TypeKeys 'ghbdtn'; $typed = ReadAll; DoubleClickWord $box; DoubleShift
        Expect "$kind double-click selection, double Shift" (ReadAll) @((Other $typed).Split(' ')[0])
        ClearAll
        TypeKeys 'ghbdtn'; $typed = ReadAll; DragSelect $box; DoubleShift
        Expect "$kind drag selection, double Shift" (ReadAll) @((Other $typed).Split(' ')[0])
        ClearAll
    }
    # the same in the real Windows 11 Notepad, in a new tab of the running instance (started when there is none)
    $pad = if ($Only -eq 'Controls') { $null } else { Get-Process Notepad -ErrorAction SilentlyContinue | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1 }
    if (-not $pad -and $Only -ne 'Controls') { Start-Process notepad.exe | Out-Null; Wait 2000; $pad = Get-Process Notepad -ErrorAction SilentlyContinue | Where-Object MainWindowHandle -ne 0 | Select-Object -First 1 }
    if ($pad) {
        $form.TopMost = $false
        Tap 0xA4; [Native]::SetForegroundWindow($pad.MainWindowHandle) | Out-Null; Wait 800
        if ([Native]::GetForegroundWindow() -ne $pad.MainWindowHandle) { Out 'WARN  Notepad did not come to the foreground' }
        Chord 0x11 0x4E; Wait 800  # Ctrl+N: a new tab
        $box = 'Notepad'; $kind = 'Notepad'
        Out "--- $kind"
        DoubleShift
        Expect "$kind empty document stays empty" (ReadAll) @('')
        # reading Notepad goes through Ctrl+A, which resets the phrase: no read between typing and converting
        TypeKeys 'ghbdtn vbh ntcn'; DoubleShift; $text = ReadAll
        Expect "$kind double Shift 1" $text @('ghbdtn vbh ntcn', 'привет мир тест')
        $prev = $text
        foreach ($i in 2..4) {
            # the read reset the phrase: type the words again, convert, read
            ClearAll; TypeKeys 'ghbdtn vbh ntcn'; DoubleShift; $text = ReadAll
            Expect "$kind double Shift $i" $text @('ghbdtn vbh ntcn', 'привет мир тест')
            Expect "$kind alternates" $text @((Other $prev))
            $prev = $text
        }
        ClearAll
        TypeKeys 'ghbdtn'; Tap 0x08; Wait 100; TypeKeys 'n'; DoubleShift
        Expect "$kind Backspace in the phrase" (ReadAll) @('привет', 'ghbdtn')
        ClearAll
        TypeKeys 'ghbdtn'; PauseKey
        Expect "$kind Pause converts the typed phrase" (ReadAll) @('привет', 'ghbdtn')
        ClearAll
        TypeKeys 'ghbdtn'; SelectLine; DoubleShift; $r = ReadAll
        Expect "$kind Shift+Home selection, double Shift" $r @('привет', 'ghbdtn')
        ClearAll
        TypeKeys 'ghbdtn'; SelectLeft 6; DoubleShift; $r = ReadAll
        Expect "$kind Shift+Left selection, double Shift" $r @('привет', 'ghbdtn')
        ClearAll
        Chord 0x11 0x57; Wait 500  # Ctrl+W closes the empty tab without a prompt
    } else { Out 'SKIP  Notepad not available' }
} finally {
    ReleaseModifiers
    $form.Close()
    Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
    if ($LogFile -and (Test-Path $appLog)) { Add-Content -Encoding UTF8 $LogFile ('--- kbswitch log', (Get-Content $appLog)) }
}
if ($script:failed) { exit 1 }
