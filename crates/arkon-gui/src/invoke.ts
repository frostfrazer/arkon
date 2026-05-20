import { invoke } from "@tauri-apps/api/core";
import { Channel } from "@tauri-apps/api/core";

// ─── Progress events (streamed from ship/rollback/promote) ────────────────────

export type ProgressEvent =
  | { type: "log";     message: string }
  | { type: "step";    step: string; detail: string }
  | { type: "success"; message: string; url: string | null }
  | { type: "error";   message: string }
  | { type: "done" };

// ─── Response types ───────────────────────────────────────────────────────────

export interface DetectResult {
  adapter:     string;
  description: string;
  confidence:  number;
}

export interface TargetInfo {
  name: string;
  kind: string;
  host: string | null;
}

export interface DeployEntry {
  id:                   string;
  target:               string;
  adapter:              string;
  status:               "success" | "failed" | "rolledback" | "skipped";
  artifact_fingerprint: string;
  duration_ms:          number;
  size_bytes:           number;
  deployed_at:          string;
}

export interface SnapshotEntry {
  id:          string;
  target:      string;
  adapter:     string;
  deployed_at: string;
  size_bytes:  number;
}

export interface StatusEntry {
  name:       string;
  kind:       string;
  online:     boolean;
  latency_ms: number | null;
  host:       string | null;
}

export interface CostEntry {
  target:             string;
  upload_usd:         number;
  egress_monthly_usd: number;
  breakdown:          string;
}

export interface DoctorEntry {
  name:    string;
  ok:      boolean;
  message: string;
}

export interface ShipResult {
  ok:                   boolean;
  target:               string;
  url:                  string | null;
  snapshot_id:          string | null;
  artifact_fingerprint: string;
  size_bytes:           number;
  duration_ms:          number;
}

// ─── Typed IPC wrappers ───────────────────────────────────────────────────────

export const arkon = {
  // READ
  detectProject:   (root: string): Promise<DetectResult>     => invoke("detect_project", { root }),
  getProjectRoot:  ():             Promise<string | null>     => invoke("get_project_root"),
  setProjectRoot:  (root: string): Promise<void>              => invoke("set_project_root", { root }),
  listTargets:     (root: string): Promise<TargetInfo[]>      => invoke("list_targets", { root }),
  getDeployHistory:(limit = 50):   Promise<DeployEntry[]>     => invoke("get_deploy_history", { limit }),
  getSnapshots:    (project: string, target?: string): Promise<SnapshotEntry[]> =>
    invoke("get_snapshots", { project, target: target ?? null }),
  listSecrets:     (project: string): Promise<string[]>       => invoke("list_secrets", { project }),
  setSecret:       (project: string, key: string, value: string): Promise<void> =>
    invoke("set_secret", { project, key, value }),
  deleteSecret:    (project: string, key: string): Promise<void> =>
    invoke("delete_secret", { project, key }),
  getStatus:       (root: string): Promise<StatusEntry[]>     => invoke("get_status", { root }),
  getCostEstimate: (root: string): Promise<CostEntry[]>       => invoke("get_cost_estimate", { root }),
  doctorCheck:     ():             Promise<DoctorEntry[]>      => invoke("doctor_check"),

  // WRITE (with progress streaming)
  ship(
    root: string,
    target: string | null,
    dryRun: boolean,
    onProgress: (e: ProgressEvent) => void,
  ): Promise<ShipResult> {
    const channel = new Channel<ProgressEvent>();
    channel.onmessage = onProgress;
    return invoke("ship", {
      root,
      target,
      dryRun,
      onProgress: channel,
    });
  },

  rollback(
    root: string,
    query: string,
    target: string | null,
    onProgress: (e: ProgressEvent) => void,
  ): Promise<ShipResult> {
    const channel = new Channel<ProgressEvent>();
    channel.onmessage = onProgress;
    return invoke("rollback", { root, query, target, onProgress: channel });
  },

  promote(
    root: string,
    from: string,
    to: string,
    onProgress: (e: ProgressEvent) => void,
  ): Promise<ShipResult> {
    const channel = new Channel<ProgressEvent>();
    channel.onmessage = onProgress;
    return invoke("promote", { root, from, to, onProgress: channel });
  },
};
