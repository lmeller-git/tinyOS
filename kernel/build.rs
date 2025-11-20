use std::{env, fs, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=NULL");

    update_submodules();

    build_user_programs();

    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    // Tell cargo to pass the linker script to the linker..
    println!("cargo:rustc-link-arg=-Tlinker-{arch}.ld");
    // ..and to re-run if it changes.
    println!("cargo:rerun-if-changed=linker-{arch}.ld");
}

fn update_submodules() {
    println!("cargo:warning=Updating git submodules...");

    let Ok(output) = Command::new("git")
        .args([
            "submodule",
            "update",
            "--init",
            "--recursive",
            "--remote",
            "--force",
        ])
        .output()
    else {
        eprintln!("cargo:warning=Submodule update error");
        return;
    };

    if !output.status.success() {
        println!(
            "cargo:warning=Submodule update error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn build_user_programs() {
    println!("cargo:warning=Building user programs...");

    let programs = concat!(env!("CARGO_MANIFEST_DIR"), "/../tinyosprograms/programs");
    let out = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out);

    let programs_dir = Path::new(programs);
    let mut bins = Vec::new();
    for program in fs::read_dir(programs_dir).unwrap().flatten() {
        let dir = program.path();
        if dir.join("build.sh").is_file() {
            let child = Command::new("bash")
                .arg("build.sh")
                .current_dir(&dir)
                .output()
                .expect("could not run build.sh");

            println!(
                "cargo:warning=stdout build.sh: {}",
                String::from_utf8_lossy(&child.stdout)
            );
            println!(
                "cargo:warning=stderr build.sh: {}",
                String::from_utf8_lossy(&child.stderr)
            );

            if !child.status.success() {
                panic!(
                    "could not build {}: {}",
                    dir.display(),
                    String::from_utf8_lossy(&child.stderr)
                );
            }
        }
        if dir.join("a.out").exists() {
            // dir should now contain a.out
            let dir = programs_dir.join(program.file_name()).join("a.out");

            // copy binaries into OUT_DIR
            let file_name = format!("{}.out", program.file_name().display());
            let target = out_dir.join(&file_name);
            fs::copy(dir, &target).expect("could not copy bin into OUT_DIR");
            bins.push(file_name);
        } else {
            println!("carg:warning=stderr {{build.sh ran}}, but a.out does not exist");
        }
    }

    let mut includes = String::new();
    includes.push_str(
        "pub fn get_binaries() -> alloc::vec::Vec<(alloc::string::String, &'static [u8])>{\n\talloc::vec![\n",
    );
    for bin in bins {
        includes.push_str(&format!(
            "\t\t(\"{bin}\".into(), include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{bin}\"))),\n"
        ));
    }
    includes.push_str("\t]\n}\n");

    fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/include_bins.rs"),
        includes,
    )
    .unwrap();
}
