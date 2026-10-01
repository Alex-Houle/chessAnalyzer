export interface Archive {
  year: number;
  month: number;
}

export interface Side {
  username: string;
  rating: number;
  result: string;
}

export interface Game {
  id: string;
  url: string;
  pgn: string;
  time_control: string;
  time_class: string;
  rated: boolean;
  end_time: number;
  opening: string | null;
  white: Side;
  black: Side;
}

export type Classification =
  | "best"
  | "excellent"
  | "good"
  | "inaccuracy"
  | "mistake"
  | "blunder";

export interface Eval {
  /** Centipawns from White's point of view. */
  cp: number;
  /** Moves to mate, positive when White is mating; 0 means checkmate on the board. */
  mate: number | null;
  /** White's winning chances in percent. */
  win: number;
  best_uci: string | null;
  best_san: string | null;
  pv: string[];
}

export interface Position {
  fen: string;
  /** `null` until the engine has searched this position. */
  eval: Eval | null;
}

export interface MoveReview {
  ply: number;
  color: "white" | "black";
  san: string;
  uci: string;
  /** `null` while the move is still being analyzed. */
  classification: Classification | null;
  missed: "mate" | "tactic" | null;
  win_loss: number;
  cp_loss: number;
  comment: string;
}

export interface PlayerSummary {
  accuracy: number;
  acpl: number;
  best: number;
  excellent: number;
  good: number;
  inaccuracy: number;
  mistake: number;
  blunder: number;
  missed: number;
}

export interface GameAnalysis {
  depth: number;
  /** Index 0 is the starting position; index n is the position after move n. */
  positions: Position[];
  moves: MoveReview[];
  white: PlayerSummary;
  black: PlayerSummary;
}

export type Job =
  /** Partial results arrive while the engines work: unsearched positions have no eval yet. */
  | { status: "queued" | "running"; done: number; total: number; result: GameAnalysis | null }
  | { status: "done"; result: GameAnalysis }
  | { status: "error"; message: string };

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, init);
  if (!response.ok) {
    const body = await response.json().catch(() => null);
    throw new Error(body?.error ?? `Request failed (${response.status})`);
  }
  return response.json();
}

export const fetchArchives = (username: string) =>
  request<Archive[]>(`/api/players/${encodeURIComponent(username)}/archives`);

export const fetchGames = (username: string, { year, month }: Archive) =>
  request<Game[]>(`/api/players/${encodeURIComponent(username)}/games/${year}/${month}`);

export const startAnalysis = (pgn: string, depth: number) =>
  request<{ id: string }>("/api/analysis", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ pgn, depth }),
  });

/** Resolves to `null` when the server no longer knows the job. */
export async function fetchJob(id: string): Promise<Job | null> {
  const response = await fetch(`/api/analysis/${id}`);
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`Request failed (${response.status})`);
  return response.json();
}

/** Tells the server to stop an unfinished analysis. `keepalive` lets it survive a page unload. */
export const cancelAnalysis = (id: string) =>
  void fetch(`/api/analysis/${id}`, { method: "DELETE", keepalive: true }).catch(() => {});
