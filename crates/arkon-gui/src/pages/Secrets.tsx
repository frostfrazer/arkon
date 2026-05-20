import { useState, useEffect } from "react";
import { arkon } from "../invoke";

export default function Secrets() {
  const [keys, setKeys]         = useState<string[]>([]);
  const [loading, setLoading]   = useState(true);
  const [newKey, setNewKey]     = useState("");
  const [newVal, setNewVal]     = useState("");
  const [adding, setAdding]     = useState(false);
  const [project]               = useState("my-app"); // loaded from arkon.toml in production

  const reload = () => {
    arkon.listSecrets(project)
      .then(setKeys)
      .catch(() => setKeys([]))
      .finally(() => setLoading(false));
  };

  useEffect(reload, []);

  const handleAdd = async () => {
    if (!newKey.trim() || !newVal.trim()) return;
    setAdding(true);
    try {
      await arkon.setSecret(project, newKey.trim().toUpperCase(), newVal.trim());
      setNewKey("");
      setNewVal("");
      reload();
    } finally {
      setAdding(false);
    }
  };

  const handleDelete = async (key: string) => {
    if (!confirm(`Delete secret "${key}"?`)) return;
    await arkon.deleteSecret(project, key).catch(() => {});
    reload();
  };

  if (loading) return <div className="loading">loading vault...</div>;

  return (
    <div>
      <div className="page-header">
        <div className="page-title">Secrets vault</div>
        <div className="page-sub">AES-256-GCM encrypted · machine-locked · never written to arkon.toml</div>
      </div>

      {/* Add secret */}
      <div className="card">
        <div className="card-header"><div className="card-title">Add secret</div></div>
        <div style={{ display: "flex", gap: 8 }}>
          <input
            className="input"
            placeholder="KEY_NAME"
            value={newKey}
            onChange={e => setNewKey(e.target.value.toUpperCase())}
            style={{ flex: "0 0 200px", textTransform: "uppercase" }}
          />
          <input
            className="input"
            type="password"
            placeholder="value"
            value={newVal}
            onChange={e => setNewVal(e.target.value)}
            style={{ flex: 1 }}
          />
          <button
            className="btn btn-primary"
            onClick={handleAdd}
            disabled={adding || !newKey.trim() || !newVal.trim()}
          >
            {adding ? "saving..." : "Save"}
          </button>
        </div>
      </div>

      {/* Secret list */}
      <div className="card" style={{ padding: 0, overflow: "hidden" }}>
        <div className="card-header" style={{ padding: "12px 16px" }}>
          <div className="card-title">{keys.length} secret{keys.length !== 1 ? "s" : ""} stored</div>
        </div>
        {keys.length === 0 ? (
          <div className="empty">No secrets stored yet</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Key</th>
                <th>Value</th>
                <th style={{ width: 80 }}></th>
              </tr>
            </thead>
            <tbody>
              {keys.map(k => (
                <tr key={k}>
                  <td className="mono" style={{ color: "var(--cyan)" }}>{k}</td>
                  <td className="mono" style={{ color: "var(--dim)" }}>••••••••••••</td>
                  <td>
                    <button
                      className="btn btn-danger btn-sm"
                      onClick={() => handleDelete(k)}
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
