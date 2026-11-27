import { useCallback, useEffect, useRef, useState } from "react";

import DetailDrawer from "./components/DetailDrawer";
import LaunchTable, { type TableFilters } from "./components/LaunchTable";
import TopBar from "./components/TopBar";
import { getAnalysis, getRecentAnalyses, getRecentEvents, getStatus } from "./lib/ipc";
import type { LaunchAnalysis, LaunchEvent, StatusResponse } from "./lib/types";

const REFRESH_MS = 2500;

export default function App() {
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [analyses, setAnalyses] = useState<LaunchAnalysis[]>([]);
  const [events, setEvents] = useState<LaunchEvent[]>([]);
  const [backendError, setBackendError] = useState<string | null>(null);
  const [filters, setFilters] = useState<TableFilters>({
    program: "all",
    severity: "all",
    hideFlagged: false,
  });
  const [selected, setSelected] = useState<LaunchAnalysis | null>(null);
  const selectedSignature = useRef<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [nextStatus, nextAnalyses, nextEvents] = await Promise.all([
        getStatus(),
        getRecentAnalyses(200),
        getRecentEvents(200),
      ]);
      setStatus(nextStatus);
      setAnalyses(nextAnalyses);
      setEvents(nextEvents);
      setBackendError(null);

      // Keep the open drawer in sync with the store (analyses can be replaced
      // when a launch is re-assessed).
      const open = selectedSignature.current;
      if (open) {
        const current = nextAnalyses.find((a) => a.signature === open);
        if (current) setSelected(current);
      }
    } catch (error) {
      setBackendError(
        error instanceof Error ? error.message : "the TrenchWatch backend did not answer",
      );
    }
  }, []);

  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, REFRESH_MS);
    return () => clearInterval(timer);
  }, [refresh]);

  const openDetails = useCallback((signature: string) => {
    selectedSignature.current = signature;
    setSelected(analyses.find((a) => a.signature === signature) ?? null);
    // Pull the freshest copy once on open; the interval keeps it fresh after.
    getAnalysis(signature).then((fresh) => {
      if (fresh && selectedSignature.current === signature) setSelected(fresh);
    });
  }, [analyses]);

  const closeDetails = useCallback(() => {
    selectedSignature.current = null;
    setSelected(null);
  }, []);

  const programOptions = ["all", ...new Set(events.map((event) => event.program_label))];

  return (
    <div className="app-shell">
      <TopBar status={status} />

      <div className="filter-bar">
        <label className="filter">
          <span>launchpad</span>
          <select
            value={filters.program}
            onChange={(e) => setFilters((f) => ({ ...f, program: e.target.value }))}
          >
            {programOptions.map((label) => (
              <option key={label} value={label === "all" ? "all" : label}>
                {label === "all" ? "all launchpads" : label}
              </option>
            ))}
          </select>
        </label>

        <label className="filter">
          <span>verdict</span>
          <select
            value={filters.severity}
            onChange={(e) =>
              setFilters((f) => ({ ...f, severity: e.target.value as TableFilters["severity"] }))
            }
          >
            <option value="all">all verdicts</option>
            <option value="critical">critical</option>
            <option value="warn">warning</option>
            <option value="info">note</option>
            <option value="none">no flags found</option>
          </select>
        </label>

        <label className="filter filter-toggle">
          <input
            type="checkbox"
            checked={filters.hideFlagged}
            onChange={(e) => setFilters((f) => ({ ...f, hideFlagged: e.target.checked }))}
          />
          <span>hide flagged launches</span>
        </label>

        <label className="filter filter-disabled" title="Ships with the liquidity-depth signal (S6)">
          <span>min liquidity</span>
          <select disabled>
            <option>coming with S6</option>
          </select>
        </label>

        <label className="filter filter-disabled" title="v1 watches Solana mainnet only">
          <span>chain</span>
          <select disabled>
            <option>solana · mainnet-beta</option>
          </select>
        </label>

        <span className="filter-note">
          {analyses.length} launch{analyses.length === 1 ? "" : "es"} analyzed
        </span>
      </div>

      <main className="content">
        {backendError ? (
          <div className="empty-state">
            <p>The dashboard cannot reach the TrenchWatch backend.</p>
            <p className="dim">
              {backendError}. Start the app with <code>npm run tauri dev</code> — the web preview
              alone has no chain connection.
            </p>
          </div>
        ) : (
          <LaunchTable
            analyses={analyses}
            events={events}
            filters={filters}
            selected={selected?.signature ?? null}
            onSelect={openDetails}
          />
        )}
      </main>

      {selected && <DetailDrawer analysis={selected} onClose={closeDetails} />}
    </div>
  );
}
