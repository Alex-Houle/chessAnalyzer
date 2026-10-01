import type { Eval } from "./api";
import { formatEval } from "./review";

export type Orientation = "white" | "black";

/** Image for a FEN piece letter: "N" → white knight, "n" → black knight. */
const pieceImage = (piece: string) =>
  `/pieces/${piece === piece.toUpperCase() ? "w" : "b"}${piece.toUpperCase()}.svg`;

const FILES = "abcdefgh";

/** Board from a FEN as [rank 8 → 1][file a → h], with `null` for empty squares. */
function parseBoard(fen: string): (string | null)[][] {
  return fen
    .split(" ")[0]
    .split("/")
    .map((row) =>
      [...row].flatMap((c) => (/\d/.test(c) ? Array<null>(Number(c)).fill(null) : [c])),
    );
}

/** Column/row of a square such as "e4" on the rendered grid. */
function gridPosition(square: string, orientation: Orientation) {
  const file = FILES.indexOf(square[0]);
  const rank = Number(square[1]) - 1;
  return orientation === "white" ? { x: file, y: 7 - rank } : { x: 7 - file, y: rank };
}

interface BoardProps {
  fen: string;
  orientation: Orientation;
  /** UCI move that led to this position; its squares are highlighted. */
  lastMove?: string;
  /** UCI move drawn as an arrow, e.g. the engine's preferred move. */
  arrow?: string | null;
  /** Annotation pinned to the destination square of `lastMove`. */
  badge?: { symbol: string; bg: string };
}

export function Board({ fen, orientation, lastMove, arrow, badge }: BoardProps) {
  const board = parseBoard(fen);
  const from = lastMove?.slice(0, 2);
  const to = lastMove?.slice(2, 4);

  const squares = [];
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 8; x++) {
      const rankIndex = orientation === "white" ? y : 7 - y;
      const fileIndex = orientation === "white" ? x : 7 - x;
      const name = `${FILES[fileIndex]}${8 - rankIndex}`;
      const piece = board[rankIndex]?.[fileIndex] ?? null;
      const dark = (rankIndex + fileIndex) % 2 === 1;
      const highlighted = name === from || name === to;
      squares.push(
        <div
          key={name}
          className={`relative flex items-center justify-center ${dark ? "bg-[#b58863]" : "bg-[#f0d9b5]"}`}
        >
          {highlighted && <div className="absolute inset-0 bg-[#9bc700]/40" />}
          {x === 0 && (
            <span className={`absolute left-[4%] top-[2%] text-[2.4cqw] font-semibold ${dark ? "text-[#f0d9b5]" : "text-[#b58863]"}`}>
              {8 - rankIndex}
            </span>
          )}
          {y === 7 && (
            <span className={`absolute bottom-[2%] right-[5%] text-[2.4cqw] font-semibold ${dark ? "text-[#f0d9b5]" : "text-[#b58863]"}`}>
              {FILES[fileIndex]}
            </span>
          )}
          {piece && <img src={pieceImage(piece)} alt={piece} draggable={false} className="relative size-full select-none" />}
          {badge && name === to && (
            <span
              className={`absolute right-[3%] top-[3%] z-10 flex size-[4.6cqw] items-center justify-center rounded-full text-[2.6cqw] font-bold text-white shadow ring-2 ring-white/70 ${badge.bg}`}
            >
              {badge.symbol}
            </span>
          )}
        </div>,
      );
    }
  }

  const arrowFrom = arrow ? gridPosition(arrow.slice(0, 2), orientation) : null;
  const arrowTo = arrow ? gridPosition(arrow.slice(2, 4), orientation) : null;

  return (
    <div className="@container relative aspect-square w-full overflow-hidden rounded-lg shadow-xl shadow-black/40">
      <div className="grid size-full grid-cols-8 grid-rows-8">{squares}</div>
      {arrowFrom && arrowTo && (
        <svg viewBox="0 0 8 8" className="pointer-events-none absolute inset-0 size-full">
          <defs>
            <marker id="arrowhead" markerWidth="3" markerHeight="3" refX="1.6" refY="1.5" orient="auto">
              <path d="M0,0 L3,1.5 L0,3 Z" fill="#10b981" />
            </marker>
          </defs>
          <line
            x1={arrowFrom.x + 0.5}
            y1={arrowFrom.y + 0.5}
            x2={arrowTo.x + 0.5}
            y2={arrowTo.y + 0.5}
            stroke="#10b981"
            strokeWidth="0.16"
            strokeLinecap="round"
            opacity="0.85"
            markerEnd="url(#arrowhead)"
          />
        </svg>
      )}
    </div>
  );
}

/** Vertical bar showing White's winning chances for the current position. */
export function EvalBar({ position, orientation }: { position: Eval | null; orientation: Orientation }) {
  const whiteAhead = (position?.cp ?? 0) >= 0;
  return (
    <div
      className={`relative flex w-6 shrink-0 overflow-hidden rounded-lg bg-stone-700 ${
        orientation === "white" ? "flex-col-reverse" : "flex-col"
      }`}
      title={`Evaluation ${formatEval(position)}`}
    >
      <div className="bg-stone-100 transition-[height] duration-300" style={{ height: `${position?.win ?? 50}%` }} />
      <span
        className={`absolute inset-x-0 text-center text-[10px] font-semibold tabular-nums ${
          whiteAhead ? "text-stone-900" : "text-stone-100"
        } ${whiteAhead === (orientation === "white") ? "bottom-1" : "top-1"}`}
      >
        {formatEval(position).replace(/^[+-]/, "")}
      </span>
    </div>
  );
}
