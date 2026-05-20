import { useState, useEffect } from "react";
import { arkon, DoctorEntry } from "../invoke";

export default function Doctor() {
  const [checks, setChecks] = useState<DoctorEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [running, setRunning] = useState(false);

  const run = () => {
    setRunning(true);
    arkon.doctorCheck()
      .then(setChecks)
      .catch(() => setChecks([]))
      .finally(() => { setLoading(false); setRunning(false); });
  };

  useEffect(run, []);

  const passed = checks.filter(c => c.ok).length;
  const failed = checks.filter(c => !c.ok).length;

  if (loading) return <div className="loading">running checks...</div>;

  return (
    <div>
      <div className="page-header" style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between" }}>
        <div>
          <div className="page-title">System doctor</div>
          <div className="page-sub">
            {passed} passed · {failed} failed
          </div>
        </div>
        <button className="btn btn-primary" onClick={run} disabled={running}>
          {running ? "checking..." : "↻  Re-run"}
        </button>
      </div>

      <div className="card" style={{ padding: 0, overflow: "hidden" }}>
        <table className="table">
          <thead>
            <tr>
              <th style={{ width: 32 }}></th>
              <th>Dependency</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            {checks.map(c => (
              <tr key={c.name}>
                <td>
                  <div className={`dot ${c.ok ? "dot-green" : "dot-red"}`}></div>
                </td>
                <td style={{ fontFamily: "var(--mono)", fontWeight: 600 }}>{c.name}</td>
                <td style={{ fontSize: 12, color: c.ok ? "var(--muted)" : "var(--red)" }}>
                  {c.message}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {failed > 0 && (
        <div className="card" style={{ borderColor: "rgba(200,74,106,0.3)", background: "var(--red2)" }}>
          <div style={{ fontSize: 12, color: "var(--red)", fontWeight: 600, marginBottom: 6 }}>
            {failed} dependency check{failed !== 1 ? "s" : ""} failed
          </div>
          <div style={{ fontSize: 12, color: "var(--muted)" }}>
            Some ARKON features may not work correctly. Install the missing dependencies and re-run.
          </div>
        </div>
      )}
    </div>
  );
}
