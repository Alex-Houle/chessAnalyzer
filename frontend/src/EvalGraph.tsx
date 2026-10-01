import { useEffect, useRef, useState } from "react";
import type { GameAnalysis } from "./api";
import { CLASSIFICATIONS, MISSED, formatEval, isError, moveLabel, moveMeta } from "./review";

const HEIGHT = 120;

interface EvalGraphProps {
  analysis: GameAnalysis;
  ply: number;
  /** Whose errors to mark on the graph. */
  player: "white" | "black";
  onSelect: (ply: number) => void;
}

/** White's winning chances over the game, with errors marked. Click or drag to navigate. */
export function EvalGraph({ analysis, ply, player, onSelect }: EvalGraphProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [hover, setHover] = useState<number | null>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const { positions, moves } = analysis;
  const last = positions.length - 1;
  const x = (i: number) => (last === 0 ? 0 : (i / last) * width);
  const y = (win: number) => HEIGHT - (win / 100) * HEIGHT;

  // While analysis is running only some positions have an evaluation; the line grows as they arrive.
  const points = positions.flatMap((p, i) => (p.eval ? [{ x: x(i), y: y(p.eval.win) }] : []));
  const line = points.map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" ");
  const area = points.length > 0 ? `${line} L${points[points.length - 1].x.toFixed(1)},${HEIGHT} L${points[0].x.toFixed(1)},${HEIGHT} Z` : "";
  const marked = moves.filter((m) => isError(m) && m.color === player);

  const plyAt = (event: React.PointerEvent) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return Math.round(Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width)) * last);
  };

  const hovered = hover !== null ? positions[hover].eval : null;
  const hoveredMove = hover !== null && hover > 0 ? moves[hover - 1] : null;

  return (
    <div>
      <div ref={ref} className="relative">
        <svg
          width="100%"
          height={HEIGHT}
          className="block cursor-pointer touch-none rounded-xl bg-stone-800"
          role="img"
          aria-label="White's winning chances over the course of the game"
          onPointerMove={(e) => {
            const p = plyAt(e);
            setHover(p);
            if (e.buttons === 1) onSelect(p);
          }}
          onPointerDown={(e) => onSelect(plyAt(e))}
          onPointerLeave={() => setHover(null)}
        >
          {width > 0 && (
            <>
              <path d={area} fill="#e7e5e4" />
              <path d={line} fill="none" stroke="#fafaf9" strokeWidth="2" strokeLinejoin="round" />
              <line x1="0" x2={width} y1={HEIGHT / 2} y2={HEIGHT / 2} stroke="#78716c" strokeDasharray="3 4" />
              <line x1={x(ply)} x2={x(ply)} y1="0" y2={HEIGHT} stroke="#34d399" strokeWidth="2" />
              {hover !== null && hover !== ply && (
                <line x1={x(hover)} x2={x(hover)} y1="0" y2={HEIGHT} stroke="#a8a29e" />
              )}
              {marked.map((m) => (
                <circle
                  key={m.ply}
                  cx={x(m.ply)}
                  cy={Math.min(HEIGHT - 6, Math.max(6, y(positions[m.ply].eval?.win ?? 50)))}
                  r="4"
                  fill={moveMeta(m).hex}
                  stroke="#211f1c"
                  strokeWidth="2"
                />
              ))}
            </>
          )}
        </svg>
        {hover !== null && (
          <div
            className="pointer-events-none absolute top-1 z-10 -transtone-x-1/2 whitespace-nowrap rounded bg-stone-950 px-2 py-1 text-xs text-stone-200 shadow ring-1 ring-stone-700"
            style={{ left: Math.min(Math.max(x(hover), 70), Math.max(width - 70, 70)) }}
          >
            {hoveredMove ? moveLabel(hoveredMove) : "Start"}
            <span className="mx-1.5 text-stone-500">·</span>
            <span className="font-semibold tabular-nums">{formatEval(hovered)}</span>
            {hoveredMove && (
              <>
                <span className="mx-1.5 text-stone-500">·</span>
                {moveMeta(hoveredMove).label}
              </>
            )}
          </div>
        )}
      </div>
      <div className="mt-1.5 flex flex-wrap gap-x-4 gap-y-1 text-xs text-stone-400">
        <span>White's winning chances by move</span>
        {[CLASSIFICATIONS.mistake, CLASSIFICATIONS.blunder, MISSED].map((meta) => (
          <span key={meta.label} className="flex items-center gap-1.5">
            <span className={`size-2 rounded-full ${meta.bg}`} />
            {meta.label}
          </span>
        ))}
      </div>
    </div>
  );
}
