//! Snapshot-derived tree rows. Keep path splitting and filtering off scroll frames.

use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeRow {
    Folder {
        path: String,
        name: String,
        depth: usize,
        collapsed: bool,
    },
    File {
        index: usize,
        name: String,
        depth: usize,
    },
}

#[derive(Default)]
pub struct TreeCache {
    valid: bool,
    query: String,
    collapsed: HashSet<String>,
    rows: Arc<Vec<TreeRow>>,
}

impl TreeCache {
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// `paths` is lazy: cache hits must not visit even one snapshot file.
    /// The owner invalidates this cache whenever it installs a new snapshot.
    pub fn prepare<'a>(
        &mut self,
        query: &str,
        collapsed: &HashSet<String>,
        paths: impl IntoIterator<Item = &'a str>,
    ) -> Arc<Vec<TreeRow>> {
        if self.valid && self.query == query && self.collapsed == *collapsed {
            return self.rows.clone();
        }
        let query_lower = query.to_lowercase();
        let mut rows = Vec::new();
        let mut folders = HashSet::new();
        for (index, path) in paths.into_iter().enumerate() {
            if !query_lower.is_empty() && !path.to_lowercase().contains(&query_lower) {
                continue;
            }
            let parts: Vec<_> = path.split('/').collect();
            let mut hidden = false;
            for depth in 0..parts.len().saturating_sub(1) {
                let path = parts[..=depth].join("/");
                let is_collapsed = collapsed.contains(&path);
                if folders.insert(path.clone()) {
                    rows.push(TreeRow::Folder {
                        path,
                        name: parts[depth].to_owned(),
                        depth,
                        collapsed: is_collapsed,
                    });
                }
                if is_collapsed {
                    hidden = true;
                    break;
                }
            }
            if !hidden {
                rows.push(TreeRow::File {
                    index,
                    name: parts.last().unwrap_or(&"").to_string(),
                    depth: parts.len().saturating_sub(1),
                });
            }
        }
        self.query = query.to_owned();
        self.collapsed = collapsed.clone();
        self.rows = Arc::new(rows);
        self.valid = true;
        self.rows.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn indices(rows: &[TreeRow]) -> Vec<usize> {
        rows.iter()
            .filter_map(|row| match row {
                TreeRow::File { index, .. } => Some(*index),
                TreeRow::Folder { .. } => None,
            })
            .collect()
    }

    #[test]
    fn scroll_redraws_do_not_visit_snapshot_paths() {
        let paths: Vec<_> = (0..20_000)
            .map(|i| format!("src/group-{}/file-{i}.rs", i / 100))
            .collect();
        let visits = Cell::new(0);
        let iter = || {
            paths.iter().map(|path| {
                visits.set(visits.get() + 1);
                path.as_str()
            })
        };
        let collapsed = HashSet::new();
        let mut cache = TreeCache::default();
        let first = cache.prepare("", &collapsed, iter());
        assert_eq!(visits.get(), paths.len());
        assert_eq!(indices(&first).len(), paths.len());
        for _ in 0..100 {
            assert!(Arc::ptr_eq(&first, &cache.prepare("", &collapsed, iter())));
        }
        assert_eq!(visits.get(), paths.len(), "redraw must not rescan files");
    }

    #[test]
    fn filter_and_folder_changes_keep_snapshot_indices_and_unicode_names() {
        let paths = [
            "src/深/Alpha.rs",
            "src/深/Beta.rs",
            "src/other.rs",
            "README.md",
        ];
        let mut cache = TreeCache::default();
        let mut collapsed = HashSet::new();
        let all = cache.prepare("", &collapsed, paths);
        assert_eq!(indices(&all), vec![0, 1, 2, 3]);
        assert_eq!(
            all.iter()
                .filter(|r| matches!(r, TreeRow::Folder { .. }))
                .count(),
            2
        );
        let filtered = cache.prepare("BETA", &collapsed, paths);
        assert_eq!(indices(&filtered), vec![1]);
        assert!(
            matches!(&filtered[1], TreeRow::Folder { path, name, depth: 1, .. }
            if path == "src/深" && name == "深")
        );
        collapsed.insert("src/深".into());
        let folded = cache.prepare("", &collapsed, paths);
        assert_eq!(indices(&folded), vec![2, 3]);
        assert!(matches!(
            &folded[1],
            TreeRow::Folder {
                collapsed: true,
                ..
            }
        ));
        collapsed.insert("src".into());
        let parent_folded = cache.prepare("", &collapsed, paths);
        assert_eq!(parent_folded.len(), 2);
        assert_eq!(indices(&parent_folded), vec![3]);
        collapsed.clear();
        assert_eq!(*cache.prepare("", &collapsed, paths), *all);
        assert!(cache.prepare("not-present", &collapsed, paths).is_empty());
    }

    #[test]
    fn snapshot_invalidation_and_empty_results_are_cached() {
        let mut cache = TreeCache::default();
        let collapsed = HashSet::new();
        let first = cache.prepare("", &collapsed, ["before.rs"]);
        cache.invalidate();
        let next = cache.prepare("", &collapsed, ["after.rs"]);
        assert!(!Arc::ptr_eq(&first, &next));
        assert!(matches!(&next[0], TreeRow::File { name, .. } if name == "after.rs"));
        cache.invalidate();
        let empty = cache.prepare("", &collapsed, std::iter::empty());
        assert!(empty.is_empty());
        let never_visited = std::iter::from_fn(|| -> Option<&str> {
            panic!("a cached empty result must not rescan the snapshot")
        });
        assert!(Arc::ptr_eq(
            &empty,
            &cache.prepare("", &collapsed, never_visited)
        ));
    }
}
