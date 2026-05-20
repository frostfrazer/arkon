import { useState, useEffect, useRef } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { arkon, ProgressEvent, ShipResult, TargetInfo, SnapshotEntry } from "../invoke";

type Mode = "ship" | "rollback" | "promote";

export default function Deploy() {
  const [root, setRoot]         = useState<string>("");
  const [targets, setTargets]   = useState<TargetInfo[]>([]);
  const [snapshots, setSnaps]   = useState<SnapshotEntry[]>([]);
  const [target, setTarget]     = useState<string>("");
  const [fromTarget, setFrom]   = useState<string>("");
  const [query, setQuery]       = useState<string>("latest");
  const [mode, setMode]         = useState<Mode>("ship");
  const [dryRun, setDryRun]     = useState(false);
  const [running, setRunning]   = useState(false);
  const [log, setLog]           = useState<ProgressEvent[]>([]);
  const [result, setResult]     = useState<ShipResult | null>(null);
  const logRef                  = useRef<HTMLDivElement>(null);

  // Auto-scroll log
  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [log]);

  const pickProject = async () => {
    const selected = await open({ directory: true, multiple: false, title: "Select project root" });
    if (!selected || Array.isArray(selected)) return;
    setRoot(selected);
    await arkon.setProjectRoot(selected);
    const t = await arkon.listTargets(selected).catch(() => [] as TargetInfo[]);
    setTargets(t);
    if (t.length > 0) { setTarget(t[0].name); setFrom(t[0].name); }
  };

  const loadSnapshots = async () => {
    if (!root) return;
    const cfg = await arkon.listTargets(root).catch(() => [] as TargetInfo[]);
    if (cfg.length === 0) return;
    // Use first target name as project name proxy
    const proj = cfg[0]?.name ? "my-app" : "my-app";
    const s = await arkon.getSnapshots(proj, fromTarget || undefined).catch(() => [] as SnapshotEntry[]);
    setSnaps(s);
  };

  useEffect(() => { if (mode === "rollback") loadSnapshots(); }, [mode, fromTarget, root]);

  const appendLog = (e: ProgressEvent) => setLog(prev => [...prev, e]);

  const run = async () => {
    if (!root) { alert("Select a project first"); return; }
    setRunning(true);
    setLog([]);
    setResult(null);

    try {
      let res: ShipResult;
      if (mode === "ship") {
        res = await arkon.ship(root, target || null, dryRun, appendLog);
      } else if (mode === "rollback") {
        res = await arkon.rollback(root, query, target || null, appendLog);
      } else {
        const toTarget = targets.find(t => t.name !== fromTarget)?.name || target;
        res = await arkon.promote(root, fromTarget, toTarget, appendLog);
      }
      setResult(res);
    } catch (e: any) {
      appendLog({ type: "error", message: String(e) });
    } finally {
      setRunning(false);
    }
  };

  const logLine = (e: ProgressEvent, i: number) => {
    switch (e.type) {
      case "log":     return <div key={i} className="log-line">{e.message}</div>;
      case "step":    return <div key={i} className="log-step"><span className="log-step-name">{e.step}</span><span className="log-step-val">{e.detail}</span></div>;
      case "success": return <div key={i} className="log-success">✓  {e.message}</div>;
      case "error":   return <div key={i} className="log-error">✗  {e.message}</div>;
      case "done":    return null;
    }
  };

  return (
    <div>
      <div className="page-header">
        <div className="page-title">Deploy</div>
        <div className="page-sub">Build, ship, rollback, and promote — all from here</div>
      </div>

      {/* Project picker */}
      <div className="card">
        <div className="card-header"><div className="card-title">Project</div></div>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            className="input"
            value={root}
            onChange={e => setRoot(e.target.value)}
            placeholder="/path/to/project"
            style={{ flex: 1 }}
          />
          <button className="btn btn-primary" onClick={pickProject}>Browse…</button>
        </div>
      </div>

      {/* Mode tabs */}
      <div style={{ display: "flex", gap: 4, marginBottom: 12 }}>
        {(["ship", "rollback", "promote"] as Mode[]).map(m => (
          <button
            key={m}
            className={`btn${mode === m ? " btn-primary" : ""}`}
            onClick={() => setMode(m)}
            disabled={running}
          >
            {m === "ship" ? "⬆ Ship" : m === "rollback" ? "↩ Rollback" : "⇄ Promote"}
          </button>
        ))}
        <div style={{ flex: 1 }}></div>
        <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "var(--muted)", cursor: "pointer" }}>
          <input type="checkbox" checked={dryRun} onChange={e => setDryRun(e.target.checked)} />
          dry run
        </label>
      </div>

      {/* Mode-specific controls */}
      <div className="card">
        {mode === "ship" && (
          <div>
            <div className="card-header"><div className="card-title">Target</div></div>
            <select
              className="input"
              value={target}
              onChange={e => setTarget(e.target.value)}
              style={{ fontFamily: "var(--mono)" }}
            >
              {targets.length === 0
                ? <option value="">— load a project first —</option>
                : targets.map(t => <option key={t.name} value={t.name}>{t.name} ({t.kind})</option>)
              }
            </select>
          </div>
        )}

        {mode === "rollback" && (
          <div>
            <div className="card-header"><div className="card-title">Snapshot</div></div>
            <div style={{ display: "flex", gap: 8, marginBottom: 10 }}>
              <input
                className="input"
                value={query}
                onChange={e => setQuery(e.target.value)}
                placeholder="latest / date / snapshot ID prefix"
                style={{ flex: 1 }}
              />
            </div>
            {snapshots.length > 0 && (
              <div style={{ marginTop: 8 }}>
                {snapshots.slice(0, 5).map(s => (
                  <div
                    key={s.id}
                    className="row-item"
                    style={{ cursor: "pointer", marginBottom: 4 }}
                    onClick={() => setQuery(s.id)}
                  >
                    <div className="row-item-main">
                      <div className="row-item-name" style={{ fontFamily: "var(--mono)", fontSize: 12 }}>{s.id}</div>
                      <div className="row-item-sub">
                        {s.target} · {new Date(s.deployed_at).toLocaleString()} · {(s.size_bytes / 1_048_576).toFixed(1)}MB
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {mode === "promote" && (
          <div style={{ display: "flex", gap: 12, alignItems: "center" }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 10, color: "var(--muted)", marginBottom: 6, textTransform: "uppercase", letterSpacing: "0.1em" }}>From</div>
              <select className="input" value={fromTarget} onChange={e => setFrom(e.target.value)}>
                {targets.map(t => <option key={t.name} value={t.name}>{t.name}</option>)}
              </select>
            </div>
            <div style={{ color: "var(--gold)", fontSize: 18, paddingTop: 20 }}>→</div>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 10, color: "var(--muted)", marginBottom: 6, textTransform: "uppercase", letterSpacing: "0.1em" }}>To</div>
              <select className="input" value={target} onChange={e => setTarget(e.target.value)}>
                {targets.filter(t => t.name !== fromTarget).map(t =>
                  <option key={t.name} value={t.name}>{t.name}</option>
                )}
              </select>
            </div>
          </div>
        )}
      </div>

      {/* Run button */}
      <button
        className="btn btn-primary"
        onClick={run}
        disabled={running || !root}
        style={{ width: "100%", justifyContent: "center", padding: "10px", marginBottom: 12, fontSize: 14 }}
      >
        {running
          ? "running..."
          : dryRun
            ? `▷  Dry run`
            : mode === "ship" ? "⬆  Deploy" : mode === "rollback" ? "↩  Rollback" : "⇄  Promote"
        }
      </button>

      {/* Progress log */}
      {log.length > 0 && (
        <div className="card" style={{ padding: 0 }}>
          <div
            ref={logRef}
            style={{
              fontFamily: "var(--mono)",
              fontSize: 11,
              lineHeight: 1.9,
              padding: "12px 16px",
              maxHeight: 240,
              overflowY: "auto",
              background: "#060810",
              borderRadius: 10,
            }}
          >
            {log.map(logLine)}
          </div>
        </div>
      )}

      {/* Result */}
      {result && !running && (
        <div className="card" style={{
          borderColor: result.ok ? "rgba(74,200,122,0.3)" : "rgba(200,74,106,0.3)",
          background:  result.ok ? "var(--green2)" : "var(--red2)",
        }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
            <div className={`dot ${result.ok ? "dot-green" : "dot-red"}`}></div>
            <span style={{ fontWeight: 700, fontSize: 13 }}>
              {result.ok ? "Deploy complete" : "Deploy failed"}
            </span>
            <span style={{ color: "var(--muted)", fontSize: 11, fontFamily: "var(--mono)" }}>
              {result.duration_ms}ms · {(result.size_bytes / 1_048_576).toFixed(1)}MB
            </span>
          </div>
          {result.url && (
            <a
              href={result.url}
              target="_blank"
              rel="noreferrer"
              style={{ color: "var(--gold)", fontFamily: "var(--mono)", fontSize: 12 }}
            >
              {result.url}
            </a>
          )}
          {result.snapshot_id && (
            <div style={{ color: "var(--muted)", fontSize: 11, fontFamily: "var(--mono)", marginTop: 4 }}>
              snapshot: {result.snapshot_id}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// Extra CSS for log lines (injected inline since styles.css is global)
const style = document.createElement("style");
style.textContent = `
  .log-line    { color: #5a6080; }
  .log-step    { display: flex; gap: 12px; }
  .log-step-name { color: #c8a84b; min-width: 80px; }
  .log-step-val  { color: #8890b8; }
  .log-success { color: #4ac87a; }
  .log-error   { color: #c84a6a; }
`;
document.head.appendChild(style);
