//! PGN parsing and move-by-move game review on top of the UCI engine.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use tokio::task::JoinSet;
use shakmaty::fen::Fen;
use shakmaty::san::SanPlus;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Move, Position};

use crate::engine::{Engine, EngineConfig, Score};

/// Centipawn value used to represent a forced mate.
const MATE_CP: i32 = 10_000;
/// Plies of the engine line returned alongside each position.
const PV_PLIES: usize = 8;
const MAX_PLIES: usize = 600;

pub struct ParsedGame {
    pub start: Chess,
    pub moves: Vec<Move>,
}

/// Parses the mainline of a PGN, ignoring comments, variations and NAGs.
pub fn parse_pgn(pgn: &str) -> Result<ParsedGame> {
    let mut start_fen = None;
    let mut movetext = String::new();
    for line in strip_comments(pgn).lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if let Some(value) = header_value(line, "FEN") {
                start_fen = Some(value.to_owned());
            }
        } else {
            movetext.push_str(line);
            movetext.push(' ');
        }
    }

    let start: Chess = match start_fen {
        Some(fen) => fen
            .parse::<Fen>()
            .context("invalid FEN header")?
            .into_position(CastlingMode::Standard)
            .map_err(|e| anyhow!("illegal starting position: {e}"))?,
        None => Chess::default(),
    };

    let mut pos = start.clone();
    let mut moves = Vec::new();
    for token in movetext.split_whitespace() {
        if matches!(token, "1-0" | "0-1" | "1/2-1/2" | "*") {
            break;
        }
        if token.starts_with('$') {
            continue;
        }
        // "12.", "12..." and "12.Nf3" all carry a move-number prefix.
        let san = token
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start_matches('.')
            .trim_end_matches(['!', '?']);
        if san.is_empty() {
            continue;
        }
        let m = SanPlus::from_ascii(san.as_bytes())
            .map_err(|_| anyhow!("unreadable move `{token}`"))?
            .san
            .to_move(&pos)
            .map_err(|_| anyhow!("illegal move `{token}` at ply {}", moves.len() + 1))?;
        pos.play_unchecked(m);
        moves.push(m);
        if moves.len() > MAX_PLIES {
            bail!("game is too long to analyze");
        }
    }
    if moves.is_empty() {
        bail!("PGN contains no moves");
    }
    Ok(ParsedGame { start, moves })
}

/// Removes `{...}` comments, `;` line comments and `(...)` variations.
fn strip_comments(pgn: &str) -> String {
    let mut out = String::with_capacity(pgn.len());
    let (mut in_brace, mut paren_depth, mut in_line_comment, mut in_header) = (false, 0u32, false, false);
    for c in pgn.chars() {
        if c == '\n' {
            in_line_comment = false;
            in_header = false;
        }
        match c {
            _ if in_line_comment => continue,
            _ if in_header => out.push(c),
            '}' if in_brace => in_brace = false,
            _ if in_brace => {}
            '{' => in_brace = true,
            '(' => paren_depth += 1,
            ')' if paren_depth > 0 => paren_depth -= 1,
            _ if paren_depth > 0 => {}
            ';' => in_line_comment = true,
            '[' => {
                in_header = true;
                out.push(c);
            }
            _ => out.push(c),
        }
        if in_brace || paren_depth > 0 {
            // Keep tokens on either side of a comment separated.
            if !out.ends_with(' ') {
                out.push(' ');
            }
        }
    }
    out
}

fn header_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let rest = line.strip_prefix('[')?.strip_prefix(name)?.trim_start();
    rest.strip_prefix('"')?.split('"').next()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Classification {
    Best,
    Excellent,
    Good,
    Inaccuracy,
    Mistake,
    Blunder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Missed {
    /// The player had a forced mate and let it go.
    Mate,
    /// The opponent had just erred and the player failed to punish it.
    Tactic,
}

/// Engine evaluation of one position, from White's point of view.
#[derive(Debug, Clone, Serialize)]
pub struct Eval {
    /// Centipawns; forced mates are mapped near ±10000.
    pub cp: i32,
    /// Moves until mate; positive when White is mating. `0` means the game is over by checkmate.
    pub mate: Option<i32>,
    /// White's winning chances in percent.
    pub win: f64,
    pub best_uci: Option<String>,
    pub best_san: Option<String>,
    /// Engine line in SAN starting with the best move.
    pub pv: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionView {
    pub fen: String,
    /// `None` until the engine has searched this position.
    pub eval: Option<Eval>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MoveReview {
    pub ply: usize,
    pub color: &'static str,
    pub san: String,
    pub uci: String,
    /// `None` until the positions before and after the move are both evaluated.
    pub classification: Option<Classification>,
    pub missed: Option<Missed>,
    /// Winning chances given up by this move, in percentage points.
    pub win_loss: f64,
    pub cp_loss: i32,
    pub comment: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PlayerSummary {
    pub accuracy: f64,
    pub acpl: f64,
    pub best: u32,
    pub excellent: u32,
    pub good: u32,
    pub inaccuracy: u32,
    pub mistake: u32,
    pub blunder: u32,
    pub missed: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameAnalysis {
    pub depth: u8,
    /// One entry per position: index 0 is the start, index `n` follows move `n`.
    pub positions: Vec<PositionView>,
    pub moves: Vec<MoveReview>,
    pub white: PlayerSummary,
    pub black: PlayerSummary,
}

struct Played {
    color: Color,
    san: String,
    uci: String,
}

/// Every position of a game, replayed once so the engine workers can share it.
pub struct Prepared {
    positions: Vec<Chess>,
    fens: Vec<String>,
    played: Vec<Played>,
}

impl Prepared {
    pub fn new(game: &ParsedGame) -> Self {
        let fen = |pos: &Chess| Fen::from_position(pos, EnPassantMode::Legal).to_string();
        let mut pos = game.start.clone();
        let mut fens = vec![fen(&pos)];
        let mut positions = vec![pos.clone()];
        let mut played = Vec::with_capacity(game.moves.len());
        for &m in &game.moves {
            let color = pos.turn();
            let uci = m.to_uci(CastlingMode::Standard).to_string();
            let san = SanPlus::from_move_and_play_unchecked(&mut pos, m).to_string();
            played.push(Played { color, san, uci });
            fens.push(fen(&pos));
            positions.push(pos.clone());
        }
        Prepared { positions, fens, played }
    }

    /// Number of positions, i.e. moves + 1.
    pub fn len(&self) -> usize {
        self.positions.len()
    }
}

/// Winning chances in percent for a centipawn score (Lichess' logistic model).
fn win_percent(cp: i32) -> f64 {
    50.0 + 50.0 * (2.0 / (1.0 + (-0.003_682_08 * f64::from(cp)).exp()) - 1.0)
}

fn sign(color: Color) -> i32 {
    if color.is_white() { 1 } else { -1 }
}

/// Evaluation of a finished game, which the engine cannot search.
fn terminal_eval(pos: &Chess) -> Option<Eval> {
    if !pos.is_game_over() {
        return None;
    }
    let (cp, mate) = if pos.is_checkmate() {
        (-sign(pos.turn()) * MATE_CP, Some(0))
    } else {
        (0, None)
    };
    Some(Eval {
        cp,
        mate,
        win: win_percent(cp),
        best_uci: None,
        best_san: None,
        pv: Vec::new(),
    })
}

async fn evaluate_position(engine: &mut Engine, pos: &Chess, fen: &str, depth: u8) -> Result<Eval> {
    let turn = sign(pos.turn());
    let result = engine.evaluate(fen, depth).await?;
    let (cp, mate) = match result.score {
        Score::Cp(cp) => (turn * cp.clamp(-MATE_CP + 1000, MATE_CP - 1000), None),
        Score::Mate(n) => (
            turn * n.signum() * (MATE_CP - n.abs().min(999)),
            Some(turn * n),
        ),
    };

    // Translate the engine line to SAN, stopping at anything unparsable.
    let mut line_pos = pos.clone();
    let mut pv = Vec::new();
    for uci in result.pv.iter().take(PV_PLIES) {
        let Some(m) = UciMove::from_ascii(uci.as_bytes())
            .ok()
            .and_then(|u| u.to_move(&line_pos).ok())
        else {
            break;
        };
        pv.push(SanPlus::from_move_and_play_unchecked(&mut line_pos, m).to_string());
    }

    Ok(Eval {
        cp,
        mate,
        win: win_percent(cp),
        best_uci: result.pv.first().cloned(),
        best_san: pv.first().cloned(),
        pv,
    })
}

/// Evaluates every position with a pool of `workers` engine processes.
/// `on_eval` is called with the position index as each result arrives, in no fixed order.
pub async fn evaluate_all(
    config: &EngineConfig,
    prepared: Arc<Prepared>,
    depth: u8,
    workers: usize,
    on_eval: Arc<dyn Fn(usize, Eval) + Send + Sync>,
) -> Result<()> {
    // Workers claim positions in game order so results fill in from the first move.
    let next = Arc::new(AtomicUsize::new(0));
    let mut pool = JoinSet::new();
    for _ in 0..workers.clamp(1, prepared.len()) {
        let (config, prepared, next, on_eval) =
            (config.clone(), prepared.clone(), next.clone(), on_eval.clone());
        pool.spawn(async move {
            let mut engine = Engine::spawn(&config).await?;
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(pos) = prepared.positions.get(i) else {
                    break;
                };
                let eval = match terminal_eval(pos) {
                    Some(eval) => eval,
                    None => evaluate_position(&mut engine, pos, &prepared.fens[i], depth).await?,
                };
                on_eval(i, eval);
            }
            engine.quit().await;
            anyhow::Ok(())
        });
    }
    // Returning early drops the pool, which aborts the remaining workers and their engines.
    while let Some(joined) = pool.join_next().await {
        joined??;
    }
    Ok(())
}

/// Reviews every move whose surrounding positions are evaluated; the rest stay pending.
pub fn review(prepared: &Prepared, evals: &[Option<Eval>], depth: u8) -> GameAnalysis {
    let mut moves: Vec<MoveReview> = Vec::with_capacity(prepared.played.len());
    for (i, played) in prepared.played.iter().enumerate() {
        let s = sign(played.color);
        let mut review = MoveReview {
            ply: i + 1,
            color: if s > 0 { "white" } else { "black" },
            san: played.san.clone(),
            uci: played.uci.clone(),
            classification: None,
            missed: None,
            win_loss: 0.0,
            cp_loss: 0,
            comment: String::new(),
        };
        let (Some(before), Some(after)) = (&evals[i], &evals[i + 1]) else {
            moves.push(review);
            continue;
        };
        let is_best = before.best_uci.as_deref() == Some(played.uci.as_str());

        // Everything below is from the mover's point of view.
        let win_before = if s > 0 { before.win } else { 100.0 - before.win };
        let win_after = if s > 0 { after.win } else { 100.0 - after.win };
        let win_loss = if is_best { 0.0 } else { (win_before - win_after).max(0.0) };
        let cp_loss = if is_best {
            0
        } else {
            let clamp = |cp: i32| (s * cp).clamp(-1000, 1000);
            (clamp(before.cp) - clamp(after.cp)).max(0)
        };

        let classification = match win_loss {
            _ if is_best => Classification::Best,
            l if l < 2.0 => Classification::Excellent,
            l if l < 5.0 => Classification::Good,
            l if l < 10.0 => Classification::Inaccuracy,
            l if l < 20.0 => Classification::Mistake,
            _ => Classification::Blunder,
        };

        let had_mate = before.mate.map(|m| s * m).filter(|&m| m > 0);
        let still_mating = after.mate.is_some() && s * after.cp > 0;
        let gets_mated = after.mate.filter(|_| s * after.cp < 0).map(i32::abs);
        let was_getting_mated = before.mate.is_some() && s * before.cp < 0;
        let opponent_erred = moves.last().is_some_and(|prev| prev.win_loss >= 10.0);

        let missed = if is_best {
            None
        } else if had_mate.is_some() && !still_mating {
            Some(Missed::Mate)
        } else if opponent_erred && win_loss >= 10.0 {
            Some(Missed::Tactic)
        } else {
            None
        };

        let best = before.best_san.as_deref().unwrap_or("the engine move");
        review.comment = match (missed, classification) {
            (Some(Missed::Mate), _) => format!(
                "Missed a forced mate in {} starting with {best}.",
                had_mate.unwrap_or_default()
            ),
            (_, Classification::Best) => "Best move.".to_owned(),
            (Some(Missed::Tactic), _) => format!(
                "Missed tactic: the opponent's last move was an error and {best} would have punished it."
            ),
            (_, Classification::Blunder | Classification::Mistake)
                if gets_mated.is_some() && !was_getting_mated =>
            {
                match gets_mated {
                    Some(0) | None => format!("Walks into checkmate. {best} was necessary."),
                    Some(n) => format!("Allows a forced mate in {n}. {best} was necessary."),
                }
            }
            (_, Classification::Blunder) => format!("Blunder. {best} was the move."),
            (_, Classification::Mistake) => format!("Mistake. {best} was much stronger."),
            (_, Classification::Inaccuracy) => format!("Inaccuracy. {best} was better."),
            (_, Classification::Good) => format!("Good move, though {best} was slightly better."),
            (_, Classification::Excellent) => "Excellent move.".to_owned(),
        };
        review.classification = Some(classification);
        review.missed = missed;
        review.win_loss = win_loss;
        review.cp_loss = cp_loss;
        moves.push(review);
    }

    GameAnalysis {
        depth,
        white: summarize(&moves, "white"),
        black: summarize(&moves, "black"),
        positions: prepared
            .fens
            .iter()
            .zip(evals)
            .map(|(fen, eval)| PositionView { fen: fen.clone(), eval: eval.clone() })
            .collect(),
        moves,
    }
}

fn summarize(moves: &[MoveReview], color: &str) -> PlayerSummary {
    let mut summary = PlayerSummary::default();
    let (mut accuracy_sum, mut cp_sum, mut count) = (0.0, 0.0, 0u32);
    for m in moves.iter().filter(|m| m.color == color) {
        let Some(classification) = m.classification else {
            continue;
        };
        count += 1;
        cp_sum += f64::from(m.cp_loss);
        // Lichess' per-move accuracy curve.
        accuracy_sum += (103.1668 * (-0.04354 * m.win_loss).exp() - 3.1669).clamp(0.0, 100.0);
        match classification {
            Classification::Best => summary.best += 1,
            Classification::Excellent => summary.excellent += 1,
            Classification::Good => summary.good += 1,
            Classification::Inaccuracy => summary.inaccuracy += 1,
            Classification::Mistake => summary.mistake += 1,
            Classification::Blunder => summary.blunder += 1,
        }
        if m.missed.is_some() {
            summary.missed += 1;
        }
    }
    if count > 0 {
        summary.accuracy = accuracy_sum / f64::from(count);
        summary.acpl = cp_sum / f64::from(count);
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chesscom_style_pgn() {
        let pgn = r#"[Event "Live Chess"]
[White "a"]
[Black "b"]

1. e4 {[%clk 0:02:59.9]} 1... e5 {[%clk 0:02:58]} 2. Qh5 $2 (2. Nf3 Nc6) 2... Nc6 3. Bc4 Nf6?? 4. Qxf7# 1-0
"#;
        let game = parse_pgn(pgn).unwrap();
        assert_eq!(game.moves.len(), 7);
        let mut pos = game.start.clone();
        for &m in &game.moves {
            pos.play_unchecked(m);
        }
        assert!(pos.is_checkmate());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_pgn("1. e4 e5 2. Ke5").is_err());
        assert!(parse_pgn("[Event \"x\"]").is_err());
    }

    #[test]
    fn win_percent_is_symmetric() {
        assert!((win_percent(0) - 50.0).abs() < 1e-9);
        assert!((win_percent(300) + win_percent(-300) - 100.0).abs() < 1e-9);
        assert!(win_percent(MATE_CP) > 99.9);
    }
}
