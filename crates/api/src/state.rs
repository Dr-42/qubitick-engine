//! `AppState` — shared server state (CodeArch §5.13).
//!
//! One server, many projects. The *app* root is fixed at startup (it holds
//! `web-next/`, the settings file and the project directories); the *project* is a
//! corpus directory that can be swapped while the server runs, because a project is
//! just a directory with a charter in it.
//!
//! The state is shaped so that a switch does not reshape every handler: the DB and
//! the symbol context keep their places and are replaced in place, and the two
//! things a handler now asks for — the active corpus path, and a read of the
//! context — are accessors.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use qubitick_ctx::Ctx;
use qubitick_db::{Db, ExprCache};
use qubitick_rewrite::diff::RewriteRule;

/// The scope LABEL the app's context is read at — not a place anything is ingested into.
///
/// Ingest gives every node the scope its own path implies (`pipeline::scope_of_path`), and the
/// app's context is built from `symbols_all` — every symbol of the project, under this one label.
/// Reading the label back as a FILTER is what once gave the interface only `Era0/Strata0`'s
/// symbols; the measurement is in `open_project` below.
pub const SCOPE: &str = "Era0/Strata0";

/// A project as the server sees it: a directory that carries a charter.
#[derive(Debug, Clone)]
pub struct Project {
    /// The corpus directory's PATH — `path.display()` — which is the id a client switches by.
    ///
    /// A path and not a name, because the engine runs from anywhere and the corpora are the
    /// user's: two projects may both be called `corpus`, on two different drives, and a wired-in
    /// path need not be under the projects root at all.
    pub id: String,
    /// The name the charter gives it (LaTeX allowed — display only).
    pub name: String,
    /// The directory that holds the charter and `Era0/…`: under the projects root, or wherever
    /// the user wired it in.
    pub corpus: PathBuf,
}


/// The corpus's citable equations, as the citation lookup hands them out: each rule with the
/// citation that names it.
pub type CiteRules = Vec<(String, RewriteRule)>;

/// Shared application state held by the axum router.
pub struct AppState {
    /// Fixed at startup. Holds `web-next/`, `web-settings.json` and the projects.
    pub app_root: PathBuf,
    /// Directory holding the project directories (one per corpus).
    pub projects_root: PathBuf,
    /// The active project's id and corpus path; swapped by a switch.
    pub active: RwLock<Project>,
    /// SQLite connection for the active project (rusqlite `Connection` is !Sync,
    /// so it lives behind a mutex). Replaced wholesale on a switch.
    pub db: Mutex<Db>,
    /// Symbol context for the active project, replaced with the DB.
    pub ctx: RwLock<Ctx>,
    /// RAM hash-consed expression cache.
    pub cache: Arc<ExprCache>,
    /// A shared lock so build operations run serially.
    pub build_lock: Mutex<()>,
    /// The corpus as a rewrite system: every citable equation with the citation that names it
    /// (`A1.5`, `D.BT.02`, `T.1`), built on first use and dropped when the project changes.
    ///
    /// Not built per request: `qubitick_build::cite_rules` reads every axiom, definition and
    /// theorem through the loader — about a second on this repo's own 329 files, against 40ms
    /// once the rules are in hand. A citation an author waits a second for, on every citation,
    /// is one they stop asking for.
    pub rules: RwLock<Option<Arc<CiteRules>>>,
}

impl AppState {
    /// Build app state, choosing the project to start with.
    ///
    /// The default is `project` when it is a charter-bearing directory (which is
    /// this repo's own corpus), else the first project found, so an install that
    /// keeps its corpora elsewhere still boots.
    pub fn new(app_root: PathBuf, projects_root: Option<PathBuf>) -> anyhow::Result<Self> {
        let app_root = canonical_or(app_root);
        let projects_root = canonical_or(projects_root.unwrap_or_else(|| app_root.clone()));
        let registered = crate::settings::load(&app_root)
            .get("projects")
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        let projects = list_projects(&projects_root, &registered);
        /* This repo's own corpus is the default, and it is the directory CALLED `project` — not
           the project whose id is the string `project`. `id` is a path (`list_projects` sets it
           from `path.display()`), so the comparison that used to stand here was one that can
           never be true: the rule read as implemented and was dead, and a checkout still started
           on its own corpus only because the repo has one charter-bearing directory at its root
           and the fallback caught it. A projects root with `aces/` and `project/` is where it
           shows — `aces` would have been served. */
        let start = projects
            .iter()
            .find(|p| p.corpus.file_name().is_some_and(|n| n.to_string_lossy() == "project"))
            .or_else(|| projects.first())
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no project found under {} — a project is a directory containing Canon.qbit",
                    projects_root.display()
                )
            })?;
        let (db, ctx) = open_project(&start.corpus)?;
        Ok(AppState {
            app_root,
            projects_root,
            active: RwLock::new(start),
            db: Mutex::new(db),
            ctx: RwLock::new(ctx),
            cache: Arc::new(ExprCache::new()),
            build_lock: Mutex::new(()),
            rules: RwLock::new(None),
        })
    }

    /// The active project's corpus directory (the thing handlers used to build as
    /// `root.join("project")`).
    pub fn corpus(&self) -> PathBuf {
        self.active.read().map(|p| p.corpus.clone()).unwrap_or_default()
    }

    /// The active project, for a handler that wants id and name together.
    pub fn project(&self) -> Project {
        self.active.read().map(|p| p.clone()).unwrap_or(Project {
            id: String::new(),
            name: String::new(),
            corpus: PathBuf::new(),
        })
    }

    /// Every project: discovered under the projects root, plus wired-in paths.
    pub fn projects(&self) -> Vec<Project> {
        list_projects(&self.projects_root, &self.registered())
    }

    /// The paths the user has wired in, from the workspace settings.
    pub fn registered(&self) -> Vec<String> {
        crate::settings::load(&self.app_root)
            .get("projects")
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str())
                    /* A path an older build stored — relative, or with a trailing slash — is
                       resolved on the way out, so the settings keep naming the project they meant
                       (and a request to forget it under another spelling still finds it). */
                    .map(|s| canonical_or(PathBuf::from(s)).display().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Wire in another project path (idempotent). Returns the project that path IS.
    ///
    /// The caller gets the project back rather than assembling an id from the string it typed:
    /// a `~` is expanded here, a trailing slash is the caller's, and a project's NAME is in its
    /// charter and nowhere in its path (crates/api/src/handlers.rs::post_projects).
    pub fn register(&self, path: &Path) -> anyhow::Result<Project> {
        if let Some(problem) = project_error(path) {
            anyhow::bail!(problem);
        }
        let mut list = self.registered();
        let entry = path.display().to_string();
        if !list.contains(&entry) {
            list.push(entry);
        }
        let patch = serde_json::json!({ "projects": list });
        let current = crate::settings::load(&self.app_root);
        let merged = crate::settings::merge(&current, &patch);
        crate::settings::save(&self.app_root, &merged)?;
        self.projects()
            .into_iter()
            .find(|p| p.corpus == path)
            .ok_or_else(|| {
                /* Unreachable while `project_error` above has just passed: what is left is the
                   settings store not taking the write, and this says exactly that rather than
                   repeating the charter complaint about a directory that has one. */
                anyhow::anyhow!(
                    "{} carries a charter but is not among the projects the engine reads back \
                     — the settings store did not take it",
                    path.display()
                )
            })
    }

    /// Forget a path the user wired in, and answer with the projects that are left.
    ///
    /// Only what `registered` holds can be forgotten. A directory under the projects ROOT is
    /// discovered rather than remembered: dropping it from the settings would not make it go
    /// away — the next read finds it again — and a promise the next boot breaks is worse than no
    /// promise. So forgetting one of those is a no-op, not an error: a client that re-sends its
    /// own list must not fail, and the answer (the list as it now stands) says what happened.
    ///
    /// The ACTIVE project is refused. The list is what the next boot starts from, so forgetting
    /// the project being served would leave a running session serving something its own list does
    /// not contain, and `switch` could never come back to it.
    pub fn unregister(&self, path: &Path) -> anyhow::Result<Vec<Project>> {
        if self.project().corpus == path {
            anyhow::bail!(
                "{} is the project being served — switch to another one first",
                path.display()
            );
        }
        let entry = path.display().to_string();
        let mut list = self.registered();
        let before = list.len();
        list.retain(|p| p != &entry);
        if list.len() != before {
            let patch = serde_json::json!({ "projects": list });
            let current = crate::settings::load(&self.app_root);
            let merged = crate::settings::merge(&current, &patch);
            crate::settings::save(&self.app_root, &merged)?;
        }
        Ok(self.projects())
    }

    /// The active corpus as a rewrite system, built once per project.
    ///
    /// `lhs → rhs` per citable equation (axioms, the `Sema:` of a definition, a theorem's
    /// statement), each labelled with the citation a proof would write. This is what turns an
    /// autocite answer into "…because of `T.1`" instead of a shape that merely looked close.
    pub fn rules(&self) -> anyhow::Result<Arc<Vec<(String, RewriteRule)>>> {
        if let Some(hit) = self.rules.read().ok().and_then(|slot| slot.clone()) {
            return Ok(hit);
        }
        let built: Vec<(String, RewriteRule)> = {
            /* DB before context, the order `GET /api/stats` takes them in: two orders in one
               process is a deadlock waiting for the first concurrent pair. */
            let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            let ctx = self.ctx.read().map_err(|_| anyhow::anyhow!("ctx lock poisoned"))?;
            qubitick_build::cite_rules(&ctx, &conn)?
                .into_iter()
                .map(|c| (c.cite, c.rule))
                .collect()
        };
        let rules = Arc::new(built);
        if let Ok(mut slot) = self.rules.write() {
            *slot = Some(rules.clone());
        }
        Ok(rules)
    }

    /// Switch the server to another project: open its DB, ingest its corpus, build
    /// its context, then swap. On any failure the old project stays active.
    pub fn switch(&self, id: &str) -> anyhow::Result<Project> {
        let Some(next) = self.projects().into_iter().find(|p| p.id == id) else {
            anyhow::bail!("no such project: {id}");
        };
        let (db, ctx) = open_project(&next.corpus)?;
        {
            let mut slot = self.db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
            *slot = db;
        }
        if let Ok(mut slot) = self.ctx.write() {
            *slot = ctx;
        }
        if let Ok(mut slot) = self.active.write() {
            *slot = next.clone();
        }
        /* the equations are THIS corpus's: a switch that left them behind would answer questions
           about the other project's axioms with this one's citations */
        if let Ok(mut slot) = self.rules.write() {
            *slot = None;
        }
        Ok(next)
    }
}

/// Open (and resync) one project's database and symbol context.
fn open_project(corpus: &Path) -> anyhow::Result<(Db, Ctx)> {
    // The DB lives inside the project: it is that corpus's cache, and swapping
    // projects must not make two corpora share one.
    let db_path = corpus.join("qubitick.db");
    let mut conn = qubitick_db::open(&db_path)?;
    qubitick_build::ingest_corpus(&mut conn, corpus)?;
    /* THE APP'S CONTEXT IS THE PROJECT, not one stratum of it. `SCOPE` is a LABEL, not a place:
       ingest gives every node the scope its path implies (`pipeline::scope_of_path`), so asking
       for `Era0/Strata0` here saw only the symbols written in that one stratum. Measured on the
       same three-node corpus: 217 symbols in `Era0/Strata0`, ONE (`=`) when the files sat in
       `Era0/Strata1` — the interface could not parse the corpus it was showing, and the corpus
       was entirely the user's to organise. What a FILE sees is a different question and it is
       asked per file, by the build (crates/build/src/pipeline.rs::scope_ctx). */
    let ctx = Ctx::from_rows(SCOPE, qubitick_db::queries::symbols_all(&conn)?);
    Ok((conn, ctx))
}

/// Every project a single executable can serve.
///
/// Two sources, because the engine runs from anywhere and the corpora are the
/// user's: directories under the projects root that carry a charter, plus whatever
/// paths the user has wired in through the UI (kept in the workspace settings). A
/// directory is a project when it carries `Canon.qbit`; nothing is configured by
/// convention beyond that.
/// A path made absolute and canonical where it can be, and left alone where it cannot.
///
/// THE ROOTS ARE RESOLVED ONCE, HERE, so every project id the engine hands out is a path that means
/// the same thing from anywhere. `--root` defaults to `.`, which used to make the id of this repo's
/// own corpus `./project` — a string that is only a place if you know which directory the server
/// was started in. The client shows that id, wires it back on the next request, and files its saved
/// workspace under it, so it has to be the directory itself. Canonicalizing is also what makes
/// `./project`, `/abs/…/project/` and `file:///abs/…/project` ONE project instead of three rows of
/// the picker.
pub fn canonical_or(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

pub fn list_projects(root: &Path, registered: &[String]) -> Vec<Project> {
    let mut out: Vec<Project> = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join("Canon.qbit").is_file() {
            continue;
        }
        let id = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let name = qubitick_build::read_canon(&path)
            .ok()
            .flatten()
            .map(|c| c.name)
            .unwrap_or_else(|| id.clone());
        out.push(Project {
            id: path.display().to_string(),
            name,
            corpus: path,
        });
        let _ = id;
    }
    // Paths the user wired in explicitly; a path that is not a project is skipped
    // rather than reported, because it may simply be on a drive that is not mounted.
    for path in registered {
        let dir = PathBuf::from(path);
        if !dir.join("Canon.qbit").is_file() {
            continue;
        }
        let id = dir.display().to_string();
        if out.iter().any(|p| p.id == id) {
            continue;
        }
        let name = qubitick_build::read_canon(&dir)
            .ok()
            .flatten()
            .map(|c| c.name)
            .unwrap_or_else(|| id.clone());
        out.push(Project {
            id,
            name,
            corpus: dir,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    out
}

/// A project is a directory that carries a charter. This is the one rule a user has
/// to know to wire in a path, so the message says it.
pub fn project_error(path: &Path) -> Option<String> {
    if !path.is_dir() {
        return Some(format!("{} is not a directory", path.display()));
    }
    if !path.join("Canon.qbit").is_file() {
        return Some(format!(
            "{} has no Canon.qbit — a project is a directory whose charter names it",
            path.display()
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /* THE FIXTURES ARE DIRECTORIES, and they have to be: this module's whole subject is a PATH.
       A project is a directory that carries a charter and can live anywhere the server can read,
       so a test that stubbed the filesystem would test the stub. Each fixture is a throwaway
       directory under `temp_dir` whose name carries the process id, the way the engine's other
       scratch-corpus tests do it — and every test removes its root at the end. */

    /// A throwaway app root, with its workspace settings ALREADY beside it.
    ///
    /// `settings::path_for` looks for `<root>/web-settings.json` and, when there is none, falls
    /// back to `$XDG_CONFIG_HOME/qubitick/web-settings.json` — the DEVELOPER's own workspace.
    /// `register` writes there, so a state test without this would wire its paths into a real
    /// user's settings file and read them back: it would pass while measuring the fallback rule
    /// instead of the one under test. Writing the file first pins `path_for` to the fixture, and
    /// the assertion below is what says so rather than assuming it.
    fn scratch(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("qb-state-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("web-settings.json"), "{}").unwrap();
        assert_eq!(
            crate::settings::path_for(&root),
            root.join("web-settings.json"),
            "the fixture owns its settings file"
        );
        root
    }

    /// A corpus that ingests: a charter that names it, and one axiom with real glyphs in it.
    ///
    /// `where_` is the directory the nodes go in. It is a parameter because the two projects in
    /// `a_switch_serves_the_other_project_whole` must be told apart, and their SCOPES are the
    /// thing that tells them apart: the fixture is what proves that the app's context is the
    /// project (`symbols_all`) rather than one stratum of it.
    fn corpus(dir: &Path, title: &str, where_: &str) -> PathBuf {
        let nodes = dir.join(where_);
        std::fs::create_dir_all(&nodes).unwrap();
        let id = dir.file_name().unwrap().to_string_lossy().to_string();
        std::fs::write(
            dir.join("Canon.qbit"),
            format!("P. {title}\nId: {id}\nKind: project\n"),
        )
        .unwrap();
        std::fs::write(
            nodes.join("A1_Base.qbit"),
            "A1. Base\n\\forall a b c : \\mathfrak{E}, (a \\Delta b) \\Delta c = a \\Delta (b \\Delta c)\n\
             a \\Delta \\varnothing = a\n",
        )
        .unwrap();
        dir.to_path_buf()
    }

    /// The symbols a context holds, as (scope, tex) — order-free, so a set comparison says
    /// "same context" without depending on the order rows come back in.
    fn symbols_of(ctx: &Ctx) -> BTreeSet<(String, String)> {
        ctx.symbols()
            .map(|s| (s.scope.clone(), s.tex.clone()))
            .collect()
    }

    #[test]
    fn a_directory_is_a_project_only_when_it_carries_a_charter() {
        let root = scratch("list");
        let mine = corpus(&root.join("mine"), "Mine", "Era0/Strata0");
        std::fs::create_dir_all(root.join("no-charter")).unwrap();
        std::fs::write(root.join("loose.qbit"), "A1. Base\n").unwrap();

        let found = list_projects(&root, &[]);

        assert_eq!(found.len(), 1, "one charter-bearing directory, and only it: {found:?}");
        assert_eq!(found[0].corpus, mine);
        assert_eq!(found[0].id, mine.display().to_string(), "the id IS the path");
        assert_eq!(found[0].name, "Mine", "the name comes from the charter, not the directory");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wired_in_path_is_a_project_and_a_dud_one_is_skipped_in_silence() {
        let root = scratch("registered");
        corpus(&root.join("alpha"), "Alpha", "Era0/Strata0");
        let wired = corpus(&root.join("elsewhere/zeta"), "Zeta", "Era0/Strata0");
        std::fs::create_dir_all(root.join("dud")).unwrap();
        let gone = root.join("not-mounted");

        let registered: Vec<String> = vec![
            wired.display().to_string(),
            root.join("dud").display().to_string(),
            gone.display().to_string(),
        ];
        let found = list_projects(&root, &registered);
        let names: Vec<&str> = found.iter().map(|p| p.name.as_str()).collect();

        assert_eq!(names, vec!["Alpha", "Zeta"], "a wired-in path joins the discovered ones, sorted by name");
        assert_eq!(found[1].corpus, wired, "and it is served from where it is, not from under the root");
        /* The dud and the missing path are SKIPPED, not reported: a wired-in path may be on a
           drive that is not mounted, and a project list that refuses to load because of one is
           worse than a list that is one entry short. That is the recorded behaviour — this is
           where it is stated. */
        assert!(!names.contains(&"dud"), "no charter: not a project");
        assert_eq!(found.len(), 2, "and a path that is not there is not an error: {found:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_refusal_names_the_rule_it_broke() {
        let root = scratch("refusal");
        let wired = corpus(&root.join("mine"), "Mine", "Era0/Strata0");
        assert_eq!(project_error(&wired), None);

        /* These two sentences go straight into the interface (the project tile shows the refusal
           in place of a value), so they are tested as prose a user has to be able to act on. */
        let flat = root.join("flat").join("not-here");
        let said = project_error(&flat).expect("a path that is not a directory is refused");
        assert!(said.contains("is not a directory"), "names what it found: {said}");

        std::fs::create_dir_all(root.join("charterless")).unwrap();
        let said = project_error(&root.join("charterless")).expect("a directory with no charter is refused");
        assert!(
            said.contains("Canon.qbit") && said.contains("charter"),
            "and a directory the user got wrong is told what makes a project: {said}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn every_project_id_is_an_absolute_path_however_it_was_written_down() {
        let root = scratch("canonical-id");
        corpus(&root.join("project"), "Mine", "Era0/Strata0");
        let canonical = std::fs::canonicalize(root.join("project")).unwrap();

        /* A settings file that remembers the same directory with a trailing slash: the shape an
           older build could store, and the shape a person types. It must not become a second row
           beside the discovered project — that is the whole point of resolving on the way out. */
        std::fs::write(
            root.join("web-settings.json"),
            serde_json::json!({ "projects": [format!("{}/", canonical.display())] }).to_string(),
        )
        .unwrap();

        let state = AppState::new(root.clone(), Some(root.clone())).expect("a root with a project boots");
        assert_eq!(
            state.project().id,
            canonical.display().to_string(),
            "the id is the directory, absolute — not the string the server was started with"
        );
        assert_eq!(
            state.projects().iter().filter(|p| p.corpus == canonical).count(),
            1,
            "one directory is one row, not one per spelling"
        );
        assert!(
            state.registered().iter().all(|p| p.starts_with('/')),
            "and a wired-in path read back from the settings is absolute too: {:?}",
            state.registered()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_server_boots_on_the_project_called_project_or_the_first_one_it_finds() {
        let root = scratch("boot");
        corpus(&root.join("aaa"), "Aaa", "Era0/Strata0");
        corpus(&root.join("project"), "This Repo's Own", "Era0/Strata0");

        let state = AppState::new(root.clone(), Some(root.clone())).expect("a root with projects boots");
        assert_eq!(
            state.corpus(),
            root.join("project"),
            "the repo's own corpus is the default even though it sorts second"
        );
        assert_eq!(state.project().name, "This Repo's Own");

        /* An install that keeps its corpora elsewhere has no directory called `project`, and it
           still has to boot: the first project by name is the start. */
        std::fs::remove_dir_all(root.join("project")).unwrap();
        let state = AppState::new(root.clone(), Some(root.clone())).expect("and one without it boots too");
        assert_eq!(state.corpus(), root.join("aaa"), "the first by name, rather than no server at all");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_path_that_is_not_a_project_is_refused_and_nothing_is_written() {
        let root = scratch("refuse-register");
        corpus(&root.join("project"), "Own", "Era0/Strata0");
        let state = AppState::new(root.clone(), Some(root.clone())).unwrap();

        let before = crate::settings::load(&root);
        std::fs::create_dir_all(root.join("charterless")).unwrap();
        let err = state
            .register(&root.join("charterless"))
            .expect_err("a directory that is not a project is refused");
        assert!(format!("{err}").contains("Canon.qbit"), "the refusal is the tile's message: {err}");
        let err = state
            .register(&root.join("not-there-either"))
            .expect_err("and a path that does not exist is refused too");
        assert!(format!("{err}").contains("is not a directory"), "with what it found instead: {err}");
        assert_eq!(
            crate::settings::load(&root),
            before,
            "a refused path is not remembered: the list is what the NEXT boot reads"
        );
        assert!(state.registered().is_empty(), "and nothing was wired in");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wired_in_path_can_be_forgotten_and_the_served_one_cannot() {
        let root = scratch("forget");
        let own = corpus(&root.join("project"), "Own", "Era0/Strata0");
        corpus(&root.join("zzz"), "Discovered", "Era0/Strata0");
        let wired = corpus(&root.join("somewhere/wired"), "Wired", "Era0/Strata0");
        let state = AppState::new(root.clone(), Some(root.clone())).unwrap();
        state.register(&wired).expect("the wired path carries a charter");

        assert_eq!(state.projects().len(), 3, "two discovered, one wired: {:?}", state.projects());

        /* THE SERVED PROJECT IS REFUSED. The list is what the next boot starts from: forgetting
           the one being served would leave a running session serving a project its own list does
           not contain, and `switch` could never come back to it. */
        let err = state
            .unregister(&own)
            .expect_err("the project being served cannot be forgotten");
        assert!(format!("{err}").contains("being served"), "and the refusal says why: {err}");
        assert_eq!(state.projects().len(), 3, "it is still there");

        /* THE WIRED ONE GOES, and it goes from the settings — the list the next boot reads — not
           just from this answer. */
        let left = state.unregister(&wired).expect("a wired-in path can be forgotten");
        assert!(state.registered().is_empty(), "the settings no longer name it");
        assert!(left.iter().all(|p| p.corpus != wired), "and it is out of the list: {left:?}");
        assert_eq!(
            list_projects(&root, &state.registered()).len(),
            2,
            "a fresh read of the same state agrees, which is what a restart does"
        );
        state.unregister(&wired).expect("forgetting it twice is not an error");
        assert_eq!(state.projects().len(), 2, "and it does not remove anything else");

        /* A DISCOVERED DIRECTORY is found rather than remembered, so forgetting it is a no-op
           that ANSWERS with the list — the client may be re-sending a list it drew earlier, and a
           refusal there would be a failure it cannot act on. The answer being one entry longer
           than the caller expected is the honest answer. */
        let left = state.unregister(&root.join("zzz")).expect("forgetting a discovered project is not an error");
        assert!(left.iter().any(|p| p.corpus == root.join("zzz")), "it is still a project: {left:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_switch_serves_the_other_project_whole() {
        let root = scratch("switch");
        /* two projects of the SAME nodes, in different strata: the scope a symbol was written in
           is what tells the two contexts apart, which is exactly the question `switch` has to
           answer — is the engine now serving B, or B's file cache with A's context? */
        let a = corpus(&root.join("a"), "A", "Era0/Strata0");
        let b = corpus(&root.join("b"), "B", "Era1/Strata2");
        let state = AppState::new(root.clone(), Some(root.clone())).unwrap();
        assert_eq!(state.corpus(), a, "the fixture's own corpus is the default");

        let a_ctx = symbols_of(&state.ctx.read().unwrap());
        assert!(!a_ctx.is_empty(), "project A's context is not empty: {a_ctx:?}");
        /* every symbol EXCEPT the one the engine itself reserves: `=` (EQ_SYM) is inserted at the
           GLOBAL scope so any era's statements can use it (`ingest` says so where it inserts it),
           and it is the same row in every project — it is not evidence about whose corpus this
           is, which is what this assertion is for */
        assert!(
            a_ctx.iter().filter(|(scope, _)| !scope.is_empty()).all(|(scope, _)| scope == "Era0/Strata0"),
            "A's symbols are at A's paths' scope: {a_ctx:?}"
        );

        let wired = state.register(&b).expect("B carries a charter, so it can be wired in");
        assert_eq!(wired.corpus, b);
        assert_eq!(wired.name, "B");
        assert_eq!(state.registered().len(), 1);
        state.register(&b).expect("registering the same path again is a no-op, not a second entry");
        assert_eq!(state.registered().len(), 1, "idempotent: a tile that re-sends its path must not grow the list");

        let served = state.switch(&wired.id).expect("switching to a project the state lists works");
        assert_eq!(served.corpus, b);
        assert_eq!(state.corpus(), b, "the handlers' corpus is B now");
        assert_eq!(state.project().name, "B");

        let b_ctx = symbols_of(&state.ctx.read().unwrap());
        assert!(!b_ctx.is_empty(), "and B's context is not empty: {b_ctx:?}");
        assert_ne!(a_ctx, b_ctx, "the context is B's, not the one the server started on");
        assert!(
            b_ctx.iter().filter(|(scope, _)| !scope.is_empty()).all(|(scope, _)| scope == "Era1/Strata2"),
            "every symbol is at the scope B's paths imply — the context is the PROJECT, not one \
             stratum and not the harness's first: {b_ctx:?}"
        );
        /* the DB moved with the corpus: each project keeps its own cache inside itself, so two
           corpora cannot share one symbol table */
        assert!(b.join("qubitick.db").is_file(), "B's cache is inside B");
        assert!(a.join("qubitick.db").is_file(), "and A keeps its own");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_switch_that_cannot_happen_leaves_the_old_project_serving() {
        let root = scratch("failed-switch");
        let a = corpus(&root.join("a"), "A", "Era0/Strata0");
        corpus(&root.join("b"), "B", "Era1/Strata2");
        let state = AppState::new(root.clone(), Some(root.clone())).unwrap();
        let before = symbols_of(&state.ctx.read().unwrap());

        let err = state.switch("no such project").expect_err("an id the list does not hold is refused");
        assert!(format!("{err}").contains("no such project"), "and says which id: {err}");

        /* The interface switches while the user is looking at it: a failed switch that had
           already replaced the DB would leave the session serving neither project. */
        assert_eq!(state.corpus(), a, "still serving A");
        assert_eq!(symbols_of(&state.ctx.read().unwrap()), before, "with the context it had");
        let _ = std::fs::remove_dir_all(&root);
    }
}
