import { useState, useEffect } from "react";
import { arkon, StatusEntry } from "../invoke";

export default function Targets() {
  const [statuses, setStatuses] = useState<StatusEntry[]>([]);
  const [loading, setLoading]   = useState(true);
  const [checking, setChecking] = useState(false);
  const root = "/";

  const load = () => {
    setChecking(true);
    arkon.getStatus(root)
      .then(setStatuses)
      .catch(() => setStatuses([]))
      .finally(() => { setLoading(false); setChecking(false); });
  };

  useEffect(load, []);

  if (loading) return <div className="loading">checking targets...</div>;

  const online  = statuses.filter(s => s.online).length;
  const offline = statuses.filter(s => !s.online).length;

  return (
    <div>
      <div className="page-header" style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between" }}>
        <div>
          <div className="page-title">Targets</div>
          <div className="page-sub">
            {online} online · {offline} unreachable
          </div>
        </div>
        <button className="btn btn-primary" onClick={load} disabled={checking}>
          {checking ? "checking..." : "↻  Refresh"}
        </button>
      </div>

      {statuses.length === 0 ? (
        <div className="card"><div className="empty">No targets configured — edit arkon.toml to add targets</div></div>
      ) : (
        statuses.map(s => (
          <div key={s.name} className="row-item">
            <div className={`dot ${s.online ? "dot-green" : "dot-red"}`}></div>
            <div className="row-item-main">
              <div className="row-item-name">{s.name}</div>
              <div className="row-item-sub">
                {s.latency_ms != null ? `${s.latency_ms}ms` : s.online ? "reachable" : "unreachable"}
              </div>
            </div>
            <span className="badge badge-gray">{s.kind}</span>
            <span className={`badge ${s.online ? "badge-green" : "badge-red"}`}>
              {s.online ? "online" : "offline"}
            </span>
          </div>
        ))
      )}
    </div>
  );
}
