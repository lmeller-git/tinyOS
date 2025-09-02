use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
};
use core::{borrow::Borrow, fmt::Display, ops::Deref};

// TODO: migrate this to libtinyos and use as dependancy

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct PathBuf {
    inner: String,
}

impl PathBuf {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn canonicalize(&mut self) {
        // TODO get cwd if path is relative
        let mut root = if self.is_relative() {
            todo!()
        } else {
            Self::new()
        };
        let segments = self
            .inner
            .split('/')
            .filter(|&segment| !segment.is_empty() && segment != ".");
        for segment in segments {
            if segment == ".." {
                root.up();
            } else {
                root.push(&segment);
            }
        }
    }

    pub fn up(&mut self) {
        let Some((new, _)) = self.inner.rsplit_once('/') else {
            return;
        };
        self.inner.truncate(new.len());
    }

    pub fn push<P: AsRef<Path>>(&mut self, path: &P) {
        // TODO validate if / is needed
        self.inner.push('/');
        self.inner.push_str(path.as_ref().as_str());
    }

    pub fn add_extension(&mut self, ext: &str) {
        // TODO validate if . is needed
        self.inner.push('.');
        self.inner.push_str(ext);
    }

    pub fn set_extension(&mut self, ext: &str) {
        // TODO validate if . is needed
        let Some((stem, _)) = self.inner.rsplit_once('.') else {
            self.inner.push_str(ext);
            return;
        };
        self.inner.truncate(stem.len());
        self.inner.push('.');
        self.inner.push_str(ext);
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn as_path(&self) -> &Path {
        self
    }
}

impl Default for PathBuf {
    fn default() -> Self {
        Self {
            inner: String::default(),
        }
    }
}

impl From<&str> for PathBuf {
    fn from(value: &str) -> Self {
        From::<String>::from(value.into())
    }
}

impl From<String> for PathBuf {
    fn from(value: String) -> Self {
        Self { inner: value }
    }
}

impl From<&Path> for PathBuf {
    fn from(value: &Path) -> Self {
        Self {
            inner: value.inner.into(),
        }
    }
}

impl AsRef<str> for PathBuf {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}

impl Deref for PathBuf {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        Path::new(self)
    }
}

impl Borrow<Path> for PathBuf {
    fn borrow(&self) -> &Path {
        self
    }
}

impl Display for PathBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "{}", self.inner)
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Path {
    inner: str,
}

impl Path {
    pub fn new<S: AsRef<str> + ?Sized>(path: &S) -> &Self {
        unsafe { &*(path.as_ref() as *const str as *const Path) }
    }

    pub fn to_owned(&self) -> PathBuf {
        PathBuf {
            inner: self.inner.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn traverse(&self) -> impl Iterator<Item = &str> {
        self.inner.split('/')
    }

    pub fn is_relative(&self) -> bool {
        self.inner.chars().next().is_some_and(|c| c == '/')
    }

    pub fn parent(&self) -> &Path {
        let Some((s, _)) = self.inner.rsplit_once('/') else {
            return Path::new("/");
        };
        Path::new(s)
    }

    pub fn extension(&self) -> &str {
        let Some((_, e)) = self.inner.rsplit_once('.') else {
            return "";
        };
        e
    }

    fn file_prefix(&self) -> &Path {
        let Some((_, f)) = self.inner.rsplit_once('/') else {
            return Path::new("");
        };
        let Some((f, _)) = f.split_once('.') else {
            return Path::new(f);
        };
        Path::new(f)
    }
}

impl ToOwned for Path {
    type Owned = PathBuf;

    fn to_owned(&self) -> Self::Owned {
        self.into()
    }

    fn clone_into(&self, target: &mut Self::Owned) {
        target.clear();
        target.push(&self);
    }
}

impl AsRef<Path> for Path {
    fn as_ref(&self) -> &Path {
        self
    }
}

impl AsRef<Path> for str {
    fn as_ref(&self) -> &Path {
        Path::new(self)
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "{}", &self.inner)
    }
}
