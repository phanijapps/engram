//! Top search bar with debounced BM25 search and a results dropdown.
//! Polls /api/search/ready on mount and shows an index-warming indicator
//! until the backend signals readiness.

import { useEffect, useRef, useState } from "react";
import { Loader2, Search, Thermometer, X } from "lucide-react";
import { api } from "../../lib/api";
import { useGraphStore } from "../../store/graphStore";
import type { SearchResult } from "../../lib/types";

export function SearchBar() {
  const focusNode = useGraphStore((s) => s.focusNode);
  const searchReady = useGraphStore((s) => s.searchReady);
  const setSearchReady = useGraphStore((s) => s.setSearchReady);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);

  // Poll search readiness until the index is warm.
  useEffect(() => {
    if (searchReady) return;
    let cancelled = false;
    const poll = async () => {
      for (let i = 0; i < 300; i++) {
        if (cancelled) return;
        await new Promise((r) => setTimeout(r, 2000));
        if (cancelled) return;
        try {
          const status = await api.searchReady();
          if (status.ready) {
            setSearchReady(true);
            return;
          }
        } catch {
          // Backend might not be up yet — keep polling.
        }
      }
    };
    poll();
    return () => {
      cancelled = true;
    };
  }, [searchReady, setSearchReady]);

  // Debounced search.
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
      api
        .search(query, 12)
        .then((data) => {
          setResults(data.results);
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
          placeholder={
            searchReady ? "Search symbols…" : "Warming search index…"
          }
          className="w-full bg-transparent text-xs text-ink placeholder:text-ink-faint focus:outline-none"
        />
        {!searchReady && (
          <span title="Building BM25 search index in the background">
            <Thermometer
              size={13}
              className="shrink-0 animate-pulse text-accent-amber"
            />
          </span>
        )}
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

      {open && query.trim().length > 0 && (
        <div className="absolute left-0 right-0 top-full z-30 mt-1 max-h-80 overflow-y-auto rounded-md border border-base-700 bg-base-850 shadow-xl">
          {!searchReady && loading && (
            <div className="border-b border-base-700 px-3 py-2 text-[10px] text-accent-amber">
              Building search index in the background — first queries may be
              slow. Subsequent searches are instant.
            </div>
          )}
          {loading && results.length === 0 && (
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
