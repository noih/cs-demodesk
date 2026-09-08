use std::{env, fs, io::Cursor, path::PathBuf};
use sha2::{Digest, Sha256};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    build_window_hook(&out);
}

fn build_window_hook(out: &std::path::Path) {
    println!("cargo:rerun-if-changed=window-hook");
    println!("cargo:rerun-if-env-changed=DEMODESK_DETOURS_PACKAGE");
    let archive = out.join("detours-4.0.1.zip");
    let bytes = if let Some(path) = env::var_os("DEMODESK_DETOURS_PACKAGE") {
        fs::read(path).expect("read DEMODESK_DETOURS_PACKAGE")
    } else if archive.is_file() {
        fs::read(&archive).unwrap()
    } else {
        ureq::get("https://codeload.github.com/microsoft/Detours/zip/refs/tags/v4.0.1")
            .call().expect("download Detours (or set DEMODESK_DETOURS_PACKAGE)")
            .body_mut().read_to_vec().unwrap()
    };
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)),
        "5ab84eb08fb9befeb16ffd04ca283731b1e0e1e53b1947ce7868d4b9654e43fc",
        "Detours checksum mismatch");
    fs::write(archive, &bytes).unwrap();
    let src = out.join("detours-src");
    fs::create_dir_all(&src).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let Some(name) = entry.name().strip_prefix("Detours-4.0.1/src/") else { continue };
        if name.is_empty() || name.contains('/') || name.contains('\\') { continue; }
        let target = src.join(name);
        std::io::copy(&mut entry, &mut fs::File::create(target).unwrap()).unwrap();
    }
    let compiler = cc::Build::new().cpp(true).static_crt(true).opt_level(2)
        .warnings(false).cargo_metadata(false).get_compiler();
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let mut command = compiler.to_command();
    command.current_dir(out).args(["/LD", "/EHsc", "/DWIN32_LEAN_AND_MEAN"])
        .arg(format!("/I{}", src.display()))
        .arg(root.join("window-hook/WindowHook.cpp"));
    for name in ["detours", "modules", "disasm", "image", "creatwth",
        "disolx86", "disolx64", "disolia64", "disolarm", "disolarm64"] {
        command.arg(src.join(format!("{name}.cpp")));
    }
    let result = command.arg("/link").arg("user32.lib")
        .arg(format!("/OUT:{}", out.join("demodesk-window-hook.dll").display()))
        .output().expect("compile native window hook");
    assert!(result.status.success(), "window hook compilation failed: {} {}",
        String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));

    let result = compiler.to_command().current_dir(out).arg("/EHsc")
        .arg(root.join("window-hook/WindowProbe.cpp"))
        .arg(format!("/Fe:{}", out.join("demodesk-window-probe.exe").display()))
        .arg("/link").arg("user32.lib").output().expect("compile window probe");
    assert!(result.status.success(), "window probe compilation failed: {} {}",
        String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
}
