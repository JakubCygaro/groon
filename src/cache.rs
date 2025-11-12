use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;
use chrono::DateTime;

use chrono::Utc;

type PageInfoMap = HashMap<PathBuf, PageInfo>;

pub struct PageCache {
    pages: Mutex<PageInfoMap>
}

#[derive(Clone, Debug)]
pub struct PageInfo {
    pub contents: Arc<String>,
    pub dependencies: Vec<Arc<PathBuf>>,
    pub last_access: SystemTime,
}

impl Default for PageInfo {
    fn default() -> Self {
        Self {
            contents: Arc::from(String::from("")),
            dependencies: vec![],
            last_access: SystemTime::now()
        }
    }
}

impl PageCache {
    pub fn new() -> Self {
        Self {
            pages: Mutex::new(PageInfoMap::new())
        }
    }
    pub fn add_page(&mut self, path: PathBuf, page: PageInfo) -> Option<PageInfo> {
        let mut mtx = self.pages.lock().unwrap();
        mtx.insert(path, page)
    }
    pub fn get_page(&self, path: &PathBuf) -> Option<PageInfo> {
        let mtx = self.pages.lock().unwrap();
        mtx.get(path).map(|p| p.to_owned())
    }
    pub fn get_page_last_access(&self, path: &PathBuf) -> Option<SystemTime> {
        let mtx = self.pages.lock().unwrap();
        mtx.get(path).map(|p| p.last_access.to_owned())
    }
    pub fn get_page_contents(&self, path: &PathBuf) -> Option<Arc<String>> {
        let mtx = self.pages.lock().unwrap();
        mtx.get(path).map(|p| p.contents.clone())
    }
    pub fn update_page<F: Fn(&mut PageInfo)>(&mut self, path: PathBuf, f: F){
        let mut mtx = self.pages.lock().unwrap();
        mtx.entry(path)
            .and_modify(&f)
            .or_insert_with(|| {
                let mut p = PageInfo::default();
                f(&mut p);
                p
            });
    }
}


