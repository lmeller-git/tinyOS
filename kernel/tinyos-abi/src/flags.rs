use bitflags::bitflags;
pub use x86_64::structures::paging::PageTableFlags;

bitflags! {
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
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::READ
    }
}

bitflags! {
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WaitOptions: u16 {
        const NOBLOCK = 1 << 0;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub struct TaskWaitOptions: u16 {
        const W_EXIT = 1 << 0;
        const W_WAKEUP = 1 << 1;
        const W_BLOCK = 1 << 2;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TaskStateChange: u16 {
        const WAKEUP = 1 << 0;
        const BLOCK = 1 << 1;
        const EXIT = 1 << 2;
    }
}
