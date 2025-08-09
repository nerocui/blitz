use std::{env, process::Command, io::{self, Write}};

fn main() {
    println!("cargo:rerun-if-changed=idl/BlitzWinUI.idl");
    let metadata_dir = format!("{}\\System32\\WinMetadata", env!("windir"));
    let winmd_str = "Generated Files/BlitzWinUI.winmd";

    // make "Generated Files" directory if it doesn't exist
    std::fs::create_dir_all("Generated Files").unwrap();

    // Build the midlrt command we want to run inside the VS DevShell
    let mut midl_args: Vec<String> = vec![
        "/winrt".into(),
        "/nomidl".into(),
        "/h".into(), "nul".into(),
        "/metadata_dir".into(), metadata_dir.clone(),
        "/reference".into(), format!("{metadata_dir}\\Windows.Foundation.winmd"),
        "/winmd".into(), winmd_str.into(),
        "idl/BlitzWinUI.idl".into(),
    ];

    // Compose the PowerShell command per user instruction.
    // We'll append the midlrt invocation after entering the DevShell so midlrt.exe is on PATH.
    let devshell_prefix = r#"&{Import-Module "C:\Program Files\Microsoft Visual Studio\2022\Enterprise\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"; Enter-VsDevShell 19c26628 -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64";"#;
    let midl_invocation = format!(" midlrt.exe {} }}", midl_args.iter().map(|s| shell_escape(s)).collect::<Vec<_>>().join(" "));
    let full_script = format!("{}{}", devshell_prefix, midl_invocation);

    let status = Command::new(r"C:\Program Files\PowerShell\7\pwsh.exe")
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(full_script)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            println!("cargo:warning=midlrt.exe failed with status {s:?}; skipping WinRT binding generation");
            return;
        }
        Err(e) => {
            println!("cargo:warning=failed to launch PowerShell DevShell for midlrt ({e}); skipping WinRT binding generation");
            return;
        }
    }

    // Generate Rust bindings from the compiled WinMD and system metadata
    let warnings = windows_bindgen::bindgen([
        "--in",
        &winmd_str,
        &metadata_dir,
        "--out",
        "src/bindings.rs",
    "--filter",
    "BlitzWinUI",
        "--flat",
        "--implement",
    ]);

    // print all warnings
    for warning in warnings.iter() {
        println!("cargo:warning={}", warning);
    }
}

fn shell_escape(arg: &str) -> String {
    if arg.is_empty() { return "''".into(); }
    if !arg.contains([' ', '"', '\'', '`', '$', ';', '(', ')', '&', '|', '<', '>', '^', '%']) {
        return arg.to_string();
    }
    // Use single quotes; escape embedded single quotes by closing/opening.
    let mut out = String::from("'");
    for c in arg.chars() {
        if c == '\'' { out.push_str("'\''"); } else { out.push(c); }
    }
    out.push('\'');
    out
}