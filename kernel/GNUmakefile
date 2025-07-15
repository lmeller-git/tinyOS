# Nuke built-in rules and variables.
MAKEFLAGS += -rR
.SUFFIXES:

# This is the name that our final executable will have.
# Change as needed.
override OUTPUT := kernel

# Convenience macro to reliably declare user overridable variables.
override USER_VARIABLE = $(if $(filter $(origin $(1)),default undefined),$(eval override $(1) := $(2)))

# Target architecture to build for. Default to x86_64.
$(call USER_VARIABLE,KARCH,x86_64)
$(call USER_VARIABLE,CARGO_FLAGS,)
$(call USER_VARIABLE,CARGO_TARGET_DIR,)
$(call USER_VARIABLE,KERNEL_BIN,)

ifeq ($(RUST_TARGET),)
    override RUST_TARGET := $(KARCH)-unknown-none
	ifeq ($(KARCH),riscv64)
    	override RUST_TARGET := riscv64gc-unknown-none-elf
	endif
endif

ifeq ($(RUST_PROFILE),)
    override RUST_PROFILE := dev
endif

override RUST_PROFILE_SUBDIR := $(RUST_PROFILE)
ifeq ($(RUST_PROFILE),dev)
    override RUST_PROFILE_SUBDIR := debug
endif

# Default target.
.PHONY: all
all:
	RUSTFLAGS="-C relocation-model=static" cargo build --target $(RUST_TARGET) --profile $(RUST_PROFILE) $(CARGO_FLAGS) --target-dir $(CARGO_TARGET_DIR)
	cp $(CARGO_TARGET_DIR)/$(RUST_TARGET)/$(RUST_PROFILE_SUBDIR)/$$(cd $(CARGO_TARGET_DIR)/$(RUST_TARGET)/$(RUST_PROFILE_SUBDIR) && find -maxdepth 1 -perm -111 -type f) $(KERNEL_BIN)

# Remove object files and the final executable.
.PHONY: clean
clean:
	cargo clean
	rm -f kernel

.PHONY: distclean
distclean: clean
