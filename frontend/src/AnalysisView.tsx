import { useEffect, useRef, useState } from "react";
import { cancelAnalysis, fetchJob, startAnalysis } from "./api";
import type { Game, GameAnalysis, Job, MoveReview, PlayerSummary } from "./api";
import { Board, EvalBar } from "./Board";
import type { Orientation } from "./Board";
import { EvalGraph } from "./EvalGraph";
import { CLASSIFICATIONS, CLASS_ORDER, MISSED, formatEval, isError, moveLabel, moveMeta } from "./review";

const DEPTHS = [10, 14, 18, 22];
const POLL_MS = 300;
const WAITING: Job = { status: "queued", done: 0, total: 0, result: null };

interface AnalysisViewProps {
  game: Game;
  username: string;
  onBack: () => void;
}

export function AnalysisView({ game, username, onBack }: AnalysisViewProps) {
  const [depth, setDepth] = useState(14);
  const [job, setJob] = useState<Job>(WAITING);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    // Set while the server may still be working, so leaving the game frees its engines.
    let running: string | null = null;
    const cancel = () => running && cancelAnalysis(running);
    window.addEventListener("pagehide", cancel);
    setJob(WAITING);

    const fail = (error: unknown) => {
      if (!cancelled) setJob({ status: "error", message: error instanceof Error ? error.message : String(error) });
    };
    const poll = async (id: string) => {
      try {
        const next = await fetchJob(id);
        if (cancelled) return;
        // The job was cancelled from elsewhere (e.g. a late cancel for the same game), so begin again.
        if (!next) return start();
        setJob(next);
        if (next.status === "queued" || next.status === "running") {
          timer = window.setTimeout(() => poll(id), POLL_MS);
        } else {
          running = null;
        }
      } catch (error) {
        fail(error);
      }
    };
    const start = () =>
      startAnalysis(game.pgn, depth).then(({ id }) => {
        // Not cancelled here: an identical request made since then would share this job.
        if (cancelled) return;
        running = id;
        poll(id);
      }, fail);
    start();

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
      window.removeEventListener("pagehide", cancel);
      cancel();
    };
  }, [game.pgn, depth]);

  return (
    <div>
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <button onClick={onBack} className="rounded-lg bg-stone-800 px-3 py-1.5 text-sm font-medium hover:bg-stone-700">
          ← Games
        </button>
        <h2 className="text-lg font-semibold">
          {game.white.username} <span className="text-stone-500">vs</span> {game.black.username}
        </h2>
        {game.opening && <span className="text-sm text-stone-400">{game.opening}</span>}
        <label className="ml-auto flex items-center gap-2 text-sm text-stone-400">
          Depth
          <select
            value={depth}
            onChange={(e) => setDepth(Number(e.target.value))}
            className="rounded-lg bg-stone-800 px-2 py-1.5 text-stone-200"
          >
            {DEPTHS.map((d) => (
              <option key={d} value={d}>
                {d}
              </option>
            ))}
          </select>
        </label>
        <a href={game.url} target="_blank" rel="noreferrer" className="text-sm text-emerald-400 hover:underline">
          Open on Chess.com
        </a>
      </div>

      {job.status === "error" ? (
        <p className="rounded-lg bg-red-950 p-4 text-red-200">Analysis failed: {job.message}</p>
      ) : (
        <>
          {job.status !== "done" && <Progress job={job} depth={depth} />}
          {job.result && (
            <Review
              key={`${game.id}-${depth}`}
              game={game}
              analysis={job.result}
              done={job.status === "done"}
              orientation={game.black.username.toLowerCase() === username.toLowerCase() ? "black" : "white"}
            />
          )}
        </>
      )}
    </div>
  );
}

function Progress({ job, depth }: { job: Job; depth: number }) {
  const fraction = job.status === "running" && job.total > 0 ? job.done / job.total : 0;
  return (
    <div className="mb-4">
      <p className="mb-1.5 text-sm text-stone-400">
        {job.status === "running"
          ? `Stockfish is analyzing at depth ${depth}: ${job.done} of ${job.total} positions`
          : "Waiting for a free engine…"}
      </p>
      <div className="h-1.5 overflow-hidden rounded-full bg-stone-800">
        <div className="h-full rounded-full bg-emerald-500 transition-[width]" style={{ width: `${fraction * 100}%` }} />
      </div>
    </div>
  );
}

interface ReviewProps {
  game: Game;
  analysis: GameAnalysis;
  /** False while results are still streaming in. */
  done: boolean;
  orientation: Orientation;
}

function Review({ game, analysis, done, orientation: initialOrientation }: ReviewProps) {
  const [ply, setPly] = useState(0);
  const [orientation, setOrientation] = useState(initialOrientation);
  const last = analysis.moves.length;
  const go = (next: number) => setPly(Math.min(last, Math.max(0, next)));

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
      const step = { ArrowLeft: -1, ArrowRight: 1, Home: -last, End: last }[event.key];
      if (step === undefined) return;
      event.preventDefault();
      setPly((p) => Math.min(last, Math.max(0, p + step)));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [last]);

  const position = analysis.positions[ply];
  const move = ply > 0 ? analysis.moves[ply - 1] : null;
  // The position the move was played from holds the engine's preferred alternative.
  const previous = ply > 0 ? analysis.positions[ply - 1] : null;
  const showBetter = move !== null && move.classification !== null && move.classification !== "best";

  // Only the looked-up player's errors are called out; `initialOrientation` is their colour.
  const errors = analysis.moves.filter((m) => isError(m) && m.color === initialOrientation);
  const top = orientation === "white" ? game.black : game.white;
  const bottom = orientation === "white" ? game.white : game.black;

  return (
    <div className="grid gap-6 lg:grid-cols-[minmax(0,34rem)_minmax(0,1fr)]">
      <div>
        <PlayerLabel side={top} color={orientation === "white" ? "black" : "white"} />
        <div className="my-2 flex gap-2">
          <EvalBar position={position.eval} orientation={orientation} />
          <Board
            fen={position.fen}
            orientation={orientation}
            lastMove={move?.uci}
            arrow={showBetter ? previous?.eval?.best_uci : null}
            badge={move?.classification ? moveMeta(move) : undefined}
          />
        </div>
        <PlayerLabel side={bottom} color={orientation} />

        <div className="mt-3 flex items-center gap-2 pl-8">
          <div className="flex flex-1 overflow-hidden rounded-lg bg-stone-800">
            {[
              { icon: "M6 5v14M19 6l-8 6 8 6V6z", title: "First move (Home)", target: 0 },
              { icon: "M16 6l-8 6 8 6V6z", title: "Previous move (←)", target: ply - 1 },
              { icon: "M8 6l8 6-8 6V6z", title: "Next move (→)", target: ply + 1 },
              { icon: "M18 5v14M5 6l8 6-8 6V6z", title: "Last move (End)", target: last },
            ].map((button) => (
              <button
                key={button.title}
                title={button.title}
                aria-label={button.title}
                disabled={button.target < 0 || button.target > last || button.target === ply}
                onClick={() => go(button.target)}
                className="flex flex-1 justify-center py-2.5 text-stone-200 hover:bg-stone-700 disabled:text-stone-600 disabled:hover:bg-transparent"
              >
                <Icon path={button.icon} />
              </button>
            ))}
          </div>
          <button
            title="Flip board"
            aria-label="Flip board"
            onClick={() => setOrientation((o) => (o === "white" ? "black" : "white"))}
            className="rounded-lg bg-stone-800 px-4 py-2.5 text-stone-200 hover:bg-stone-700"
          >
            <Icon path="M7 4v15m0 0l-3.5-3.5M7 19l3.5-3.5M17 20V5m0 0l-3.5 3.5M17 5l3.5 3.5" filled={false} />
          </button>
        </div>
      </div>

      <div className="flex min-w-0 flex-col gap-4">
        <Summary game={game} analysis={analysis} done={done} />
        <EvalGraph analysis={analysis} ply={ply} player={initialOrientation} onSelect={go} />

        <div className="min-h-24 rounded-xl bg-stone-900 p-4 ring-1 ring-stone-800">
          {move ? (
            <>
              <div className="flex items-center gap-2">
                <Badge move={move} />
                <span className="font-semibold">{moveLabel(move)}</span>
                <span className={`text-sm ${moveMeta(move).text}`}>{moveMeta(move).label}</span>
                <span className="ml-auto font-mono text-sm tabular-nums text-stone-300">{formatEval(position.eval)}</span>
              </div>
              <p className="mt-2 text-sm text-stone-300">{move.comment || "Stockfish is still working on this move."}</p>
              {showBetter && previous?.eval && previous.eval.pv.length > 0 && (
                <p className="mt-1 text-sm text-stone-400">
                  Engine line: <span className="font-mono text-emerald-300">{previous.eval.pv.join(" ")}</span>
                </p>
              )}
            </>
          ) : (
            <p className="text-sm text-stone-400">
              Step through the game with the arrow keys, or jump straight to an error below.
            </p>
          )}
        </div>

        {errors.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {errors.map((m) => (
              <button
                key={m.ply}
                onClick={() => go(m.ply)}
                title={m.comment}
                className={`flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs ring-1 ring-stone-700 hover:bg-stone-800 ${
                  m.ply === ply ? "bg-stone-800" : "bg-stone-900"
                }`}
              >
                <span className={`size-2 rounded-full ${moveMeta(m).bg}`} />
                {moveLabel(m)}
                <span className={moveMeta(m).text}>{moveMeta(m).label}</span>
              </button>
            ))}
          </div>
        )}

        <MoveList moves={analysis.moves} ply={ply} onSelect={go} />
      </div>
    </div>
  );
}

function Icon({ path, filled = true }: { path: string; filled?: boolean }) {
  return (
    <svg viewBox="0 0 24 24" className="size-5" fill={filled ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d={path} />
    </svg>
  );
}

function PlayerLabel({ side, color }: { side: Game["white"]; color: Orientation }) {
  return (
    <div className="flex items-center gap-2 pl-8 text-sm">
      <span className={`size-3 rounded-sm ring-1 ring-stone-600 ${color === "white" ? "bg-stone-100" : "bg-stone-950"}`} />
      <span className="font-semibold">{side.username}</span>
      <span className="text-stone-400">{side.rating}</span>
    </div>
  );
}

function Badge({ move }: { move: MoveReview }) {
  const meta = moveMeta(move);
  return (
    <span className={`flex size-5 shrink-0 items-center justify-center rounded-full text-[10px] font-bold text-white ${meta.bg}`}>
      {meta.symbol}
    </span>
  );
}

function Summary({ game, analysis, done }: { game: Game; analysis: GameAnalysis; done: boolean }) {
  // Averages over a partly analyzed game would be misleading, so they wait for the end.
  const final = (value: string | number) => (done ? value : "–");
  const rows: { label: string; text: string; value: (s: PlayerSummary) => number }[] = [
    ...CLASS_ORDER.map((c) => ({ ...CLASSIFICATIONS[c], value: (s: PlayerSummary) => s[c] })),
    { ...MISSED, value: (s: PlayerSummary) => s.missed },
  ];
  const players = [
    { side: game.white, summary: analysis.white, tile: "bg-stone-100 text-stone-900", muted: "text-stone-600" },
    { side: game.black, summary: analysis.black, tile: "bg-stone-800 text-stone-100", muted: "text-stone-400" },
  ];
  return (
    <div className="rounded-xl bg-stone-900 p-4 ring-1 ring-stone-800">
      <div className="grid grid-cols-2 gap-3">
        {players.map(({ side, summary, tile, muted }) => (
          <div key={side.username} className={`rounded-lg px-4 py-3 ${tile}`}>
            <div className="truncate text-sm font-medium">{side.username}</div>
            <div className="text-3xl font-bold tabular-nums">{final(summary.accuracy.toFixed(1))}</div>
            <div className={`text-xs ${muted}`}>
              Accuracy · {final(Math.round(summary.acpl))} avg. centipawn loss
            </div>
          </div>
        ))}
      </div>
      <table className="mt-3 w-full text-sm tabular-nums">
        <tbody>
          {rows.map((row) => (
            <tr key={row.label}>
              <td className="w-1/2 pr-4 text-right text-stone-200">{row.value(analysis.white)}</td>
              <td className={`whitespace-nowrap py-0.5 text-center text-xs font-medium ${row.text}`}>{row.label}</td>
              <td className="w-1/2 pl-4 text-stone-200">{row.value(analysis.black)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function MoveList({ moves, ply, onSelect }: { moves: MoveReview[]; ply: number; onSelect: (ply: number) => void }) {
  const active = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // Scroll only the list itself; scrollIntoView would also move the whole page.
    const box = list.current;
    const item = active.current;
    if (!box || !item) return;
    const top = item.getBoundingClientRect().top - box.getBoundingClientRect().top;
    if (top < 0) box.scrollTop += top;
    else if (top + item.offsetHeight > box.clientHeight) box.scrollTop += top + item.offsetHeight - box.clientHeight;
  }, [ply]);

  // A game starting from a FEN with Black to move has no White move in its first row.
  const offset = moves[0]?.color === "black" ? 1 : 0;
  const rows: (MoveReview | null)[][] = [];
  moves.forEach((m, i) => {
    const slot = i + offset;
    if (slot % 2 === 0 || rows.length === 0) rows.push([null, null]);
    rows[rows.length - 1][slot % 2] = m;
  });

  return (
    <div ref={list} className="max-h-72 overflow-y-auto rounded-xl bg-stone-900 p-2 text-sm ring-1 ring-stone-800">
      {rows.map((row, i) => (
        <div key={i} className="grid grid-cols-[2.5rem_1fr_1fr] items-center">
          <span className="text-stone-500">{i + 1}.</span>
          {row.map((m, j) =>
            m ? (
              <button
                key={j}
                ref={m.ply === ply ? active : undefined}
                onClick={() => onSelect(m.ply)}
                className={`flex items-center gap-1.5 rounded px-2 py-1 text-left hover:bg-stone-800 ${
                  m.ply === ply ? "bg-stone-700" : ""
                }`}
              >
                <span className="font-medium">{m.san}</span>
                {m.classification && (m.missed || m.classification !== "good") && (
                  <span className={`text-xs font-bold ${moveMeta(m).text}`} title={moveMeta(m).label}>
                    {moveMeta(m).symbol}
                  </span>
                )}
              </button>
            ) : (
              <span key={j} />
            ),
          )}
        </div>
      ))}
    </div>
  );
}
