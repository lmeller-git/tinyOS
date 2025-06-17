# TinyOS

yet another OS kernel written for fun  

## How to use this?

### Dependencies

Any `make` command depends on GNU make (`gmake`) and is expected to be run using it. This usually means using `make` on most GNU/Linux distros, or `gmake` on other non-GNU systems.

All `make all*` targets depend on Rust.

Additionally, building an ISO with `make all` requires `xorriso`, and building a HDD/USB image with `make all-hdd` requires `sgdisk` (usually from `gdisk` or `gptfdisk` packages) and `mtools`.

### Architectural targets

The `KARCH` make variable determines the target architecture to build the kernel and image for.

The default `KARCH` is `x86_64`. Other options include: `aarch64`, `riscv64`, and `loongarch64`.

Other architectures will need to be enabled in kernel/rust-toolchain.toml

### Makefile targets

Running `make all` will compile the kernel (from the `kernel/` directory) and then generate a bootable ISO image.

Running `make all-hdd` will compile the kernel and then generate a raw image suitable to be flashed onto a USB stick or hard drive/SSD.

Running `make run` will build the kernel and a bootable ISO (equivalent to make all) and then run it using `qemu` (if installed).

Running `make run-hdd` will build the kernel and a raw HDD image (equivalent to make all-hdd) and then run it using `qemu` (if installed).

Running `make test` will build the kernel with test_run features and run the tests

### Makefile Variables

qemu flags can be passed via QEMUFLAGS  
cargo flags can be passed via CARGO_FLAGS  
file names can be changed via CARGO_TARGET_DIR, KERNEL_BIN and IMAGE_NAME  
the rust profile can be changed with RUST_PROFILE  

## Supported architectures

x86-64 (default)

currently no real hardware is supported  

## References

OsDev wiki: https://wiki.osdev.org/
Phillip Oppermans blog series: https://os.phil-opp.com/
