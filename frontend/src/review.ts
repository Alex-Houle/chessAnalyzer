import type { Classification, Eval, MoveReview } from "./api";

interface ClassMeta {
  label: string;
  symbol: string;
  text: string;
  bg: string;
  hex: string;
}

export const CLASSIFICATIONS: Record<Classification, ClassMeta> = {
  best: { label: "Best", symbol: "★", text: "text-emerald-400", bg: "bg-emerald-500", hex: "#10b981" },
  excellent: { label: "Excellent", symbol: "!", text: "text-teal-300", bg: "bg-teal-500", hex: "#14b8a6" },
  good: { label: "Good", symbol: "✓", text: "text-stone-300", bg: "bg-stone-500", hex: "#78716c" },
  inaccuracy: { label: "Inaccuracy", symbol: "?!", text: "text-yellow-300", bg: "bg-yellow-500", hex: "#eab308" },
  mistake: { label: "Mistake", symbol: "?", text: "text-orange-400", bg: "bg-orange-500", hex: "#f97316" },
  blunder: { label: "Blunder", symbol: "??", text: "text-red-400", bg: "bg-red-500", hex: "#ef4444" },
};

export const CLASS_ORDER = Object.keys(CLASSIFICATIONS) as Classification[];

export const MISSED = { label: "Missed tactic", symbol: "✕", text: "text-fuchsia-400", bg: "bg-fuchsia-500", hex: "#d946ef" };

const PENDING = { label: "Analyzing…", symbol: "…", text: "text-stone-500", bg: "bg-stone-600", hex: "#57534e" };

/** Visual metadata for a move; a missed mate or tactic takes precedence over its class. */
export const moveMeta = (move: MoveReview) =>
  move.missed ? MISSED : move.classification ? CLASSIFICATIONS[move.classification] : PENDING;

export const isError = (move: MoveReview) =>
  move.missed !== null || move.classification === "mistake" || move.classification === "blunder";

/** "+1.4", "-0.3", "#5", "#-2", or "…" while the position is still being searched. */
export function formatEval(position: Eval | null): string {
  if (!position) return "…";
  if (position.mate !== null) {
    if (position.mate === 0) return "#";
    return position.mate > 0 ? `#${position.mate}` : `#-${-position.mate}`;
  }
  const pawns = position.cp / 100;
  return `${pawns > 0 ? "+" : ""}${pawns.toFixed(1)}`;
}

/** "12. Nf3" for White, "12… Nf6" for Black. */
export function moveLabel(move: MoveReview): string {
  const number = Math.ceil(move.ply / 2);
  return `${number}${move.color === "white" ? "." : "…"} ${move.san}`;
}
