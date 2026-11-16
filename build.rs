fn main() {
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-env-changed=SWITCHEROO_PAYLOAD_GZ");

    configure_installer_payload();

    #[cfg(windows)]
    compile_windows_resources();
}

fn configure_installer_payload() {
    use std::path::PathBuf;

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    if let Some(payload_env) = std::env::var_os("SWITCHEROO_PAYLOAD_GZ") {
        let p = PathBuf::from(&payload_env);
        if p.is_file() {
            println!("cargo:rerun-if-changed={}", p.display());
            println!("cargo:rustc-env=SWITCHEROO_PAYLOAD_FILE={}", p.display());
            return;
        }
    }

    let empty = out_dir.join("empty_payload.gz");
    if !empty.exists() {
        std::fs::write(&empty, []).expect("Failed to create empty_payload.gz");
    }
    println!("cargo:rustc-env=SWITCHEROO_PAYLOAD_FILE={}", empty.display());
}

#[cfg(windows)]
fn compile_windows_resources() {
    use std::path::PathBuf;
    use std::process::Command;

    let mut res = winresource::WindowsResource::new();
    res.set_icon("icon.ico");
    res.set("ProductName", "Switcheroo");
    res.set(
        "FileDescription",
        "Switcheroo - 1-Click Steam Account Switcher & Dota 2 Settings Synchronizer",
    );
    res.set("CompanyName", "Mightyiest");

    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env != "gnu" {
        if let Err(err) = res.compile() {
            panic!("Failed to compile Windows resources: {err}");
        }
        return;
    }

    // GNU `windres` passes paths through `popen()` when running `gcc -E`, which fails when
    // workspace paths contain spaces. Stage `.rc` and `icon.ico` in a space-free temp dir.
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let work_dir =
        std::env::temp_dir().join(format!("switcheroo_winres_{}", std::process::id()));
    std::fs::create_dir_all(&work_dir).expect("Failed to create windres temp dir");

    std::fs::copy(manifest_dir.join("icon.ico"), work_dir.join("icon.ico"))
        .expect("Failed to copy icon.ico to windres temp dir");
    res.write_resource_file(work_dir.join("main_res.rc"))
        .expect("Failed to write main_res.rc");

    let bfd_target = match std::env::var("CARGO_CFG_TARGET_ARCH")
        .unwrap_or_default()
        .as_str()
    {
        "x86_64" => "pe-x86-64",
        "x86" => "pe-i386",
        _ => "pe-x86-64",
    };

    let status = Command::new("windres")
        .current_dir(&work_dir)
        .args([
            "--use-temp-file",
            "--target",
            bfd_target,
            "main_res.rc",
            "main_res.o",
        ])
        .status()
        .expect("Failed to execute windres");

    if !status.success() {
        let _ = std::fs::remove_dir_all(&work_dir);
        panic!("windres failed with status: {status}");
    }

    let out_obj = out_dir.join("main_res.o");
    std::fs::copy(work_dir.join("main_res.o"), &out_obj)
        .expect("Failed to copy compiled resource object to OUT_DIR");
    let _ = std::fs::remove_dir_all(&work_dir);

    println!("cargo:rustc-link-arg-bin=Switcheroo={}", out_obj.display());
    println!(
        "cargo:rustc-link-arg-bin=Switcheroo-Setup={}",
        out_obj.display()
    );
}
