use std::{env, fs, path::Path, process::Command};

fn main() {
    let programs = concat!(env!("CARGO_MANIFEST_DIR"), "/../tinyosprograms/programs");

    let programs_dir = Path::new(programs);
    let mut bins = Vec::new();
    for program in fs::read_dir(programs_dir).unwrap().flatten() {
        let dir = program.path();
        if !dir.join("build.sh").is_file() {
            continue;
        }
        if Command::new("bash")
            .arg("build.sh")
            .current_dir(&dir)
            .status()
            .is_err()
        {
            panic!("could not build {}", dir.display());
        }
        // dir should now contain a.out
        let dir = programs_dir.join(program.file_name()).join("a.out");
        bins.push(dir);
    }

    let mut includes = String::new();
    includes.push_str("pub fn get_binaries() -> alloc::vec::Vec<&'static [u8]>{\n\talloc::vec![\n");
    for bin in bins {
        includes.push_str(&format!("\t\tinclude_bytes!(\"{}\"),\n", bin.display()));
    }
    includes.push_str("\t]\n}\n");

    fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/include_bins.rs"),
        includes,
    )
    .unwrap();

    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    // Tell cargo to pass the linker script to the linker..
    println!("cargo:rustc-link-arg=-Tlinker-{arch}.ld");
    // ..and to re-run if it changes.
    println!("cargo:rerun-if-changed=linker-{arch}.ld");
}
