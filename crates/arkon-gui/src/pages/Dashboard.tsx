import { useState, useEffect } from "react";
import { arkon, DetectResult, TargetInfo, DeployEntry } from "../invoke";

export default function Dashboard() {
  const [detect, setDetect]   = useState<DetectResult | null>(null);
  const [targets, setTargets] = useState<TargetInfo[]>([]);
  const [recent, setRecent]   = useState<DeployEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [root]                = useState("/");

  useEffect(() => {
    Promise.all([
      arkon.detectProject(root).catch(() => null),
      arkon.listTargets(root).catch(() => [] as TargetInfo[]),
      arkon.getDeployHistory(5).catch(() => [] as DeployEntry[]),
    ]).then(([d, t, h]) => {
      setDetect(d);
      setTargets(t);
      setRecent(h);
      setLoading(false);
    });
  }, [root]);

  if (loading) return <div className="loading">loading dashboard...</div>;

  const lastDeploy = recent[0];

  return (
    <div>
      <div className="page-header">
        <div className="page-title">Dashboard</div>
        <div className="page-sub">Project overview and quick actions</div>
      </div>

      {/* Metrics row */}
      <div className="grid-4">
        <div className="metric-card">
          <div className="metric-val">{targets.length}</div>
          <div className="metric-label">Targets</div>
        </div>
        <div className="metric-card">
          <div className="metric-val">{recent.length}</div>
          <div className="metric-label">Recent deploys</div>
        </div>
        <div className="metric-card">
          <div className="metric-val">{detect ? (detect.confidence * 100).toFixed(0) + "%" : "—"}</div>
          <div className="metric-label">Detect confidence</div>
        </div>
        <div className="metric-card">
          <div className="metric-val">{lastDeploy ? lastDeploy.duration_ms + "ms" : "—"}</div>
          <div className="metric-label">Last deploy time</div>
        </div>
      </div>

      <div className="grid-2">
        {/* Detected project */}
        <div className="card">
          <div className="card-header"><div className="card-title">Detected project</div></div>
          {detect ? (
            <div>
              <div style={{ fontSize: 16, fontWeight: 700, color: "var(--gold)", marginBottom: 6, fontFamily: "var(--mono)" }}>
                {detect.adapter}
              </div>
              <div style={{ fontSize: 12, color: "var(--muted)", marginBottom: 10 }}>{detect.description}</div>
              <div className="badge badge-cyan">{(detect.confidence * 100).toFixed(0)}% confidence</div>
            </div>
          ) : (
            <div className="empty">No arkon.toml found — run <code>arkon init</code></div>
          )}
        </div>

        {/* Last deploy */}
        <div className="card">
          <div className="card-header"><div className="card-title">Last deploy</div></div>
          {lastDeploy ? (
            <div>
              <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
                <div className={`dot ${lastDeploy.status === "success" ? "dot-green" : "dot-red"}`}></div>
                <span style={{ fontFamily: "var(--mono)", fontSize: 13 }}>{lastDeploy.target}</span>
                <span className={`badge ${lastDeploy.status === "success" ? "badge-green" : "badge-red"}`}>
                  {lastDeploy.status}
                </span>
              </div>
              <div style={{ fontSize: 11, color: "var(--muted)", fontFamily: "var(--mono)" }}>
                {new Date(lastDeploy.deployed_at).toLocaleString()} · {(lastDeploy.size_bytes / 1_048_576).toFixed(1)}MB · {lastDeploy.duration_ms}ms
              </div>
            </div>
          ) : (
            <div className="empty">No deploys yet</div>
          )}
        </div>
      </div>

      {/* Targets */}
      <div className="card">
        <div className="card-header"><div className="card-title">Configured targets</div></div>
        {targets.length === 0 ? (
          <div className="empty">No targets configured in arkon.toml</div>
        ) : (
          targets.map(t => (
            <div key={t.name} className="row-item">
              <div className="dot dot-gray"></div>
              <div className="row-item-main">
                <div className="row-item-name">{t.name}</div>
                {t.host && <div className="row-item-sub">{t.host}</div>}
              </div>
              <span className="badge badge-gray">{t.kind}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
