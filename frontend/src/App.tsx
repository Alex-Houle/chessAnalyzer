import { useEffect, useState } from "react";
import { fetchArchives, fetchGames } from "./api";
import type { Archive, Game } from "./api";
import { AnalysisView } from "./AnalysisView";

const DRAW_RESULTS = new Set(["agreed", "repetition", "stalemate", "insufficient", "50move", "timevsinsufficient"]);
const PAGE_SIZE = 50;
const USERNAME_KEY = "chess-review-username";

const monthName = ({ year, month }: Archive) =>
  new Date(year, month - 1).toLocaleString(undefined, { month: "long", year: "numeric" });

function outcome(game: Game, username: string) {
  const isBlack = game.black.username.toLowerCase() === username.toLowerCase();
  const me = isBlack ? game.black : game.white;
  const opponent = isBlack ? game.white : game.black;
  const result = me.result === "win" ? "Win" : DRAW_RESULTS.has(me.result) ? "Draw" : "Loss";
  // The losing side's result says how the game ended ("resigned", "checkmated", "timeout"…).
  const how = result === "Win" ? opponent.result : me.result;
  return { me, opponent, color: isBlack ? "Black" : "White", result, how };
}

const RESULT_STYLE: Record<string, string> = {
  Win: "bg-emerald-500/15 text-emerald-300",
  Draw: "bg-stone-500/20 text-stone-300",
  Loss: "bg-red-500/15 text-red-300",
};

export default function App() {
  const [input, setInput] = useState(() => localStorage.getItem(USERNAME_KEY) ?? "");
  const [username, setUsername] = useState("");
  const [archives, setArchives] = useState<Archive[]>([]);
  const [archive, setArchive] = useState<Archive | null>(null);
  const [games, setGames] = useState<Game[]>([]);
  const [timeClass, setTimeClass] = useState("all");
  const [shown, setShown] = useState(PAGE_SIZE);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Game | null>(null);

  const loadPlayer = async (event: React.FormEvent) => {
    event.preventDefault();
    const name = input.trim();
    if (!name) return;
    setLoading(true);
    setError(null);
    setSelected(null);
    try {
      const list = await fetchArchives(name);
      localStorage.setItem(USERNAME_KEY, name);
      setUsername(name);
      setArchives(list);
      setGames([]);
      setArchive(list[0] ?? null);
      if (list.length === 0) setError(`${name} has no games on Chess.com.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!username || !archive) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    fetchGames(username, archive)
      .then((list) => {
        if (cancelled) return;
        setGames(list);
        setShown(PAGE_SIZE);
      })
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [username, archive]);

  const timeClasses = [...new Set(games.map((g) => g.time_class))];
  const filtered = timeClass === "all" ? games : games.filter((g) => g.time_class === timeClass);

  return (
    <div className="mx-auto min-h-screen max-w-6xl px-4 py-6">
      <header className="mb-6 flex flex-wrap items-center gap-x-6 gap-y-3 border-b border-stone-800 pb-4">
        <h1 className="flex items-center gap-2 text-xl font-bold tracking-tight">
          <img src="/pieces/wN.svg" alt="" className="size-8" />
          Game Review
        </h1>
        <form onSubmit={loadPlayer} className="flex flex-1 gap-2 sm:max-w-md">
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Chess.com username"
            spellCheck={false}
            className="min-w-0 flex-1 rounded-lg bg-stone-900 px-3 py-2 outline-none ring-1 ring-stone-700 placeholder:text-stone-500 focus:ring-emerald-500"
          />
          <button
            disabled={loading || !input.trim()}
            className="rounded-lg bg-emerald-600 px-4 py-2 font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
          >
            Load games
          </button>
        </form>
      </header>

      {error && <p className="mb-4 rounded-lg bg-red-950 p-3 text-red-200">{error}</p>}

      {selected ? (
        <AnalysisView key={selected.id} game={selected} username={username} onBack={() => setSelected(null)} />
      ) : username && archive ? (
        <>
          <div className="mb-4 flex flex-wrap items-center gap-3 text-sm">
            <select
              value={`${archive.year}-${archive.month}`}
              onChange={(e) => {
                const [year, month] = e.target.value.split("-").map(Number);
                setArchive({ year, month });
              }}
              className="rounded-lg bg-stone-800 px-3 py-2"
            >
              {archives.map((a) => (
                <option key={`${a.year}-${a.month}`} value={`${a.year}-${a.month}`}>
                  {monthName(a)}
                </option>
              ))}
            </select>
            <select
              value={timeClass}
              onChange={(e) => setTimeClass(e.target.value)}
              className="rounded-lg bg-stone-800 px-3 py-2 capitalize"
            >
              <option value="all">All time controls</option>
              {timeClasses.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
            <span className="text-stone-400">
              {loading ? "Loading…" : `${filtered.length} game${filtered.length === 1 ? "" : "s"}`}
            </span>
          </div>

          <ul className="divide-y divide-stone-800 overflow-hidden rounded-xl bg-stone-900 ring-1 ring-stone-800">
            {filtered.slice(0, shown).map((game) => {
              const o = outcome(game, username);
              return (
                <li key={game.id}>
                  <button
                    onClick={() => setSelected(game)}
                    className="flex w-full flex-wrap items-center gap-x-4 gap-y-1 px-4 py-3 text-left hover:bg-stone-800"
                  >
                    <span className={`w-14 rounded px-2 py-0.5 text-center text-xs font-semibold ${RESULT_STYLE[o.result]}`}>
                      {o.result}
                    </span>
                    <span className="min-w-40 flex-1">
                      <span className="font-medium">vs {o.opponent.username}</span>
                      <span className="ml-2 text-sm text-stone-400">({o.opponent.rating})</span>
                      <span className="block truncate text-xs text-stone-400">
                        {o.color} · {o.how}
                        {game.opening && ` · ${game.opening}`}
                      </span>
                    </span>
                    <span className="text-sm capitalize text-stone-400">
                      {game.time_class} · {game.rated ? "rated" : "casual"}
                    </span>
                    <span className="w-28 text-right text-sm tabular-nums text-stone-400">
                      {new Date(game.end_time * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}
                    </span>
                    <span className="text-sm font-medium text-emerald-400">Review →</span>
                  </button>
                </li>
              );
            })}
          </ul>

          {filtered.length > shown && (
            <button
              onClick={() => setShown((n) => n + PAGE_SIZE)}
              className="mx-auto mt-4 block rounded-lg bg-stone-800 px-4 py-2 text-sm hover:bg-stone-700"
            >
              Show more
            </button>
          )}
        </>
      ) : (
        !error && (
          <div className="mx-auto mt-24 max-w-md text-center">
            <img src="/pieces/wN.svg" alt="" className="mx-auto size-24" />
            <h2 className="mt-4 text-2xl font-bold">Review your games</h2>
            <p className="mt-2 text-stone-400">
              Enter a Chess.com username to load their games, then pick one to have Stockfish find
              the mistakes, blunders and missed tactics.
            </p>
          </div>
        )
      )}
    </div>
  );
}
