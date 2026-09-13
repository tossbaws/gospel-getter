' Opens Gospel Getter in the user's default browser. Run via wscript.exe so
' no console window flashes up.
Set objShell = CreateObject("WScript.Shell")
objShell.Run "http://127.0.0.1:3002", 1, False
