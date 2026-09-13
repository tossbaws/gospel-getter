' Opens Gospel Getter in a clean, app-like browser window (no tabs/address
' bar) if Edge is installed, otherwise falls back to whatever the default
' browser is. Run via wscript.exe so no console window flashes up.
Set objShell = CreateObject("WScript.Shell")
Set objFSO = CreateObject("Scripting.FileSystemObject")

url = "http://127.0.0.1:3002"

edgePaths = Array( _
    objShell.ExpandEnvironmentStrings("%ProgramFiles(x86)%") & "\Microsoft\Edge\Application\msedge.exe", _
    objShell.ExpandEnvironmentStrings("%ProgramFiles%") & "\Microsoft\Edge\Application\msedge.exe", _
    objShell.ExpandEnvironmentStrings("%LocalAppData%") & "\Microsoft\Edge\Application\msedge.exe" _
)

launched = False
For Each p In edgePaths
    If objFSO.FileExists(p) Then
        objShell.Run """" & p & """ --app=" & url, 1, False
        launched = True
        Exit For
    End If
Next

If Not launched Then
    objShell.Run url, 1, False
End If
