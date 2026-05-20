import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { arkon, CostEntry } from "../invoke";

export default function Cost() {
  const [root, setRoot]       = useState("");
  const [costs, setCosts]     = useState<CostEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError]     = useState<string | null>(null);
  const [built, setBuilt]     = useState(false);

  const pickProject = async () => {
    const selected = await open({ directory: true, multiple: false, title: "Select project root" });
    if (!selected || Array.isArray(selected)) return;
    setRoot(selected);
    setCosts([]);
    setBuilt(false);
  };

  const estimate = async () => {
    if (!root) { alert("Select a project first"); return; }
    setLoading(true);
    setError(null);
    try {
      const results = await arkon.getCostEstimate(root);
      setCosts(results);
      setBuilt(true);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const totalUpload  = costs.reduce((s, c) => s + c.upload_usd, 0);
  const totalEgress  = costs.reduce((s, c) => s + c.egress_monthly_usd, 0);

  return (
    <div>
      <div className="page-header">
        <div className="page-title">Cost estimate</div>
        <div className="page-sub">
          Builds your artifact locally, then calculates expected cost per target — without deploying
        </div>
      </div>

      {/* Project picker */}
      <div className="card">
        <div className="card-header"><div className="card-title">Project</div></div>
        <div style={{ display: "flex", gap: 8 }}>
          <input
            className="input"
            value={root}
            onChange={e => setRoot(e.target.value)}
            placeholder="/path/to/project"
            style={{ flex: 1 }}
          />
          <button className="btn" onClick={pickProject}>Browse…</button>
          <button
            className="btn btn-primary"
            onClick={estimate}
            disabled={loading || !root}
          >
            {loading ? "building…" : "Estimate"}
          </button>
        </div>
        {loading && (
          <div style={{ marginTop: 10, fontSize: 12, color: "var(--muted)", fontFamily: "var(--mono)" }}>
            building artifact to measure size… this may take a moment
          </div>
        )}
      </div>

      {error && (
        <div className="card" style={{ borderColor: "rgba(200,74,106,0.3)", background: "var(--red2)" }}>
          <div style={{ color: "var(--red)", fontSize: 12 }}>✗  {error}</div>
        </div>
      )}

      {built && costs.length > 0 && (
        <>
          {/* Summary */}
          <div className="grid-2">
            <div className="metric-card">
              <div className="metric-val">${totalUpload.toFixed(4)}</div>
              <div className="metric-label">Upload cost (one-time)</div>
            </div>
            <div className="metric-card">
              <div className="metric-val">${totalEgress.toFixed(4)}</div>
              <div className="metric-label">Est. egress / month</div>
            </div>
          </div>

          {/* Per-target table */}
          <div className="card" style={{ padding: 0, overflow: "hidden" }}>
            <table className="table">
              <thead>
                <tr>
                  <th>Target</th>
                  <th>Upload</th>
                  <th>Egress / mo</th>
                  <th>Breakdown</th>
                </tr>
              </thead>
              <tbody>
                {costs.map(c => (
                  <tr key={c.target}>
                    <td style={{ fontWeight: 600 }}>{c.target}</td>
                    <td className="mono">
                      {c.upload_usd === 0 ? (
                        <span className="badge badge-green">free</span>
                      ) : (
                        `$${c.upload_usd.toFixed(4)}`
                      )}
                    </td>
                    <td className="mono">
                      {c.egress_monthly_usd === 0 ? (
                        <span className="badge badge-green">free</span>
                      ) : (
                        `$${c.egress_monthly_usd.toFixed(4)}/mo`
                      )}
                    </td>
                    <td style={{ fontSize: 11, color: "var(--muted)" }}>{c.breakdown}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="card" style={{ borderColor: "rgba(200,168,75,0.2)", background: "rgba(200,168,75,0.05)" }}>
            <div style={{ fontSize: 12, color: "var(--muted)" }}>
              ◈  Costs are estimates based on published pricing. Actual costs depend on traffic, 
              compression, and provider pricing changes. WebRTC and local targets are always free.
            </div>
          </div>
        </>
      )}

      {built && costs.length === 0 && (
        <div className="card">
          <div className="empty">No targets configured in arkon.toml — run <code>arkon init</code></div>
        </div>
      )}
    </div>
  );
}
