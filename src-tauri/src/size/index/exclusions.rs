use std::fs;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

const POSIX_ROOT: &str = "/";

#[derive(Debug, Clone, Default)]
pub struct ExclusionRules {
    pub roots: Vec<PathBuf>,
    pub always_skipped: Vec<PathBuf>,
    pub alias_prefix: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusionVerdict {
    Added,
    HomeWarning,
    MergedChildren(Vec<PathBuf>),
    Duplicate(PathBuf),
    InsideExisting(PathBuf),
    AlwaysSkipped,
    RootRejected,
    NotFound,
    NotAFolder,
}

impl ExclusionVerdict {
    pub fn adds_path(&self) -> bool {
        matches!(
            self,
            Self::Added | Self::HomeWarning | Self::MergedChildren(_)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathVerdict {
    pub path: PathBuf,
    pub verdict: ExclusionVerdict,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionChange {
    pub exclusions: Vec<PathBuf>,
    pub verdicts: Vec<PathVerdict>,
}

impl ExclusionChange {
    pub fn added_paths(&self) -> Vec<PathBuf> {
        self.verdicts
            .iter()
            .filter(|outcome| outcome.verdict.adds_path())
            .map(|outcome| outcome.path.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcilePlan {
    pub prune: Vec<PathBuf>,
    pub walk: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExclusionSet {
    paths: Vec<PathBuf>,
}

impl ExclusionSet {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let mut set = Self { paths };
        set.paths.sort();
        set
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn covers(&self, path: &Path) -> bool {
        self.covering(path).is_some()
    }

    pub fn covering(&self, path: &Path) -> Option<&PathBuf> {
        self.paths.iter().find(|excluded| is_within(path, excluded))
    }

    pub fn matching(&self, path: &Path) -> Option<&PathBuf> {
        self.paths.iter().find(|excluded| same_path(path, excluded))
    }

    pub fn add(&mut self, candidate: &Path, rules: &ExclusionRules) -> PathVerdict {
        let path = match rules.locate(candidate) {
            Ok(path) => path,
            Err(verdict) => {
                return PathVerdict {
                    path: candidate.to_path_buf(),
                    verdict,
                }
            }
        };
        let verdict = self
            .refusal(&path, rules)
            .unwrap_or_else(|| self.admit(&path, rules));
        PathVerdict { path, verdict }
    }

    pub fn remove(&mut self, path: &Path) -> Option<PathBuf> {
        let position = self
            .paths
            .iter()
            .position(|excluded| same_path(path, excluded))?;
        Some(self.paths.remove(position))
    }

    fn refusal(&self, path: &Path, rules: &ExclusionRules) -> Option<ExclusionVerdict> {
        if rules.is_root(path) {
            return Some(ExclusionVerdict::RootRejected);
        }
        if rules.is_always_skipped(path) {
            return Some(ExclusionVerdict::AlwaysSkipped);
        }
        if let Some(existing) = self.matching(path) {
            return Some(ExclusionVerdict::Duplicate(existing.clone()));
        }
        if let Some(covering) = self.covering(path) {
            return Some(ExclusionVerdict::InsideExisting(covering.clone()));
        }
        None
    }

    fn admit(&mut self, path: &Path, rules: &ExclusionRules) -> ExclusionVerdict {
        let (children, kept): (Vec<PathBuf>, Vec<PathBuf>) = std::mem::take(&mut self.paths)
            .into_iter()
            .partition(|excluded| is_within(excluded, path));
        self.paths = kept;
        self.paths.push(path.to_path_buf());
        self.paths.sort();
        if !children.is_empty() {
            return ExclusionVerdict::MergedChildren(children);
        }
        if rules.is_home(path) {
            return ExclusionVerdict::HomeWarning;
        }
        ExclusionVerdict::Added
    }
}

impl ExclusionRules {
    fn locate(&self, candidate: &Path) -> Result<PathBuf, ExclusionVerdict> {
        let canonical = fs::canonicalize(candidate).map_err(|_| ExclusionVerdict::NotFound)?;
        if !canonical.is_dir() {
            return Err(ExclusionVerdict::NotAFolder);
        }
        Ok(self.on_disk_form(canonical))
    }

    fn on_disk_form(&self, canonical: PathBuf) -> PathBuf {
        let Some(prefix) = &self.alias_prefix else {
            return canonical;
        };
        match canonical.strip_prefix(prefix) {
            Ok(rest) if rest.components().next().is_some() => Path::new(POSIX_ROOT).join(rest),
            _ => canonical,
        }
    }

    fn is_root(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| same_path(path, root))
    }

    fn is_always_skipped(&self, path: &Path) -> bool {
        self.always_skipped
            .iter()
            .any(|skipped| is_within(path, skipped))
    }

    fn is_home(&self, path: &Path) -> bool {
        self.home
            .as_deref()
            .is_some_and(|home| same_path(path, home))
    }
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    left == right || folded(left) == folded(right)
}

pub fn is_within(path: &Path, root: &Path) -> bool {
    path.starts_with(root) || folded(path).starts_with(folded(root))
}

pub fn plan_reconcile(stored: &[PathBuf], configured: &[PathBuf]) -> ReconcilePlan {
    let stored_set = ExclusionSet::new(stored.to_vec());
    let configured_set = ExclusionSet::new(configured.to_vec());
    let prune = configured
        .iter()
        .filter(|path| stored_set.matching(path).is_none())
        .cloned()
        .collect();
    let walk = stored
        .iter()
        .filter(|path| !configured_set.covers(path))
        .cloned()
        .collect();
    ReconcilePlan { prune, walk }
}

fn folded(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let normalized: String = text.nfc().flat_map(char::to_lowercase).collect();
    PathBuf::from(normalized)
}
