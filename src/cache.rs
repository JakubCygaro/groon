use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::templating::HTMLFile;

type PageInfoMap = HashMap<PathBuf, PageInfo>;

#[derive(Clone)]
pub struct PageCache {
    pages: PageInfoMap,
}

#[derive(Clone, Debug)]
pub struct PageInfo {
    pub contents: Option<String>,
    pub dependencies: HashSet<PathBuf>,
    pub last_modified: SystemTime,
    pub last_accessed: SystemTime,
    pub req_count: u64,
}

impl Default for PageInfo {
    fn default() -> Self {
        Self {
            contents: String::from("").into(),
            dependencies: HashSet::new(),
            last_modified: SystemTime::now(),
            last_accessed: SystemTime::now(),
            req_count: 0,
        }
    }
}

impl From<HTMLFile> for PageInfo {
    fn from(value: HTMLFile) -> Self {
        Self {
            contents: value.content.into(),
            dependencies: value.dependencies,
            last_modified: SystemTime::now(),
            last_accessed: SystemTime::now(),
            req_count: 0,
        }
    }
}

impl From<PageInfo> for HTMLFile {
    fn from(value: PageInfo) -> Self {
        Self {
            content: value.contents.unwrap_or("".to_owned()),
            dependencies: value.dependencies,
        }
    }
}

impl PageCache {
    pub fn new() -> Self {
        Self {
            pages: PageInfoMap::new(),
        }
    }
    pub fn add_page(&mut self, path: PathBuf, page: PageInfo) -> Option<PageInfo> {
        self.pages.insert(path, page)
    }
    pub fn get_page(&self, path: &PathBuf) -> Option<&PageInfo> {
        self.pages.get(path)
    }
    pub fn update_page<F: Fn(&mut PageInfo)>(&mut self, path: PathBuf, f: F) {
        self.pages.entry(path).and_modify(&f).or_insert_with(|| {
            let mut p = PageInfo::default();
            f(&mut p);
            p
        });
    }
    pub fn page_accessed_now(&mut self, path: PathBuf) {
        self.update_page(path, |p| {
            p.last_accessed = SystemTime::now();
            p.req_count += 1;
        });
    }
    pub fn has_page(&self, path: &PathBuf) -> bool {
        self.pages.contains_key(path)
    }
    fn print_cache(&self) {
        println!("{:?}", self.pages.keys())
    }
}
