// M1 scaffold shell. The mission-control dashboard lands in M4.
export default function App() {
  return (
    <div className="app-shell">
      <header className="topbar">
        <span className="brand">TRENCHWATCH</span>
        <span className="status-pill" data-state="idle">
          LISTENER OFFLINE
        </span>
      </header>
      <main className="placeholder-panel">
        <h1>Solana launch monitor</h1>
        <p>
          Real-time rug-pull risk scoring with plain-language evidence.
          Chain listener ships in milestone M2 — see the roadmap in the README.
        </p>
        <code>npm run tauri dev</code>
      </main>
    </div>
  );
}
