use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::templating::HTMLFile;

type PageInfoMap = HashMap<PathBuf, PageInfo>;

pub struct PageCache {
    pages: PageInfoMap
}

#[derive(Clone, Debug)]
pub struct PageInfo {
    pub contents: String,
    pub dependencies: Vec<PathBuf>,
    pub last_modified: SystemTime,
}

impl Default for PageInfo {
    fn default() -> Self {
        Self {
            contents: String::from(""),
            dependencies: vec![],
            last_modified: SystemTime::now()
        }
    }
}

impl From<HTMLFile> for PageInfo {
    fn from(value: HTMLFile) -> Self {
        Self {
            contents: value.content,
            dependencies: value.dependencies,
            last_modified: SystemTime::now()
        }
    }
}

impl Into<HTMLFile> for PageInfo {
    fn into(self) -> HTMLFile {
        HTMLFile { content: self.contents, dependencies: self.dependencies }
    }
}

impl PageCache {
    pub fn new() -> Self {
        Self {
            pages: PageInfoMap::new()
        }
    }
    pub fn add_page(&mut self, path: PathBuf, page: PageInfo) -> Option<PageInfo> {
        let ret = self.pages.insert(path, page);
        self.print_cache();
        ret
    }
    pub fn get_page(&self, path: &PathBuf) -> Option<&PageInfo> {
        self.pages.get(path)
    }
    pub fn update_page<F: Fn(&mut PageInfo)>(&mut self, path: PathBuf, f: F){
        self.pages.entry(path)
            .and_modify(&f)
            .or_insert_with(|| {
                let mut p = PageInfo::default();
                f(&mut p);
                p
            });
        self.print_cache();
    }
    pub fn has_page(&self, path: &PathBuf) -> bool {
        self.pages.contains_key(path)
    }
    fn print_cache(&self) {
        println!("{:?}", self.pages)
    }
}


