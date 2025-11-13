use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

type PageInfoMap = HashMap<PathBuf, PageInfo>;

pub struct PageCache {
    pages: PageInfoMap
}

#[derive(Clone, Debug)]
pub struct PageInfo {
    pub contents: String,
    pub dependencies: Vec<PathBuf>,
    pub last_access: SystemTime,
}

impl Default for PageInfo {
    fn default() -> Self {
        Self {
            contents: String::from(""),
            dependencies: vec![],
            last_access: SystemTime::now()
        }
    }
}

impl PageCache {
    pub fn new() -> Self {
        Self {
            pages: PageInfoMap::new()
        }
    }
    pub fn add_page(&mut self, path: PathBuf, page: PageInfo) -> Option<PageInfo> {
        self.pages.insert(path, page)
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
    }
}


