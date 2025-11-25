use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use crate::templating::HTMLFile;

type PageInfoMap = HashMap<PathBuf, PageInfo>;

#[derive(Clone)]
pub struct PageCache {
    max_size: u64,
    current_size: u64,
    pages: PageInfoMap,
}

#[derive(Clone, Debug)]
pub struct PageInfo {
    /// contents can be dynamically unloaded from the cache,
    /// but other relevant information about dependencies and usage times is kept
    pub contents: Option<String>,
    pub dependencies: HashSet<PathBuf>,
    pub last_modified: SystemTime,
    pub last_accessed: Instant,
}

impl PageInfo {
    fn get_score(&self, now: &Instant) -> u64 {
        let delta = *now - self.last_accessed;
        if let Some(contents) = &self.contents {
            let sz_factor = contents.len() as u64 + self.dependencies.len() as u64;
            let irrelevance_factor = delta.as_secs().min(sz_factor);
            sz_factor - irrelevance_factor
        } else {
            u64::MAX
        }
    }
}

impl Default for PageInfo {
    fn default() -> Self {
        Self {
            contents: None,
            dependencies: HashSet::new(),
            last_modified: SystemTime::now(),
            last_accessed: Instant::now(),
        }
    }
}

impl From<HTMLFile> for PageInfo {
    fn from(value: HTMLFile) -> Self {
        Self {
            contents: value.content.into(),
            dependencies: value.dependencies,
            last_modified: SystemTime::now(),
            last_accessed: Instant::now(),
        }
    }
}

impl From<PageInfo> for HTMLFile {
    fn from(value: PageInfo) -> Self {
        Self {
            content: value.contents.unwrap_or_default(),
            dependencies: value.dependencies,
        }
    }
}

impl PageCache {
    pub fn new(max_size: u64) -> Self {
        Self {
            max_size,
            current_size: 0,
            pages: PageInfoMap::new(),
        }
    }
    #[allow(dead_code)]
    pub fn add_page(&mut self, path: PathBuf, page: PageInfo) -> Option<PageInfo> {
        if let Some(con) = &page.contents {
            self.current_size += con.len() as u64;
        }
        let old = self.pages.insert(path, page);
        self.cull();
        old
    }
    pub fn get_page(&self, path: &PathBuf) -> Option<&PageInfo> {
        self.pages.get(path)
    }
    pub fn update_page<F: Fn(&mut PageInfo)>(&mut self, path: PathBuf, f: F) {
        self.pages.entry(path).and_modify(&f).or_insert_with(|| {
            let mut p = PageInfo::default();
            f(&mut p);
            if let Some(con) = &p.contents {
                self.current_size += con.len() as u64;
            }
            p
        });
        self.cull();
    }
    pub fn page_accessed_now(&mut self, path: PathBuf) {
        self.update_page(path, |p| {
            p.last_accessed = Instant::now();
        });
    }
    pub fn has_page(&self, path: &PathBuf) -> bool {
        self.pages.contains_key(path)
    }
    pub fn page_contents_loaded(&self, path: &PathBuf) -> bool {
        self.pages.get(path).map(|p| p.contents.is_some()).unwrap_or(false)
    }
    fn cull(&mut self) {
        if self.current_size <= self.max_size {
            return;
        }
        log::debug!("Performing cache cull");
        let now = Instant::now();
        let mut over: i64 = (self.current_size - self.max_size).try_into().unwrap();
        log::debug!(
            "Cache contents: {} bytes",
            self.current_size
        );
        log::debug!(
            "Cache contents are {over} bytes over the imposed limit of {} bytes",
            self.max_size
        );
        let mut scored = self
            .pages
            .iter()
            .map(|(p, page)| {
                (
                    p.to_owned(),
                    page.get_score(&now),
                    page.contents.as_ref().map(|c| c.len()).unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        scored.sort_by(|a, b| a.1.cmp(&b.1));
        let mut freed = 0;
        let to_remove = scored
            .into_iter()
            .take_while(|(_, _, sz)| {
                let test = over >= 0;
                if test{
                    over -= *sz as i64;
                    freed += *sz as u64;
                }
                test
            })
            .map(|(p, _, sz)| (p, sz))
            .collect::<Vec<_>>();
        log::debug!("Cached files to remove {:?}", to_remove);
        for (path, _) in to_remove {
            self.pages.entry(path).and_modify(|p|{
                p.contents = None
            });
        }
        log::debug!("Freed {freed} bytes from cache memory");
        self.current_size -= freed;
    }
}
