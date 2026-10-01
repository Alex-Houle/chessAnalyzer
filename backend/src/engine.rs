//! Minimal async UCI driver for a locally installed Stockfish binary.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

const READ_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub path: String,
    pub threads: usize,
    pub hash_mb: usize,
}

/// Score from the point of view of the side to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Score {
    Cp(i32),
    Mate(i32),
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub score: Score,
    /// Principal variation in UCI notation; the first entry is the best move.
    pub pv: Vec<String>,
}

pub struct Engine {
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Engine {
    pub async fn spawn(config: &EngineConfig) -> Result<Self> {
        let mut child = Command::new(&config.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to start Stockfish at `{}`", config.path))?;

        let stdin = child.stdin.take().context("no stdin on engine process")?;
        let stdout = child.stdout.take().context("no stdout on engine process")?;
        let mut engine = Engine {
            _child: child,
            stdin,
            lines: BufReader::new(stdout).lines(),
        };

        engine.send("uci").await?;
        engine.wait_for("uciok").await?;
        engine
            .send(&format!("setoption name Threads value {}", config.threads))
            .await?;
        engine
            .send(&format!("setoption name Hash value {}", config.hash_mb))
            .await?;
        engine.send("ucinewgame").await?;
        engine.send("isready").await?;
        engine.wait_for("readyok").await?;
        Ok(engine)
    }

    async fn send(&mut self, command: &str) -> Result<()> {
        self.stdin.write_all(command.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<String> {
        timeout(READ_TIMEOUT, self.lines.next_line())
            .await
            .map_err(|_| anyhow!("timed out waiting for Stockfish"))??
            .ok_or_else(|| anyhow!("Stockfish exited unexpectedly"))
    }

    async fn wait_for(&mut self, token: &str) -> Result<()> {
        loop {
            if self.read_line().await?.trim() == token {
                return Ok(());
            }
        }
    }

    /// Searches `fen` to a fixed depth. The position must not be terminal.
    pub async fn evaluate(&mut self, fen: &str, depth: u8) -> Result<SearchResult> {
        self.send(&format!("position fen {fen}")).await?;
        self.send(&format!("go depth {depth}")).await?;

        let mut best: Option<SearchResult> = None;
        loop {
            let line = self.read_line().await?;
            if let Some(rest) = line.strip_prefix("bestmove") {
                let mut result = best.ok_or_else(|| anyhow!("Stockfish returned no score"))?;
                let bestmove = rest.split_whitespace().next().unwrap_or_default();
                if bestmove.is_empty() || bestmove == "(none)" {
                    bail!("Stockfish found no legal move for {fen}");
                }
                if result.pv.first().map(String::as_str) != Some(bestmove) {
                    result.pv = vec![bestmove.to_owned()];
                }
                return Ok(result);
            }
            if let Some(info) = parse_info(&line) {
                best = Some(info);
            }
        }
    }

    pub async fn quit(mut self) {
        let _ = self.send("quit").await;
    }
}

/// Parses an `info ... score ... pv ...` line, ignoring bound-only updates.
fn parse_info(line: &str) -> Option<SearchResult> {
    let mut tokens = line.split_whitespace();
    if tokens.next()? != "info" {
        return None;
    }
    let mut score = None;
    while let Some(token) = tokens.next() {
        match token {
            "score" => {
                let kind = tokens.next()?;
                let value: i32 = tokens.next()?.parse().ok()?;
                score = Some(match kind {
                    "cp" => Score::Cp(value),
                    "mate" => Score::Mate(value),
                    _ => return None,
                });
            }
            "lowerbound" | "upperbound" => return None,
            "pv" => {
                return Some(SearchResult {
                    score: score?,
                    pv: tokens.map(str::to_owned).collect(),
                });
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cp_and_mate_lines() {
        let r = parse_info("info depth 12 seldepth 18 multipv 1 score cp -34 nodes 1 nps 1 pv e7e5 g1f3").unwrap();
        assert_eq!(r.score, Score::Cp(-34));
        assert_eq!(r.pv, ["e7e5", "g1f3"]);

        let r = parse_info("info depth 5 score mate 3 nodes 1 pv d1h5").unwrap();
        assert_eq!(r.score, Score::Mate(3));

        assert!(parse_info("info depth 5 score cp 10 lowerbound nodes 1 pv d1h5").is_none());
        assert!(parse_info("info string hello").is_none());
    }
}
