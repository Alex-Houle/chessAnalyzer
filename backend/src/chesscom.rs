//! Thin client for the public Chess.com "published data" API.

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

const BASE: &str = "https://api.chess.com/pub";

#[derive(Debug)]
pub enum ChessComError {
    NotFound,
    RateLimited,
    Upstream(String),
}

impl From<reqwest::Error> for ChessComError {
    fn from(err: reqwest::Error) -> Self {
        ChessComError::Upstream(err.to_string())
    }
}

#[derive(Deserialize)]
struct RawArchives {
    archives: Vec<String>,
}

#[derive(Deserialize)]
struct RawGames {
    games: Vec<RawGame>,
}

#[derive(Deserialize)]
struct RawGame {
    url: String,
    pgn: Option<String>,
    #[serde(default)]
    time_control: String,
    #[serde(default)]
    time_class: String,
    #[serde(default)]
    rules: String,
    #[serde(default)]
    rated: bool,
    #[serde(default)]
    end_time: u64,
    eco: Option<String>,
    white: RawSide,
    black: RawSide,
}

#[derive(Deserialize)]
struct RawSide {
    username: String,
    #[serde(default)]
    rating: u32,
    #[serde(default)]
    result: String,
}

#[derive(Serialize)]
pub struct Archive {
    pub year: u16,
    pub month: u8,
}

#[derive(Serialize)]
pub struct Side {
    pub username: String,
    pub rating: u32,
    pub result: String,
}

#[derive(Serialize)]
pub struct Game {
    pub id: String,
    pub url: String,
    pub pgn: String,
    pub time_control: String,
    pub time_class: String,
    pub rated: bool,
    pub end_time: u64,
    pub opening: Option<String>,
    pub white: Side,
    pub black: Side,
}

#[derive(Clone)]
pub struct ChessCom {
    http: reqwest::Client,
}

impl ChessCom {
    pub fn new(user_agent: &str) -> reqwest::Result<Self> {
        // Chess.com rejects requests without a descriptive User-Agent.
        let http = reqwest::Client::builder()
            .user_agent(user_agent)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { http })
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, url: String) -> Result<T, ChessComError> {
        let response = self.http.get(url).send().await?;
        match response.status() {
            StatusCode::NOT_FOUND => Err(ChessComError::NotFound),
            StatusCode::TOO_MANY_REQUESTS => Err(ChessComError::RateLimited),
            status if !status.is_success() => Err(ChessComError::Upstream(format!(
                "Chess.com responded with {status}"
            ))),
            _ => Ok(response.json().await?),
        }
    }

    /// Months in which the player has games, newest first.
    pub async fn archives(&self, username: &str) -> Result<Vec<Archive>, ChessComError> {
        let raw: RawArchives = self
            .get(format!("{BASE}/player/{username}/games/archives"))
            .await?;
        let mut archives: Vec<Archive> = raw
            .archives
            .iter()
            .filter_map(|url| {
                let mut parts = url.rsplit('/');
                let month = parts.next()?.parse().ok()?;
                let year = parts.next()?.parse().ok()?;
                Some(Archive { year, month })
            })
            .collect();
        archives.reverse();
        Ok(archives)
    }

    /// Standard-chess games from one monthly archive, newest first.
    pub async fn games(
        &self,
        username: &str,
        year: u16,
        month: u8,
    ) -> Result<Vec<Game>, ChessComError> {
        let raw: RawGames = self
            .get(format!("{BASE}/player/{username}/games/{year}/{month:02}"))
            .await?;
        let mut games: Vec<Game> = raw
            .games
            .into_iter()
            .filter(|g| g.rules == "chess")
            .filter_map(|g| {
                let pgn = g.pgn?;
                Some(Game {
                    id: g.url.rsplit('/').next().unwrap_or_default().to_owned(),
                    opening: g.eco.as_deref().and_then(opening_name),
                    url: g.url,
                    pgn,
                    time_control: g.time_control,
                    time_class: g.time_class,
                    rated: g.rated,
                    end_time: g.end_time,
                    white: g.white.into(),
                    black: g.black.into(),
                })
            })
            .collect();
        games.sort_by(|a, b| b.end_time.cmp(&a.end_time));
        Ok(games)
    }
}

impl From<RawSide> for Side {
    fn from(raw: RawSide) -> Self {
        Side {
            username: raw.username,
            rating: raw.rating,
            result: raw.result,
        }
    }
}

/// Turns `https://www.chess.com/openings/Sicilian-Defense-2.Nf3` into a readable name.
fn opening_name(eco_url: &str) -> Option<String> {
    let slug = eco_url.rsplit('/').next()?;
    // Drop the trailing move list ("-2.Nf3-d6" or "...4.Be3-a6") that Chess.com appends.
    let slug = slug.split("...").next()?;
    let words: Vec<&str> = slug
        .split('-')
        .take_while(|w| !w.starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// Chess.com usernames are alphanumeric plus `_` and `-`.
pub fn valid_username(username: &str) -> bool {
    (1..=50).contains(&username.len())
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_names() {
        assert_eq!(
            opening_name("https://www.chess.com/openings/Sicilian-Defense-Open-2.Nf3-d6").as_deref(),
            Some("Sicilian Defense Open")
        );
        assert_eq!(
            opening_name("https://www.chess.com/openings/Kings-Pawn-Opening").as_deref(),
            Some("Kings Pawn Opening")
        );
        assert_eq!(
            opening_name("https://www.chess.com/openings/Modern-Defense-Standard-Line...4.Be3-a6").as_deref(),
            Some("Modern Defense Standard Line")
        );
    }

    #[test]
    fn usernames() {
        assert!(valid_username("Hikaru"));
        assert!(valid_username("a_b-c1"));
        assert!(!valid_username("../etc"));
        assert!(!valid_username(""));
    }
}
