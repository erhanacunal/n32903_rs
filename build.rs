use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Compile the ARM assembly startup file (produces startup.o for
    // applications linking against this library).

    let asm_file = "asm/startup.s";
    println!("cargo:rerun-if-changed={}", asm_file);
    println!("cargo:rerun-if-changed=linker.ld");

    let cc = env::var("CC_armv5te_none_eabi")
        .unwrap_or_else(|_| "arm-none-eabi-gcc".to_string());

    let obj_path = out_dir.join("startup.o");
    let status = std::process::Command::new(&cc)
        .arg("-mcpu=arm926ej-s")
        .arg("-marm")
        .arg("-c")
        .arg(asm_file)
        .arg("-o")
        .arg(&obj_path)
        .status();

    match status {
        Ok(s) if s.success() => {
            // Depfile-style path for consumers — stored via env key for
            // dependents' build scripts to discover.
            println!("cargo:rustc-env=N32903_STARTUP_OBJ={}", obj_path.display());
        }
        Ok(_) | Err(_) => {
            // If the cross-compiler isn't available during a `cargo check`
            // or similar, don't fail — the source is still valid.
            println!("cargo:warning=arm-none-eabi-gcc not found; startup.o not built (ok for library-only builds)");
        }
    }

    // No linker args — this is a staticlib; the application provides linker.ld
    // and -nostartfiles.
}
