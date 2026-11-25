//! `qubitick-server` — binary launcher for the API router.
//!
//! Initializes `api::AppState`, builds the router, and binds to a port (default
//! 3000).
//!
//! TWO DIRECTORIES, and the difference is the decoupling in one line:
//!
//!   `--root`      where the server keeps ITS things — `web-next/` and the settings.
//!   `--projects`  where the PROJECTS are: every directory under it that carries a `Canon.qbit` is
//!                 one, and `QUBITICK_PROJECTS` says the same thing for a launcher that cannot pass
//!                 a flag. Without it, `--root` is searched, which is how a checkout serves its
//!                 own `./project` with no arguments at all.
//!
//! Neither is a project "registered" in the engine: a corpus is a directory the user owns, and a
//! client can wire in a path that is nowhere near either of them (`POST /api/projects`).

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

#[derive(Parser)]
#[command(name = "qubitick-server")]
struct Cli {
    /// HTTP port to bind (default 3000).
    #[arg(long, short, default_value_t = 3000)]
    port: u16,

    /// Where the server keeps its own files: `web-next/`, the settings (default: `.`).
    #[arg(long, default_value = ".")]
    root: PathBuf,

    /// Directory whose subdirectories are the projects (default: `--root`). Also read from
    /// `QUBITICK_PROJECTS`; the flag wins.
    #[arg(long)]
    projects: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let root = cli.root;

    // Flag first, then the environment: a launcher that cannot pass a flag still gets to say
    // where the corpora are, and a flag is how a shell script says it for one run.
    let projects = cli
        .projects
        .or_else(|| std::env::var("QUBITICK_PROJECTS").ok().map(PathBuf::from));
    let state = Arc::new(qubitick_api::AppState::new(root, projects)?);

    let app = qubitick_api::router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", cli.port)).await?;
    println!("qubitick-server listening on http://localhost:{}", cli.port);
    axum::serve(listener, app).await?;
    Ok(())
}