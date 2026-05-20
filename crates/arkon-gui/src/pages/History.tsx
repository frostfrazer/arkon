import { useState, useEffect } from "react";
import { arkon, DeployEntry } from "../invoke";

export default function History() {
  const [entries, setEntries] = useState<DeployEntry[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    arkon.getDeployHistory(100)
      .then(setEntries)
      .catch(() => setEntries([]))
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div className="loading">loading history...</div>;

  function fmtSize(bytes: number) {
    return (bytes / 1_048_576).toFixed(1) + " MB";
  }

  function fmtDate(iso: string) {
    return new Date(iso).toLocaleString(undefined, {
      month: "short", day: "numeric",
      hour: "2-digit", minute: "2-digit",
    });
  }

  return (
    <div>
      <div className="page-header">
        <div className="page-title">Deploy history</div>
        <div className="page-sub">HMAC-chained audit log — every entry is cryptographically linked</div>
      </div>

      <div className="card" style={{ padding: 0, overflow: "hidden" }}>
        {entries.length === 0 ? (
          <div className="empty">No deploy history yet — run <code>arkon ship</code> to get started</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>ID</th>
                <th>Target</th>
                <th>Adapter</th>
                <th>Status</th>
                <th>Size</th>
                <th>Duration</th>
                <th>Fingerprint</th>
                <th>Deployed</th>
              </tr>
            </thead>
            <tbody>
              {entries.map(e => (
                <tr key={e.id}>
                  <td className="mono">{e.id}</td>
                  <td style={{ fontWeight: 600 }}>{e.target}</td>
                  <td className="mono">{e.adapter}</td>
                  <td>
                    <span className={`badge ${e.status === "success" ? "badge-green" : e.status === "failed" ? "badge-red" : "badge-gray"}`}>
                      {e.status}
                    </span>
                  </td>
                  <td className="mono">{fmtSize(e.size_bytes)}</td>
                  <td className="mono">{e.duration_ms}ms</td>
                  <td className="mono">{e.artifact_fingerprint}</td>
                  <td className="mono" style={{ fontSize: 11 }}>{fmtDate(e.deployed_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
