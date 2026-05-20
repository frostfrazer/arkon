import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import Dashboard from "./pages/Dashboard";
import Deploy from "./pages/Deploy";
import History from "./pages/History";
import Targets from "./pages/Targets";
import Secrets from "./pages/Secrets";
import Doctor from "./pages/Doctor";
import Cost from "./pages/Cost";
import "./styles.css";

type Page = "dashboard" | "deploy" | "history" | "targets" | "secrets" | "doctor" | "cost";

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");

  useEffect(() => {
    const unlisten = listen<string>("navigate", (e) => {
      const path = e.payload.replace("/", "") as Page;
      if (path) setPage(path as Page);
    });
    // Tray "Deploy" menu item fires this event
    const unlistenShip = listen("tray-ship", () => setPage("deploy"));
    const unlistenPreview = listen("tray-preview", () => setPage("deploy"));
    return () => {
      unlisten.then(f => f());
      unlistenShip.then(f => f());
      unlistenPreview.then(f => f());
    };
  }, []);

  const nav: { id: Page; label: string; icon: string }[] = [
    { id: "dashboard", label: "Dashboard", icon: "▲" },
    { id: "deploy",    label: "Deploy",    icon: "⬆" },
    { id: "history",   label: "History",   icon: "◎" },
    { id: "targets",   label: "Targets",   icon: "⬡" },
    { id: "cost",      label: "Cost",      icon: "◈" },
    { id: "secrets",   label: "Secrets",   icon: "⊛" },
    { id: "doctor",    label: "Doctor",    icon: "⊕" },
  ];

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="sidebar-logo">
          <span className="logo-mark">▲</span>
          <span className="logo-text">ARKON</span>
        </div>
        <nav className="sidebar-nav">
          {nav.map(n => (
            <button
              key={n.id}
              className={`nav-item${page === n.id ? " active" : ""}`}
              onClick={() => setPage(n.id)}
            >
              <span className="nav-icon">{n.icon}</span>
              <span className="nav-label">{n.label}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className="version">v0.1.0</span>
        </div>
      </aside>
      <main className="content">
        {page === "dashboard" && <Dashboard />}
        {page === "deploy"    && <Deploy />}
        {page === "history"   && <History />}
        {page === "targets"   && <Targets />}
        {page === "cost"      && <Cost />}
        {page === "secrets"   && <Secrets />}
        {page === "doctor"    && <Doctor />}
      </main>
    </div>
  );
}
