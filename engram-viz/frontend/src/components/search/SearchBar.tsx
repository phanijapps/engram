//! Top search bar with debounced BM25 search and a results dropdown.

import { useEffect, useRef, useState } from "react";
import { Loader2, Search, X } from "lucide-react";
import { api } from "../../lib/api";
import { useGraphStore } from "../../store/graphStore";
import type { SearchResult } from "../../lib/types";

export function SearchBar() {
  const focusNode = useGraphStore((s) => s.focusNode);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [warmup, setWarmup] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    if (query.trim().length === 0) {
      setResults([]);
      setOpen(false);
      return;
    }
    debounceRef.current = setTimeout(() => {
      setLoading(true);
      setOpen(true);
      const started = Date.now();
      api
        .search(query, 12)
        .then((data) => {
          setResults(data.results);
          // The first search warms the BM25 index (one-time, ~minutes).
          if (Date.now() - started > 5000) setWarmup(true);
        })
        .catch(() => setResults([]))
        .finally(() => setLoading(false));
    }, 300);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query]);

  // Close dropdown on outside click.
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (boxRef.current && !boxRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const pick = (id: string) => {
    focusNode(id);
    setOpen(false);
    setQuery("");
  };

  return (
    <div ref={boxRef} className="relative w-80">
      <div className="flex items-center gap-2 rounded-md border border-base-600 bg-base-850 px-3 py-1.5 focus-within:border-accent">
        {loading ? (
          <Loader2 size={14} className="animate-spin text-accent" />
        ) : (
          <Search size={14} className="text-ink-faint" />
        )}
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onFocus={() => results.length > 0 && setOpen(true)}
          placeholder="Search symbols…"
          className="w-full bg-transparent text-xs text-ink placeholder:text-ink-faint focus:outline-none"
        />
        {query && (
          <button
            onClick={() => {
              setQuery("");
              setResults([]);
            }}
            className="text-ink-faint hover:text-ink"
          >
            <X size={13} />
          </button>
        )}
      </div>

      {open && (query.trim().length > 0) && (
        <div className="absolute left-0 right-0 top-full z-30 mt-1 max-h-80 overflow-y-auto rounded-md border border-base-700 bg-base-850 shadow-xl">
          {warmup && loading && (
            <div className="border-b border-base-700 px-3 py-2 text-[10px] text-accent-amber">
              Building search index on first query — this can take a minute or
              two. Subsequent searches are instant.
            </div>
          )}
          {loading && results.length === 0 && !warmup && (
            <div className="px-3 py-3 text-center text-[11px] text-ink-faint">
              Searching…
            </div>
          )}
          {!loading && results.length === 0 && (
            <div className="px-3 py-3 text-center text-[11px] text-ink-faint">
              No matches.
            </div>
          )}
          {results.map((r) => (
            <button
              key={r.id}
              onClick={() => pick(r.id)}
              className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-base-750"
            >
              <span className="font-mono text-[10px] text-accent-cyan">
                {r.kind.slice(0, 4)}
              </span>
              <span className="truncate text-xs text-ink">{r.name}</span>
              {r.file && (
                <span className="ml-auto shrink-0 truncate text-[10px] text-ink-faint">
                  {r.file}
                </span>
              )}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
