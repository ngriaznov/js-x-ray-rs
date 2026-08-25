//! Upstream: `src/EntryFilesAnalyser.ts`
//!
//! `analyse` is synchronous here (the Node original interleaves concurrent
//! async generators purely for I/O overlap; `AstAnalyser::analyse_file` is
//! already synchronous in this port, so a plain recursive walk produces the
//! same set of reports without needing `combine-async-iterators`). Entry
//! files and dependencies are also accepted as filesystem paths rather than
//! `string | URL`, since the Rust API has no URL-vs-path duality to bridge.
//!
//! `digraph-js` is reimplemented locally as [`DiGraph`], scoped to the two
//! operations the upstream test suite actually exercises on
//! `EntryFilesAnalyser.dependencies`: [`DiGraph::find_cycles`] and
//! [`DiGraph::get_deep_children`]. Upstream's depth-limited traversal is
//! itself a generator wrapped in a generator, so a finite `depthLimit`
//! truncates by *total items pulled* rather than by graph depth; only
//! `depth_limit` faithfully matching that requires reproducing generator
//! step semantics, so `DiGraph` truncates by true depth instead. This is
//! observably identical for `depth_limit <= 1` (the only value exercised
//! upstream) and for the unbounded case `find_cycles` always uses.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use indexmap::{IndexMap, IndexSet};
use serde_json::{Map, Value};

use crate::ast_analyser::{AstAnalyser, AstAnalyserOptions, ReportOnFile, RuntimeOptions};
use crate::collectable_set::DefaultCollectableSet;
use crate::parser::{JsSourceParser, SourceParser, TsSourceParser};
use crate::source_file::SourceFile;

#[derive(Debug, thiserror::Error)]
pub enum EntryFilesAnalyserError {
    #[error("astAnalyzer instance must have a 'dependency' collectable")]
    MissingDependencyCollectable,
}

fn default_extensions() -> Vec<String> {
    JsSourceParser::FILE_EXTENSIONS
        .iter()
        .chain(TsSourceParser::FILE_EXTENSIONS)
        .map(|ext| ext.trim_start_matches('.').to_owned())
        .chain(std::iter::once("node".to_owned()))
        .collect()
}

/// Upstream `ReportOnEntryFile` (`ReportOnFile & { file: string }`), modeled
/// as a wrapper since Rust's `ReportOnFile` is an enum rather than a
/// structurally-extensible object.
#[derive(Debug)]
pub struct ReportOnEntryFile {
    pub file: String,
    pub report: ReportOnFile,
}

impl ReportOnEntryFile {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        matches!(self.report, ReportOnFile::Ok { .. })
    }
}

pub type LoadExtensions = Box<dyn Fn(Vec<String>) -> Vec<String>>;
pub type SourceFileHook = Rc<dyn Fn(&mut SourceFile)>;
pub type FileMetadataHook = Rc<dyn Fn(&Path) -> Map<String, Value>>;

#[derive(Default)]
pub struct EntryFilesAnalyserOptions {
    /// Defaults to an `AstAnalyser` with a `dependency` `DefaultCollectableSet`.
    pub ast_analyzer: Option<AstAnalyser>,
    pub load_extensions: Option<LoadExtensions>,
    pub root_path: Option<PathBuf>,
    pub ignore_enoent: bool,
    pub package_dependencies: HashSet<String>,
}

#[derive(Default)]
pub struct EntryFilesRuntimeOptions {
    pub remove_html_comments: bool,
    pub metadata: Option<Map<String, Value>>,
    pub package_name: Option<String>,
    pub initialize: Option<SourceFileHook>,
    pub finalize: Option<SourceFileHook>,
    pub file_metadata: Option<FileMetadataHook>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Stats {
    pub number_of_imports_detected: usize,
    pub number_of_files_processed: usize,
}

pub struct EntryFilesAnalyser {
    ast_analyzer: AstAnalyser,
    allowed_extensions: IndexSet<String>,
    root_path: Option<PathBuf>,
    ignore_enoent: bool,
    package_dependencies: HashSet<String>,
    pub dependencies: DiGraph,
    pub stats: Stats,
    pub unique_imports: HashSet<PathBuf>,
}

impl EntryFilesAnalyser {
    pub fn new(options: EntryFilesAnalyserOptions) -> Result<Self, EntryFilesAnalyserError> {
        let EntryFilesAnalyserOptions {
            ast_analyzer,
            load_extensions,
            root_path,
            ignore_enoent,
            package_dependencies,
        } = options;

        let ast_analyzer = ast_analyzer.unwrap_or_else(|| {
            AstAnalyser::new(AstAnalyserOptions {
                collectables: vec![DefaultCollectableSet::new("dependency")],
                ..Default::default()
            })
        });
        if ast_analyzer.get_collectable_set("dependency").is_none() {
            return Err(EntryFilesAnalyserError::MissingDependencyCollectable);
        }

        let raw_allowed_extensions = match &load_extensions {
            Some(load_extensions) => load_extensions(default_extensions()),
            None => default_extensions(),
        };

        Ok(Self {
            ast_analyzer,
            allowed_extensions: raw_allowed_extensions.into_iter().collect(),
            root_path,
            ignore_enoent,
            package_dependencies,
            dependencies: DiGraph::default(),
            stats: Stats::default(),
            unique_imports: HashSet::new(),
        })
    }

    pub fn analyse<P: AsRef<Path>>(
        &mut self,
        entry_files: impl IntoIterator<Item = P>,
        options: EntryFilesRuntimeOptions,
    ) -> io::Result<Vec<ReportOnEntryFile>> {
        self.dependencies = DiGraph::default();
        self.stats = Stats::default();
        self.unique_imports.clear();

        let mut dep_path_cache = HashMap::new();
        let mut out = Vec::new();

        let unique_entries: IndexSet<PathBuf> = entry_files
            .into_iter()
            .map(|file| file.as_ref().to_path_buf())
            .collect();

        for entry_file in unique_entries {
            let normalized_entry_file = self.normalize_and_clean_entry_file(&entry_file);

            if self.ignore_enoent && !self.file_exists(&normalized_entry_file)? {
                continue;
            }

            let relative_file = self.get_relative_file_path(&normalized_entry_file);
            self.analyse_file(
                &normalized_entry_file,
                relative_file,
                &options,
                &mut dep_path_cache,
                &mut out,
            )?;
        }

        self.stats.number_of_imports_detected = self.unique_imports.len();
        Ok(out)
    }

    /// Upstream exposes `astAnalyzer` as a public field so callers can read
    /// back collectable sets populated during `analyse`; this mirrors that
    /// with a borrow, since Rust's `AstAnalyser` is owned rather than shared
    /// by reference.
    #[must_use]
    pub fn ast_analyzer(&self) -> &AstAnalyser {
        &self.ast_analyzer
    }

    fn normalize_and_clean_entry_file(&self, file: &Path) -> PathBuf {
        let normalized = normalize_path(file);
        match &self.root_path {
            Some(root) if !normalized.is_absolute() => normalize_path(&root.join(normalized)),
            _ => normalized,
        }
    }

    fn get_relative_file_path(&self, file: &Path) -> String {
        match &self.root_path {
            Some(root) => relative_path(root, file),
            None => file.to_string_lossy().into_owned(),
        }
    }

    fn get_parser_from_file_extension(&self, file: &Path) -> Option<Box<dyn SourceParser>> {
        let extension = format!(".{}", file.extension()?.to_str()?);

        if JsSourceParser::FILE_EXTENSIONS.contains(&extension.as_str()) {
            Some(Box::new(JsSourceParser))
        } else if TsSourceParser::FILE_EXTENSIONS.contains(&extension.as_str()) {
            Some(Box::new(TsSourceParser))
        } else {
            None
        }
    }

    fn analyse_file(
        &mut self,
        file: &Path,
        relative_file: String,
        options: &EntryFilesRuntimeOptions,
        dep_path_cache: &mut HashMap<PathBuf, Option<PathBuf>>,
        out: &mut Vec<ReportOnEntryFile>,
    ) -> io::Result<()> {
        // Skip declaration files as they are not meant to be analysed.
        if file.to_string_lossy().contains("d.ts") {
            return Ok(());
        }
        self.dependencies.add_vertex(relative_file.clone());

        let mut final_metadata = options.metadata.clone().unwrap_or_default();
        if let Some(file_metadata) = &options.file_metadata {
            final_metadata.extend(file_metadata(file));
        }

        let runtime_options = RuntimeOptions {
            remove_html_comments: options.remove_html_comments,
            custom_parser: self.get_parser_from_file_extension(file),
            initialize: options
                .initialize
                .as_ref()
                .map(|f| clone_as_fn_once(Rc::clone(f))),
            finalize: options
                .finalize
                .as_ref()
                .map(|f| clone_as_fn_once(Rc::clone(f))),
            metadata: Some(final_metadata),
            package_name: options.package_name.clone(),
            ..Default::default()
        };

        let report = self.ast_analyzer.analyse_file(file, runtime_options)?;
        self.stats.number_of_files_processed += 1;

        let file_dependencies: Vec<String> = match &report {
            ReportOnFile::Ok { dependencies, .. } => dependencies.keys().cloned().collect(),
            ReportOnFile::Failed { .. } => Vec::new(),
        };
        let is_ok = matches!(report, ReportOnFile::Ok { .. });

        out.push(ReportOnEntryFile {
            file: relative_file.clone(),
            report,
        });
        if !is_ok {
            return Ok(());
        }

        let Some(parent) = file.parent() else {
            return Ok(());
        };

        for name in file_dependencies {
            if self.package_dependencies.contains(&name) {
                continue;
            }

            let joined = normalize_path(&parent.join(&name));
            let Some(dep_file) = self.get_internal_dep_path(&joined, dep_path_cache)? else {
                continue;
            };
            self.unique_imports.insert(dep_file.clone());

            let dep_relative_file = self.get_relative_file_path(&dep_file);
            if !self.dependencies.has_vertex(&dep_relative_file) {
                self.dependencies.add_vertex(dep_relative_file.clone());
                self.analyse_file(
                    &dep_file,
                    dep_relative_file.clone(),
                    options,
                    dep_path_cache,
                    out,
                )?;
            }
            self.dependencies
                .add_edge(relative_file.clone(), dep_relative_file);
        }

        Ok(())
    }

    fn get_internal_dep_path(
        &self,
        file_path: &Path,
        cache: &mut HashMap<PathBuf, Option<PathBuf>>,
    ) -> io::Result<Option<PathBuf>> {
        if let Some(cached) = cache.get(file_path) {
            return Ok(cached.clone());
        }

        let resolved = self.resolve_internal_dep_path(file_path)?;
        cache.insert(file_path.to_path_buf(), resolved.clone());
        Ok(resolved)
    }

    fn resolve_internal_dep_path(&self, file_path: &Path) -> io::Result<Option<PathBuf>> {
        match file_path.extension().and_then(|ext| ext.to_str()) {
            None => {
                for ext in &self.allowed_extensions {
                    let dep_path_with_ext = PathBuf::from(format!("{}.{ext}", file_path.display()));
                    if self.file_exists(&dep_path_with_ext)? {
                        return Ok(Some(dep_path_with_ext));
                    }
                }
                Ok(None)
            }
            Some(extension) => {
                if !self.allowed_extensions.contains(extension) {
                    return Ok(None);
                }
                Ok(self
                    .file_exists(file_path)?
                    .then(|| file_path.to_path_buf()))
            }
        }
    }

    fn file_exists(&self, file_path: &Path) -> io::Result<bool> {
        match std::fs::metadata(file_path) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }
}

fn clone_as_fn_once(f: Rc<dyn Fn(&mut SourceFile)>) -> Box<dyn FnOnce(&mut SourceFile)> {
    Box::new(move |source_file| f(source_file))
}

/// `path.normalize` equivalent: collapses `.` components and resolves `..`
/// against preceding normal components, without touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if matches!(out.last(), Some(Component::Normal(_))) => {
                out.pop();
            }
            other => out.push(other),
        }
    }

    let result: PathBuf = out.iter().collect();
    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

/// `path.relative(from, to)` equivalent for two already-normalized paths.
fn relative_path(from: &Path, to: &Path) -> String {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();
    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(a, b)| a == b)
        .count();

    let parts: Vec<String> = std::iter::repeat_n("..".to_owned(), from_components.len() - common)
        .chain(
            to_components[common..]
                .iter()
                .map(|c| c.as_os_str().to_string_lossy().into_owned()),
        )
        .collect();

    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join(std::path::MAIN_SEPARATOR_STR)
    }
}

#[derive(Debug, Default, Clone)]
struct Vertex {
    adjacent_to: Vec<String>,
}

/// A minimal internal reimplementation of `digraph-js`'s `DiGraph`, scoped
/// to what `EntryFilesAnalyser` and its test suite exercise: vertex/edge
/// bookkeeping, `find_cycles`, and `get_deep_children`.
#[derive(Debug, Default)]
pub struct DiGraph {
    vertices: IndexMap<String, Vertex>,
}

impl DiGraph {
    #[must_use]
    pub fn has_vertex(&self, id: &str) -> bool {
        self.vertices.contains_key(id)
    }

    pub fn add_vertex(&mut self, id: String) {
        self.vertices.entry(id).or_default();
    }

    pub fn add_edge(&mut self, from: String, to: String) {
        if from == to || !self.vertices.contains_key(&to) {
            return;
        }
        if let Some(vertex) = self.vertices.get_mut(&from)
            && !vertex.adjacent_to.contains(&to)
        {
            vertex.adjacent_to.push(to);
        }
    }

    /// Upstream `getDeepChildren`.
    #[must_use]
    pub fn get_deep_children(&self, root_id: &str, depth_limit: usize) -> Vec<String> {
        let Some(root) = self.vertices.get(root_id) else {
            return Vec::new();
        };

        let mut visited = Vec::new();
        let mut out = Vec::new();
        for adjacent_id in root.adjacent_to.clone() {
            if self.vertices.contains_key(&adjacent_id) {
                self.find_deep_dependencies(
                    root_id,
                    &adjacent_id,
                    Some(depth_limit.saturating_sub(1)),
                    &mut visited,
                    &mut out,
                );
            }
        }
        out
    }

    /// Upstream `findDeepDependencies` (top-to-bottom only — `getDeepParents`
    /// is unused by `EntryFilesAnalyser`). `depth_remaining` of `None` means
    /// unlimited.
    fn find_deep_dependencies(
        &self,
        root_id: &str,
        traversed_id: &str,
        depth_remaining: Option<usize>,
        visited: &mut Vec<String>,
        out: &mut Vec<String>,
    ) {
        if visited.iter().any(|id| id == traversed_id) {
            return;
        }
        out.push(traversed_id.to_owned());
        visited.push(traversed_id.to_owned());
        if root_id == traversed_id || depth_remaining == Some(0) {
            return;
        }

        let Some(vertex) = self.vertices.get(traversed_id) else {
            return;
        };
        let next_depth = depth_remaining.map(|depth| depth - 1);
        for adjacent_id in vertex.adjacent_to.clone() {
            if self.vertices.contains_key(&adjacent_id) {
                self.find_deep_dependencies(root_id, &adjacent_id, next_depth, visited, out);
            }
        }
    }

    /// Upstream `findCycles` with the default (unlimited) `maxDepth`.
    ///
    /// # Panics
    ///
    /// Never in practice: `root_id` is looked up via `position` right after
    /// `adjacency_list.contains(&root_id)` was just confirmed true.
    #[must_use]
    pub fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cyclic_paths_with_maybe_duplicates: Vec<Vec<String>> = Vec::new();

        for (root_id, root_adjacent_id) in self.collect_root_adjacency_lists() {
            let mut adjacency_list: Vec<String> = Vec::new();
            let mut visited = Vec::new();
            let mut deep_dependencies = Vec::new();
            self.find_deep_dependencies(
                &root_id,
                &root_adjacent_id,
                None,
                &mut visited,
                &mut deep_dependencies,
            );

            for deep_adjacent_vertex_id in deep_dependencies {
                if !adjacency_list.contains(&deep_adjacent_vertex_id) {
                    adjacency_list.push(deep_adjacent_vertex_id.clone());
                }
                if deep_adjacent_vertex_id == root_id || adjacency_list.contains(&root_id) {
                    let index = adjacency_list
                        .iter()
                        .position(|id| id == &root_id)
                        .expect("root_id was just confirmed present in adjacency_list");
                    let mut vertices_in_cyclic_path = vec![root_id.clone()];
                    vertices_in_cyclic_path.extend(adjacency_list[..=index].iter().cloned());
                    cyclic_paths_with_maybe_duplicates
                        .push(self.backtrack_vertices_involved_in_cycle(vertices_in_cyclic_path));
                }
            }
        }

        keep_unique_vertices_paths(cyclic_paths_with_maybe_duplicates)
    }

    fn collect_root_adjacency_lists(&self) -> Vec<(String, String)> {
        self.vertices
            .iter()
            .flat_map(|(root_id, vertex)| {
                vertex
                    .adjacent_to
                    .iter()
                    .filter(|adjacent_id| self.vertices.contains_key(*adjacent_id))
                    .map(|adjacent_id| (root_id.clone(), adjacent_id.clone()))
            })
            .collect()
    }

    /// Upstream `backtrackVerticesInvolvedInCycle`, ported index-for-index:
    /// the loop counter is not recomputed from the shrinking array length,
    /// matching the original's (surprising but load-bearing) behavior.
    fn backtrack_vertices_involved_in_cycle(&self, mut path: Vec<String>) -> Vec<String> {
        let mut i = path.len();
        while i > 1 {
            let current_node = path[i - 1].clone();
            let is_current_node_parent = path.get(i - 2).is_some_and(|node_before| {
                self.vertices
                    .get(node_before)
                    .is_some_and(|vertex| vertex.adjacent_to.contains(&current_node))
            });
            if !is_current_node_parent {
                path.remove(i - 2);
            }
            i -= 1;
        }

        let mut seen = HashSet::new();
        path.into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect()
    }
}

/// Upstream `keepUniqueVerticesPaths` (a `lodash.uniqWith` call comparing
/// sorted copies), preserving first-occurrence order like `uniqWith`.
fn keep_unique_vertices_paths(paths: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let mut unique: Vec<Vec<String>> = Vec::new();
    for path in paths {
        let mut sorted_path = path.clone();
        sorted_path.sort();
        let is_duplicate = unique.iter().any(|existing| {
            let mut sorted_existing = existing.clone();
            sorted_existing.sort();
            existing.len() == path.len() && sorted_existing == sorted_path
        });
        if !is_duplicate {
            unique.push(path);
        }
    }
    unique
}
