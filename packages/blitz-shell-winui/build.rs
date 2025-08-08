use std::env;

fn main() {
    println!("cargo:rerun-if-changed=idl/Blitz.WinUI.idl");
    let metadata_dir = format!("{}\\System32\\WinMetadata", env!("windir"));
    let mut command = std::process::Command::new("midlrt.exe");
    let winmd_str = "Generated Files/BlitzWinRT.winmd";

    // make "Generated Files" directory if it doesn't exist
    std::fs::create_dir_all("Generated Files").unwrap();

    command.args([
        "/winrt",
        "/nomidl",
        "/h",
        "nul",
        "/metadata_dir",
        &metadata_dir,
        "/reference",
        &format!("{metadata_dir}\\Windows.Foundation.winmd"),
        "/winmd",
        &winmd_str,
        "idl/Blitz.WinUI.idl",
    ]);

    if !command.status().unwrap().success() {
        panic!("Failed to run midlrt");
    }

    // Generate Rust bindings from the compiled WinMD and system metadata
    let warnings = windows_bindgen::bindgen([
        "--in",
        &winmd_str,
        &metadata_dir,
        "--out",
        "src/bindings.rs",
    "--filter",
    "Blitz.WinUI",
        "--flat",
        "--implement",
    ]);

    // print all warnings
    for warning in warnings.iter() {
        println!("cargo:warning={}", warning);
    }
    // {
    //     panic!("{error}");
    // }
}