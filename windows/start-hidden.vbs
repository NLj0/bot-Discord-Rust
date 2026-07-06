Set shell = CreateObject("WScript.Shell")
installDir = CreateObject("Scripting.FileSystemObject").GetParentFolderName(WScript.ScriptFullName)
shell.CurrentDirectory = installDir
shell.Run "cmd /c run-live-forever.bat", 0, False
