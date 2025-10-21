use alloc::{string::String, vec::Vec};

use crate::{
    KernelRes,
    eprintln,
    include_bins,
    kernel::{
        devices,
        fs::{self, OpenOptions, Path, PathBuf, UnlinkOptions},
        io::{Read, Write},
        mem,
        threading::{self, schedule, task::TaskBuilder},
    },
};

pub const KERNEL_DIR: &str = "/kernel";
pub const INCLUDED_BINS: &str = "/ram/bin";

pub fn early_init() {
    mem::init();
}

pub fn late_init() {
    fs::init();
    devices::init();
    load_init_bins();
    threading::init();
}

pub fn default_task() -> KernelRes<()> {
    let mut bin_path = Path::new(INCLUDED_BINS).to_owned();
    let binaries = fs::lsdir(&bin_path)?;
    let mut bin_data = Vec::new();

    for name in binaries.split('\t').filter(|n| !n.is_empty()) {
        bin_path.push(name);

        if let Ok(bin) = fs::open(&bin_path, OpenOptions::READ)
            && let Ok(n_read) = bin
                .read_to_end(&mut bin_data, 0)
                .inspect_err(|e| eprintln!("binary {} could not be read.\n{}", name, e))
        {
            let task = TaskBuilder::from_bytes(&bin_data[..n_read])?
                .with_default_files()
                .as_usr()?
                .build();

            schedule::add_built_task(task);
        }

        bin_path.up();
    }
    Ok(())
}

fn load_init_bins() {
    let mut binaries: Vec<(String, &'static [u8])> = include_bins::get_binaries();
    // binaries.push((
    //     "test".into(),
    //     include_bytes!("../../../../../tinyosprograms/programs/example-rs/a.out"),
    // ));
    let mut bin_path: PathBuf = Path::new(INCLUDED_BINS).into();

    for (name, bin) in binaries.into_iter() {
        bin_path.push(name.as_str());
        if let Ok(file) = fs::open(&bin_path, OpenOptions::CREATE_ALL | OpenOptions::WRITE) {
            if let Err(e) = file.write_all(bin, 0) {
                eprintln!(
                    "could not write elf data into binary {}.\n{}\nRemoving the binary...",
                    name, e
                );
                fs::rm(&bin_path, UnlinkOptions::empty()).unwrap();
            }
        } else {
            eprintln!("failed to add binary {}", name);
        };
        bin_path.up();
    }
}

#[macro_export]
macro_rules! create_device_file {
    ($device:expr, $path:expr) => {
        create_device_file!(
            $device,
            $path,
            $crate::kernel::fs::OpenOptions::READ | $crate::kernel::fs::OpenOptions::CREATE_ALL
        )
    };

    ($device:expr, $path:expr, $permissions:expr) => {{
        let mut p = $crate::kernel::fs::Path::new($crate::kernel::fs::PROCFS_PATH).to_owned();
        p.push($path.strip_prefix("/").unwrap_or($path));
        $crate::register_device_file!($device, $path)
            .and_then(|_| $crate::kernel::fs::open(&p, $permissions))
    }};
}
