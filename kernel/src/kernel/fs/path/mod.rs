use alloc::{
    borrow::ToOwned,
    string::{String, ToString},
};
use core::{borrow::Borrow, fmt::Display, ops::Deref};

// TODO: migrate this to libtinyos and use as dependancy

const PATH_SEP: char = '/';
const EXT_SEP: char = '.';
// currently root dir is an empty str, due to path being represented as &str, without handling of first SEP
// A Path /foo will yield ROOT_DIR as its parent
// TODO: add Components, which correctly parse ROOT + Prefix, ...
const ROOT_DIR: &str = "";

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
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
            todo!("need to query cwd from fs")
        } else {
            Self::new()
        };
        let segments = self
            .inner
            .split(PATH_SEP)
            .filter(|&segment| !segment.is_empty() && segment != ".");
        for segment in segments {
            if segment == ".." {
                root.up();
            } else {
                root.push(&segment);
            }
        }
        *self = root;
    }

    pub fn up(&mut self) {
        let Some((new, _)) = self.inner.rsplit_once(PATH_SEP) else {
            return;
        };
        self.inner.truncate(new.len());
    }

    /// appends path to self
    /// if path is absolute, self will be replaced with path
    /// No canonicalization will be performed by this method, call PathBuf::canonicalize for that
    pub fn push<P: AsRef<Path> + ?Sized>(&mut self, path: &P) {
        if !path.as_ref().is_relative() {
            self.clear();
            self.inner.push_str(path.as_ref().as_str());
        } else {
            self.inner.push(PATH_SEP);
            self.inner.push_str(path.as_ref().as_str());
        }
    }

    pub fn add_extension(&mut self, ext: &str) {
        if !ext.starts_with(EXT_SEP) {
            self.inner.push(EXT_SEP);
        }
        self.inner.push_str(ext);
    }

    pub fn set_extension(&mut self, ext: &str) {
        let Some((stem, _)) = self.inner.rsplit_once(EXT_SEP) else {
            self.inner.push_str(ext);
            return;
        };
        self.inner.truncate(stem.len());
        if !ext.starts_with(EXT_SEP) {
            self.inner.push(EXT_SEP);
        }
        self.inner.push_str(ext);
    }

    pub fn clear_extension(&mut self) {
        let Some((stem, _)) = self.inner.rsplit_once(EXT_SEP) else {
            return;
        };
        self.inner.truncate(stem.len());
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

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
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
        // do not filter empty segments, as they are real (ROOT_DIR is empty)
        self.inner.split(PATH_SEP)
    }

    pub fn is_relative(&self) -> bool {
        // an absolute Path must start with '/'
        // as currently "" == ROOT_DIR, ROOT_DIR is relative
        self.inner.chars().next().is_none_or(|c| c != '/')
    }

    pub fn parent(&self) -> Option<&Path> {
        self.inner.rsplit_once(PATH_SEP).map(|(s, _)| Path::new(s))
    }

    pub fn extension(&self) -> &str {
        let Some((_, e)) = self.inner.rsplit_once(EXT_SEP) else {
            return "";
        };
        e
    }

    pub fn file_prefix(&self) -> &str {
        let Some((_, f)) = self.inner.rsplit_once(PATH_SEP) else {
            return "";
        };
        let Some((f, _)) = f.split_once(EXT_SEP) else {
            return f;
        };
        f
    }

    pub fn file(&self) -> &str {
        let Some((_, f)) = self.inner.rsplit_once(PATH_SEP) else {
            return "";
        };
        f
    }

    pub fn ancestors(&self) -> Ancestors<'_> {
        Ancestors::new(&self)
    }

    pub fn strip_prefix<S: AsRef<Path>>(&self, prefix: &S) -> Option<&Self> {
        self.inner
            .strip_prefix(prefix.as_ref().as_str())
            .map(|postfix| Path::new(postfix))
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

pub struct Ancestors<'a> {
    next: Option<&'a Path>,
}

impl<'a> Ancestors<'a> {
    pub fn new(p: &'a Path) -> Self {
        Self { next: Some(p) }
    }
}

impl<'a> Iterator for Ancestors<'a> {
    type Item = &'a Path;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next;
        self.next = next.and_then(Path::parent);
        next
    }
}

#[cfg(feature = "test_run")]
mod tests {
    use os_macros::kernel_test;

    use super::*;

    #[kernel_test]
    fn path() {
        let mut path = PathBuf::new();
        assert!(path.is_relative());
        path.push("/foo");
        path.push("bar/baz");
        assert!(!path.is_relative());

        let mut components = path.traverse();
        assert_eq!(components.next(), Some(""));
        assert_eq!(components.next(), Some("foo"));
        assert_eq!(components.next(), Some("bar"));
        assert_eq!(components.next(), Some("baz"));
        assert!(components.next().is_none());
        drop(components);

        let mut ancestors = path.ancestors();
        assert_eq!(ancestors.next().unwrap(), path.as_path());
        assert_eq!(ancestors.next(), path.parent());
        assert_eq!(ancestors.next().unwrap(), Path::new("/foo"));
        assert_eq!(ancestors.next().unwrap(), Path::new(ROOT_DIR));
        assert!(ancestors.next().is_none());

        path.add_extension("txt");
        path.add_extension("gz");
        assert_eq!(path.extension(), "gz");
        path.clear_extension();
        assert_eq!(path.extension(), "txt");
        path.set_extension(".rs");
        assert_eq!(path.extension(), "rs");
        assert_eq!(path.file_prefix(), "baz");
        path.up();
        assert_eq!(path.file_prefix(), "bar");

        path.push("./baz/../../foo.bar");
        path.canonicalize();
        assert_eq!(path.file(), "foo.bar");
        assert_eq!(path.parent().unwrap().file(), "foo");
    }
}
