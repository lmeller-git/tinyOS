use core::fmt::Display;

use bitflags::bitflags;
pub use x86_64::structures::paging::PageTableFlags;

bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OpenOptions: u32 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const APPEND = 1 << 2;
        const TRUNCATE = 1 << 3;
        const CREATE = 1 << 4;
        const CREATE_DIR = 1 << 5;
        const CREATE_ALL = 1 << 6;
        const CREATE_LINK = 1 << 7;
        const NO_FOLLOW_LINK = 1 << 8;
        const EXECUTE = 1 << 9;
    }
}

impl OpenOptions {
    pub fn with_read(self) -> Self {
        self | Self::READ
    }

    pub fn with_write(self) -> Self {
        self | Self::WRITE
    }

    pub fn with_no_follow_symlink(self) -> Self {
        self | Self::NO_FOLLOW_LINK
    }

    pub fn with_truncate(self) -> Self {
        self | Self::TRUNCATE
    }

    pub fn with_append(self) -> Self {
        self | Self::APPEND
    }

    pub fn with_exec(self) -> Self {
        self | Self::EXECUTE
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::READ
    }
}

bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UnlinkOptions: u32 {
        const FORCE = 1 << 0;
        const RECURSIVE = 1 << 1;
        const NO_PRESERVE_ROOT = 1 << 2;
    }
}

impl UnlinkOptions {
    pub fn with_force(self) -> Self {
        self | Self::FORCE
    }

    pub fn with_rmdir(self) -> Self {
        self | Self::RECURSIVE
    }
}

impl Default for UnlinkOptions {
    fn default() -> Self {
        Self::empty()
    }
}

bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WaitOptions: u16 {
        const NOBLOCK = 1 << 0;
    }
}

bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct TaskWaitOptions: u16 {
        const W_EXIT = 1 << 0;
        const W_WAKEUP = 1 << 1;
        const W_BLOCK = 1 << 2;
    }
}

bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TaskStateChange: u16 {
        const WAKEUP = 1 << 0;
        const BLOCK = 1 << 1;
        const EXIT = 1 << 2;
    }
}

bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NodeType: u8 {
        const FILE = 1 << 0;
        const DIR = 1 << 1;
        const SYMLINK = 1 << 2;
        const MOUNT = 1 << 3;
        const VOID = 1 << 4;
    }
}

impl Display for NodeType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.contains(NodeType::FILE) {
            write!(f, "FILE ")?;
        }
        if self.contains(NodeType::DIR) {
            write!(f, "DIR ")?;
        }
        if self.contains(NodeType::SYMLINK) {
            write!(f, "SYMLINK ")?;
        }
        if self.contains(NodeType::MOUNT) {
            write!(f, "MOUNT ")?;
        }
        if self.contains(NodeType::VOID) {
            write!(f, "VOID ")?;
        }
        if self.is_empty() {
            write!(f, "-")?;
        }
        Ok(())
    }
}

bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NodePermissions: u8 {
        const R = 1 << 0;
        const W = 1 << 1;
        const X = 1 << 2;
    }
}

impl Default for NodePermissions {
    fn default() -> Self {
        Self::rw()
    }
}

impl NodePermissions {
    pub fn read() -> Self {
        Self::R
    }

    pub fn rw() -> Self {
        Self::read() | Self::W
    }

    pub fn rx() -> Self {
        Self::read() | Self::X
    }

    pub fn rwx() -> Self {
        Self::all()
    }

    pub fn r(&self) -> bool {
        self.contains(Self::R)
    }

    pub fn w(&self) -> bool {
        self.contains(Self::W)
    }

    pub fn x(&self) -> bool {
        self.contains(Self::X)
    }
}

impl Display for NodePermissions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let r = self.contains(NodePermissions::R);
        let w = self.contains(NodePermissions::W);
        let x = self.contains(NodePermissions::X);
        write!(
            f,
            "{}{}{}",
            if r { "r" } else { "-" },
            if w { "w" } else { "-" },
            if x { "x" } else { "-" }
        )
    }
}
