export interface InstanceInfo {
  name: string;
  loader: string;
  mcver: string;
  jar: string;
  port: string;
  motd: string;
  has_mods: boolean;
  has_plugins: boolean;
}

export interface ModRow {
  file: string;
  name: string;
  version: string;
  disabled: boolean;
  size: number;
  mtime: number;
  kind: "mod" | "plugin";
  dep_for?: string | null;
  url?: string | null;
  source?: string | null;
  icon?: string | null;
  configs?: string[];
}

export interface UpdateItem {
  file: string;
  name: string;
  installed: string;
  known: boolean;
  project: string | number | null;
  latest: string | null;
  update: string | null;
  latest_url: string | null;
  latest_filename: string | null;
  source: string | null;
  blocked?: boolean;
  installed_name?: string | null;
}

export interface UpdatesState {
  status: "idle" | "checking" | "done" | "error";
  ts?: number;
  items?: Record<string, UpdateItem>;
  phase?: string;
  done?: number;
  total?: number;
  cur?: string | null;
  error?: string;
}

export interface CheckProgressState {
  status: "running" | "done";
  instance: string;
  phase: string;
  done: number;
  total: number;
  cur: string | null;
}

export interface SearchHit {
  project_id: string | number;
  slug: string;
  title: string;
  icon: string;
  downloads: number;
  desc: string;
  author?: string;
}

export interface CartItem {
  source: "modrinth" | "curseforge";
  project_id: string;
  slug: string;
  title: string;
  desc: string;
  author: string;
  downloads: number;
  icon: string;
  mc: string;
  loader: string;
  deps: boolean;
}

export interface TrashEntry {
  file: string;
  size: number;
  mtime: number;
  kind: "jar" | "config";
}

export interface DiffRow {
  type: "version" | "only_a" | "only_b";
  name: string;
  a: string | null;
  b: string | null;
  ad: boolean | null;
  bd: boolean | null;
}

export interface ProgressJob {
  status: "running" | "done" | "error";
  kind: "install" | "update";
  instance: string;
  label: string;
  phase: string;
  cur: string | null;
  files_done: number;
  files_total: number;
  bytes_done: number;
  bytes_total: number | null;
  fixed_total: boolean;
  error: string | null;
  ts: number;
}

export interface BatchStatus {
  status: "idle" | "running" | "done";
  total: number;
  done: number;
  current: string | null;
  results: Array<{ file: string; ok?: string; error?: string }>;
  skipped?: string[];
}
