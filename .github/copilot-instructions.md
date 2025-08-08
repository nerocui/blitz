```instructions
 - Always run cargo build/check for blitz-shell-winui from a powershell with the following command line
 `"C:\Program Files\PowerShell\7\pwsh.exe" -NoExit -Command "&{Import-Module """C:\Program Files\Microsoft Visual Studio\2022\Enterprise\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"""; Enter-VsDevShell 19c26628 -SkipAutomaticLocation -DevCmdArguments """-arch=x64 -host_arch=x64"""}"`
	 This ensures midlrt.exe and the Windows SDK tools are on PATH for IDL/WinMD generation.

 - The WinUI shell embeds rendering inside a Microsoft.UI.Xaml.Controls.SwapChainPanel.
	 Do not require or assume a top-level HWND from the C# app. Accept a panel reference
	 (WinRT object) and create the WGPU surface via DXGI/SwapChainPanel interop internally.

 - bindings.rs is generated from idl/Blitz.WinUI.idl via midlrt + windows-bindgen. Do not edit it by hand.
```
