mod analysis;
mod chesscom;
mod engine;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Semaphore;
use tokio::task::AbortHandle;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use analysis::{Eval, GameAnalysis, Prepared};
use chesscom::{ChessCom, ChessComError};
use engine::{Engine, EngineConfig};

const DEFAULT_DEPTH: u8 = 14;
const MIN_DEPTH: u8 = 6;
const MAX_DEPTH: u8 = 24;
/// Finished analyses kept in memory before the oldest is dropped.
const MAX_JOBS: usize = 200;
const MAX_PGN_BYTES: usize = 64 * 1024;

#[derive(Clone)]
enum Job {
    /// Waiting for an engine slot (`started == false`) or being analyzed.
    Active {
        prepared: Arc<Prepared>,
        /// Evaluations received so far, indexed by position.
        evals: Vec<Option<Eval>>,
        depth: u8,
        started: bool,
    },
    Done(Arc<GameAnalysis>),
    Error(String),
}

#[derive(Default)]
struct Jobs {
    by_id: HashMap<u64, Job>,
    /// Insertion order, for eviction. The id doubles as the cache key (PGN + depth hash).
    order: VecDeque<u64>,
    /// Handles for cancelling analyses that are still queued or running.
    tasks: HashMap<u64, AbortHandle>,
}

struct AppState {
    chesscom: ChessCom,
    engine: EngineConfig,
    jobs: Mutex<Jobs>,
    /// Engine processes used per analysis.
    workers: usize,
    /// Bounds how many analyses run at once.
    slots: Semaphore,
}

type Shared = Arc<AppState>;

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<ChessComError> for ApiError {
    fn from(err: ChessComError) -> Self {
        match err {
            ChessComError::NotFound => ApiError(
                StatusCode::NOT_FOUND,
                "No such player or archive on Chess.com.".into(),
            ),
            ChessComError::RateLimited => ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Chess.com is rate limiting requests. Try again in a moment.".into(),
            ),
            ChessComError::Upstream(message) => ApiError(StatusCode::BAD_GATEWAY, message),
        }
    }
}

fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chess_analyzer=info,tower_http=info".into()),
        )
        .init();

    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let engine = EngineConfig {
        path: env_or("STOCKFISH_PATH", "stockfish".to_owned()),
        threads: env_or("STOCKFISH_THREADS", 1usize).max(1),
        hash_mb: env_or("STOCKFISH_HASH_MB", 64),
    };
    // Many single-threaded engines on separate positions scale better than one multi-threaded search.
    let workers = env_or("STOCKFISH_WORKERS", cores.min(8)).max(1);

    // Fail fast if the engine is missing rather than on the first request.
    Engine::spawn(&engine).await?.quit().await;
    tracing::info!(path = %engine.path, workers, "Stockfish is available");

    let user_agent = env_or(
        "CHESSCOM_USER_AGENT",
        concat!("chess-analyzer/", env!("CARGO_PKG_VERSION")).to_owned(),
    );
    let state: Shared = Arc::new(AppState {
        chesscom: ChessCom::new(&user_agent)?,
        engine,
        jobs: Mutex::default(),
        workers,
        slots: Semaphore::new(env_or("MAX_CONCURRENT_ANALYSES", 1usize).max(1)),
    });

    let static_dir = env_or("STATIC_DIR", "../frontend/dist".to_owned());
    let spa = ServeDir::new(&static_dir)
        .fallback(ServeFile::new(format!("{static_dir}/index.html")));

    let app = Router::new()
        .route("/api/players/{username}/archives", get(archives))
        .route("/api/players/{username}/games/{year}/{month}", get(games))
        .route("/api/analysis", post(start_analysis))
        .route("/api/analysis/{id}", get(analysis_status).delete(cancel_analysis))
        .fallback_service(spa)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!(
        "{}:{}",
        env_or("HOST", "127.0.0.1".to_owned()),
        env_or("PORT", 3001u16)
    );
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn check_username(username: &str) -> Result<String, ApiError> {
    if chesscom::valid_username(username) {
        Ok(username.to_ascii_lowercase())
    } else {
        Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Invalid Chess.com username.".into(),
        ))
    }
}

async fn archives(
    State(state): State<Shared>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let username = check_username(&username)?;
    Ok(Json(state.chesscom.archives(&username).await?))
}

async fn games(
    State(state): State<Shared>,
    Path((username, year, month)): Path<(String, u16, u8)>,
) -> Result<impl IntoResponse, ApiError> {
    let username = check_username(&username)?;
    if !(1..=12).contains(&month) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "Invalid month.".into()));
    }
    Ok(Json(state.chesscom.games(&username, year, month).await?))
}

#[derive(Deserialize)]
struct AnalysisRequest {
    pgn: String,
    depth: Option<u8>,
}

async fn start_analysis(
    State(state): State<Shared>,
    Json(request): Json<AnalysisRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.pgn.len() > MAX_PGN_BYTES {
        return Err(ApiError(StatusCode::PAYLOAD_TOO_LARGE, "PGN is too large.".into()));
    }
    let depth = request.depth.unwrap_or(DEFAULT_DEPTH).clamp(MIN_DEPTH, MAX_DEPTH);
    let game = analysis::parse_pgn(&request.pgn)
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    // Identical requests share one job, so results are cached and never computed twice.
    let mut hasher = DefaultHasher::new();
    (&request.pgn, depth).hash(&mut hasher);
    let id = hasher.finish();

    let prepared = Arc::new(Prepared::new(&game));
    {
        let mut jobs = state.jobs.lock().unwrap();
        match jobs.by_id.get(&id) {
            Some(Job::Error(_)) | None => {}
            Some(_) => return Ok(Json(json!({ "id": id.to_string() }))),
        }
        let job = Job::Active {
            prepared: prepared.clone(),
            evals: vec![None; prepared.len()],
            depth,
            started: false,
        };
        if jobs.by_id.insert(id, job).is_none() {
            jobs.order.push_back(id);
        }
        while jobs.order.len() > MAX_JOBS {
            if let Some(oldest) = jobs.order.pop_front() {
                jobs.by_id.remove(&oldest);
            }
        }
    }

    let task_state = state.clone();
    let task = tokio::spawn(async move {
        let state = task_state;
        let update = {
            let state = state.clone();
            move |apply: &dyn Fn(&mut Job)| {
                if let Some(job) = state.jobs.lock().unwrap().by_id.get_mut(&id) {
                    apply(job);
                }
            }
        };
        let outcome = async {
            let _permit = state.slots.acquire().await?;
            update(&|job| {
                if let Job::Active { started, .. } = job {
                    *started = true;
                }
            });
            let on_eval = {
                let update = update.clone();
                Arc::new(move |i: usize, eval: Eval| {
                    update(&|job| {
                        if let Job::Active { evals, .. } = job {
                            evals[i] = Some(eval.clone());
                        }
                    })
                })
            };
            analysis::evaluate_all(&state.engine, prepared.clone(), depth, state.workers, on_eval).await
        }
        .await;
        match outcome {
            Ok(()) => update(&|job| {
                if let Job::Active { evals, .. } = job {
                    *job = Job::Done(Arc::new(analysis::review(&prepared, evals, depth)));
                }
            }),
            Err(err) => {
                tracing::error!("analysis failed: {err:#}");
                update(&|job| *job = Job::Error(err.to_string()));
            }
        }
        state.jobs.lock().unwrap().tasks.remove(&id);
    });
    if !task.is_finished() {
        state.jobs.lock().unwrap().tasks.insert(id, task.abort_handle());
    }

    // u64 ids exceed JavaScript's safe integer range, so they travel as strings.
    Ok(Json(json!({ "id": id.to_string() })))
}

async fn analysis_status(
    State(state): State<Shared>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let not_found = || ApiError(StatusCode::NOT_FOUND, "Unknown analysis job.".into());
    let id: u64 = id.parse().map_err(|_| not_found())?;
    let job = state.jobs.lock().unwrap().by_id.get(&id).cloned();
    Ok(Json(match job.ok_or_else(not_found)? {
        // While the engines work, the client gets every move reviewed so far.
        Job::Active { prepared, evals, depth, started } => json!({
            "status": if started { "running" } else { "queued" },
            "done": evals.iter().flatten().count(),
            "total": evals.len(),
            "result": analysis::review(&prepared, &evals, depth),
        }),
        Job::Done(result) => json!({ "status": "done", "result": result }),
        Job::Error(message) => json!({ "status": "error", "message": message }),
    }))
}

/// Stops an unfinished analysis the client no longer wants; finished ones stay cached.
async fn cancel_analysis(State(state): State<Shared>, Path(id): Path<String>) -> StatusCode {
    if let Ok(id) = id.parse::<u64>() {
        let mut jobs = state.jobs.lock().unwrap();
        // Aborting the task drops its engine pool, which kills the Stockfish processes.
        if let Some(task) = jobs.tasks.remove(&id) {
            task.abort();
            jobs.by_id.remove(&id);
            jobs.order.retain(|&other| other != id);
        }
    }
    StatusCode::NO_CONTENT
}
