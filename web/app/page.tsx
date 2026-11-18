"use client";

import { useEffect, useRef, useState } from "react";
import { api, post, query } from "../lib/api";
import type {
  BatchStatus,
  CartItem,
  CheckProgressState,
  DiffRow,
  InstanceInfo,
  ModRow,
  ProgressJob,
  SearchHit,
  TrashEntry,
  UpdatesState,
} from "../lib/types";
import { Sidebar } from "../components/Sidebar";
import { ModsView } from "../components/ModsView";
import { AddModView } from "../components/AddModView";
import { TrashView } from "../components/TrashView";
import { DiffView } from "../components/DiffView";
import { ProgressOverlay } from "../components/ProgressOverlay";
import { CheckProgressOverlay } from "../components/CheckProgressOverlay";
import { ConfigModal } from "../components/ConfigModal";
import { VersionModal } from "../components/VersionModal";
import { OrphansModal } from "../components/OrphansModal";
import { RootPicker } from "../components/RootPicker";
import { SettingsModal } from "../components/SettingsModal";
import { SettingsProvider, useSettings } from "../lib/i18n";

type View = "mods" | "add" | "trash" | "diff";
type SortKey = "name" | "version" | "size" | "mtime" | "status";

interface Toast {
  id: number;
  message: string;
  error: boolean;
}

function delay(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export default function Page() {
  return (
    <SettingsProvider>
      <PageContent />
    </SettingsProvider>
  );
}

function PageContent() {
  const { t } = useSettings();
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [mods, setMods] = useState<ModRow[]>([]);
  const [updates, setUpdates] = useState<UpdatesState | null>(null);
  const [view, setView] = useState<View>("mods");
  const [loadingMods, setLoadingMods] = useState(false);
  const [filter, setFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "enabled" | "disabled">("all");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDir, setSortDir] = useState<1 | -1>(1);
  const [selectedFiles, setSelectedFiles] = useState<Set<string>>(new Set());
  const [statusText, setStatusText] = useState("");
  const [checkProgress, setCheckProgress] = useState<CheckProgressState | null>(null);

  const [hits, setHits] = useState<SearchHit[]>([]);
  const [hitsSource, setHitsSource] = useState<"modrinth" | "curseforge">("modrinth");
  const [searchLoading, setSearchLoading] = useState(false);
  const [cart, setCart] = useState<CartItem[]>([]);

  const [trashEntries, setTrashEntries] = useState<TrashEntry[]>([]);
  const [trashLoading, setTrashLoading] = useState(false);

  const [toasts, setToasts] = useState<Toast[]>([]);
  const [progress, setProgress] = useState<ProgressJob | null>(null);

  const [root, setRoot] = useState("");
  const [cfOn, setCfOn] = useState(false);
  const [rootPicker, setRootPicker] = useState(false);
  const [settingsModal, setSettingsModal] = useState(false);
  const [configModal, setConfigModal] = useState<{ file: string; configs: string[] } | null>(null);
  const [versionModal, setVersionModal] = useState<string | null>(null);
  const [orphansModal, setOrphansModal] = useState<{ instance: string; files: string[] } | null>(null);

  const generation = useRef(0);
  const toastId = useRef(0);
  const pollTimer = useRef<ReturnType<typeof setInterval> | null>(null);
  const pollStart = useRef(0);

  function toast(message: string, error = false) {
    const id = ++toastId.current;
    setToasts((current) => [...current, { id, message, error }]);
    setTimeout(() => {
      setToasts((current) => current.filter((item) => item.id !== id));
    }, error ? 6000 : 2500);
  }

  async function loadSources() {
    try {
      const sources = await api<{ modrinth: boolean; curseforge: boolean }>("/api/sources");
      setCfOn(sources.curseforge);
    } catch {
      setCfOn(false);
    }
  }

  async function loadRoot() {
    try {
      const response = await api<{ root: string | null }>("/api/config/root");
      setRoot(response.root || "");
    } catch {
      setRoot("");
    }
  }

  async function loadInstances() {
    const list = await api<InstanceInfo[]>("/api/instances");
    setInstances(list);
  }

  async function refreshCachedUpdates(name: string) {
    try {
      const cached = await api<UpdatesState>(`/api/updates/${encodeURIComponent(name)}/cached`);
      if (cached.status === "done") setUpdates(cached);
    } catch {
      // keep the previous state
    }
  }

  async function loadMods(name = selected, guard?: number) {
    if (!name) return;
    const rows = await api<ModRow[]>(`/api/instances/${encodeURIComponent(name)}/mods`);
    if (guard !== undefined && guard !== generation.current) return;
    setMods(rows);
    setLoadingMods(false);
  }

  async function pick(name: string) {
    generation.current += 1;
    const current = generation.current;
    setSelected(name);
    setView("mods");
    setMods([]);
    setUpdates(null);
    setSelectedFiles(new Set());
    setFilter("");
    setStatusFilter("all");
    setStatusText("");
    setLoadingMods(true);
    try {
      const cached = await api<UpdatesState>(`/api/updates/${encodeURIComponent(name)}/cached`);
      if (current === generation.current && cached.status === "done") setUpdates(cached);
    } catch {
      // ignore
    }
    if (current !== generation.current) return;
    try {
      await loadMods(name, current);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
      setLoadingMods(false);
    }
  }

  async function checkUpdates() {
    if (!selected) return;
    const checking = selected;
    setStatusText(t("status_checking"));
    setCheckProgress({
      status: "running",
      instance: checking,
      phase: "hashing",
      done: 0,
      total: 0,
      cur: null,
    });
    try {
      let state = await api<UpdatesState>(`/api/updates/${encodeURIComponent(checking)}?force=1`);
      while (state.status === "checking") {
        const counts = state.total ? ` ${state.done}/${state.total}` : "";
        setStatusText(`${state.phase || "checking"}${counts}…`);
        setCheckProgress({
          status: "running",
          instance: checking,
          phase: state.phase || "hashing",
          done: Number(state.done) || 0,
          total: Number(state.total) || 0,
          cur: state.cur || null,
        });
        await delay(400);
        state = await api<UpdatesState>(`/api/updates/${encodeURIComponent(checking)}`);
      }
      if (state.status !== "done") {
        toast(t("status_check_failed", { e: state.error || state.status }), true);
        setStatusText("");
        setCheckProgress(null);
        return;
      }
      setUpdates(state);
      const total = Number(state.total) || 0;
      setCheckProgress({
        status: "done",
        instance: checking,
        phase: "done",
        done: Number(state.done) || total,
        total,
        cur: null,
      });
      const updateCount = Object.values(state.items || {}).filter((item) => item.update).length;
      const unknownCount = Object.values(state.items || {}).filter((item) => !item.known).length;
      setStatusText(
        t("status_summary", {
          u: updateCount,
          time: new Date((state.ts || 0) * 1000).toLocaleTimeString(),
        }) + (unknownCount ? t("status_unknown", { n: unknownCount }) : "")
      );
      setTimeout(() => setCheckProgress(null), 2000);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
      setStatusText("");
      setCheckProgress(null);
    }
  }

  function startProgress(kind: "install" | "update", label: string, total: number) {
    pollStart.current = Date.now() / 1000;
    if (pollTimer.current) clearInterval(pollTimer.current);
    setProgress({
      status: "running",
      kind,
      instance: selected || "",
      label,
      phase: "preparing",
      cur: null,
      files_done: 0,
      files_total: total || 1,
      bytes_done: 0,
      bytes_total: null,
      fixed_total: true,
      error: null,
      ts: pollStart.current,
    });
    pollTimer.current = setInterval(() => void pollProgress(), 400);
    void pollProgress();
  }

  function stopProgress() {
    if (pollTimer.current) clearInterval(pollTimer.current);
    pollTimer.current = null;
    setProgress(null);
  }

  async function pollProgress() {
    if (!selected) return;
    try {
      const response = await api<{ install: ProgressJob | null; update: ProgressJob | null }>(
        `/api/progress/${encodeURIComponent(selected)}`
      );
      const candidates = [response.update, response.install].filter(
        (job): job is ProgressJob => Boolean(job && job.ts >= pollStart.current - 2)
      );
      const running = candidates.find((job) => job.status === "running");
      const failed = candidates.find((job) => job.status === "error");
      const done = candidates.find((job) => job.status === "done");
      setProgress(running || failed || done || null);
    } catch {
      // ignore transient polling errors
    }
  }

  async function toggle(file: string) {
    const snapshot = mods;
    const nextFile = file.endsWith(".disabled")
      ? file.slice(0, -".disabled".length)
      : `${file}.disabled`;
    setMods((current) =>
      current.map((mod) =>
        mod.file === file ? { ...mod, file: nextFile, disabled: !mod.disabled } : mod
      )
    );
    setSelectedFiles((prev) => {
      const next = new Set(prev);
      if (next.delete(file)) next.add(nextFile);
      return next;
    });
    try {
      const result = await post<{ file: string }>("/api/mods/toggle", { instance: selected, file });
      setMods((current) =>
        current.map((mod) =>
          mod.file === nextFile ? { ...mod, file: result.file } : mod
        )
      );
    } catch (e) {
      setMods(snapshot);
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function trashOne(file: string) {
    if (!confirm(t("confirm_trash_one", { file }))) return;
    const snapshot = mods;
    setMods((current) => current.filter((mod) => mod.file !== file));
    setSelectedFiles((prev) => {
      const next = new Set(prev);
      next.delete(file);
      return next;
    });
    try {
      const result = await post<{ file: string; configs?: number }>("/api/mods/trash", {
        instance: selected,
        file,
      });
      toast(t("toast_trashed", { file }) + (result.configs ? t("toast_trashed_configs", { n: result.configs }) : ""));
    } catch (e) {
      setMods(snapshot);
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function trashSelected() {
    const files = [...selectedFiles].filter((file) => mods.some((mod) => mod.file === file));
    if (!files.length) {
      toast(t("toast_no_selected_mods"), true);
      return;
    }
    if (!confirm(t("confirm_trash_selected", { n: files.length }))) return;
    const snapshot = mods;
    setMods((current) => current.filter((mod) => !files.includes(mod.file)));
    setSelectedFiles(new Set());
    try {
      const result = await post<{ moved: string[]; configs: number; errors: Array<{ file: string; error: string }> }>(
        "/api/mods/trash_selected",
        { instance: selected, files }
      );
      const failed = result.errors?.length || 0;
      toast(
        t("toast_trashed_bulk", { n: result.moved.length }) +
          (result.configs ? t("toast_configs_moved", { n: result.configs }) : "") +
          (failed ? t("toast_failed_count", { n: failed }) : ""),
        failed > 0
      );
      if (failed > 0) await loadMods();
    } catch (e) {
      setMods(snapshot);
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function updateAll() {
    if (!updates) {
      toast(t("toast_run_check"), true);
      return;
    }
    const count = Object.values(updates.items || {}).filter((item) => item.update).length;
    if (!count) {
      toast(t("toast_no_update"), true);
      return;
    }
    if (!confirm(t("confirm_update_all", { n: count }))) return;

    startProgress("update", `${count} mod`, count);
    try {
      await post("/api/update/all", { instance: selected });
    } catch (e) {
      stopProgress();
      toast(e instanceof Error ? e.message : String(e), true);
      return;
    }

    try {
      for (;;) {
        await delay(1500);
        let batch: BatchStatus;
        try {
          batch = await api<BatchStatus>(`/api/update/all/${encodeURIComponent(selected!)}`);
        } catch {
          continue;
        }
        if (batch.status === "running") {
          setStatusText(t("status_updating", { done: batch.done, total: batch.total, cur: batch.current || "" }));
          continue;
        }
        const ok = batch.results.filter((row) => row.ok).length;
        const bad = batch.results.filter((row) => row.error);
        await refreshCachedUpdates(selected!);
        await loadMods();
        stopProgress();
        setStatusText("");
        toast(
          bad.length
            ? t("toast_update_all_fail", { ok, n: bad.length }) + "\n" + bad.map((row) => `${row.file} — ${row.error}`).join("\n")
            : t("toast_update_all_ok", { ok }),
          bad.length > 0
        );
        break;
      }
    } finally {
      // ensure overlay is removed if something aborts
      if (pollTimer.current) stopProgress();
    }
  }

  async function updateSelected() {
    if (!updates) {
      toast(t("toast_run_check"), true);
      return;
    }
    const files = [...selectedFiles].filter((file) => updates.items?.[file]?.update);
    if (!files.length) {
      toast(t("toast_no_updatable"), true);
      return;
    }
    if (!confirm(t("confirm_update_selected", { n: files.length }))) return;

    startProgress("update", `${files.length} mod`, files.length);
    try {
      await post("/api/update/selected", { instance: selected, files });
    } catch (e) {
      stopProgress();
      toast(e instanceof Error ? e.message : String(e), true);
      return;
    }

    try {
      for (;;) {
        await delay(1500);
        let batch: BatchStatus;
        try {
          batch = await api<BatchStatus>(`/api/update/all/${encodeURIComponent(selected!)}`);
        } catch {
          continue;
        }
        if (batch.status === "running") {
          setStatusText(t("status_updating", { done: batch.done, total: batch.total, cur: batch.current || "" }));
          continue;
        }
        const ok = batch.results.filter((row) => row.ok).length;
        const bad = batch.results.filter((row) => row.error);
        await refreshCachedUpdates(selected!);
        await loadMods();
        stopProgress();
        setStatusText("");
        toast(
          bad.length
            ? t("toast_update_sel_fail", { ok, n: bad.length }) + "\n" + bad.map((row) => `${row.file} — ${row.error}`).join("\n")
            : t("toast_update_sel_ok", { ok }),
          bad.length > 0
        );
        setSelectedFiles(new Set());
        break;
      }
    } finally {
      if (pollTimer.current) stopProgress();
    }
  }

  async function applyVersion(file: string, versionId: string, mc: string, loader: string) {
    startProgress("update", file, 1);
    try {
      const result = await post<{ file: string }>("/api/update/apply", {
        instance: selected,
        file,
        version: versionId,
        mc,
        loader,
      });
      await refreshCachedUpdates(selected!);
      await loadMods();
      stopProgress();
      toast(t("toast_installed", { file: result.file }));
    } catch (e) {
      stopProgress();
      toast(e instanceof Error ? e.message : String(e), true);
      throw e;
    }
  }

  function openConfig(file: string) {
    const mod = mods.find((row) => row.file === file);
    const configs = mod?.configs || [];
    if (!configs.length) {
      toast(t("toast_no_config", { file }), true);
      return;
    }
    if (configs.length === 1) {
      void openConfigFile(configs[0]);
      return;
    }
    setConfigModal({ file, configs });
  }

  async function openConfigFile(rel: string) {
    try {
      await api(`/api/config/open?${query({ instance: selected || "", file: rel })}`);
      toast(t("toast_open_config", { rel }));
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function openConfigDir(dir: string) {
    try {
      const result = await api<{ opened: string }>(
        `/api/config/folder?${query({ instance: selected || "", dir })}`
      );
      toast(t("toast_open_folder", { opened: result.opened }));
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function openUrl(url: string) {
    if (!url) return;
    const isTauri = Boolean(
      (window as unknown as { __TAURI_INTERNALS__?: unknown; __TAURI__?: unknown }).__TAURI_INTERNALS__ ||
        (window as unknown as { __TAURI__?: unknown }).__TAURI__
    );
    if (!isTauri) {
      window.open(url, "_blank");
      toast(t("toast_open", { url }));
      return;
    }
    try {
      await api(`/api/open?url=${encodeURIComponent(url)}`);
      toast(t("toast_open", { url }));
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function trashView() {
    setView("trash");
    setTrashEntries([]);
    setTrashLoading(true);
    try {
      const entries = await api<TrashEntry[]>(`/api/instances/${encodeURIComponent(selected || "")}/trash`);
      setTrashEntries(entries);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    } finally {
      setTrashLoading(false);
    }
  }

  async function restoreEntry(entry: TrashEntry) {
    try {
      const result = await post<{ file: string }>("/api/mods/restore", {
        instance: selected,
        file: entry.file,
        kind: entry.kind,
      });
      toast(t("toast_restored", { file: result.file }));
      await trashView();
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function purgeEntry(entry: TrashEntry) {
    const suffix = entry.kind === "jar" ? t("purge_suffix") : "";
    if (!confirm(t("confirm_purge_entry", { file: entry.file }) + suffix)) return;
    try {
      const result = await post<{ count: number }>("/api/mods/purge", {
        instance: selected,
        file: entry.file,
        kind: entry.kind,
      });
      toast(t("toast_purged", { file: entry.file, n: result.count }));
      await trashView();
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function purgeAll() {
    if (!trashEntries.length) {
      toast(t("toast_trash_empty"));
      return;
    }
    if (!confirm(t("confirm_purge_all", { n: trashEntries.length, inst: selected || "" }))) return;
    try {
      const result = await post<{ count: number }>("/api/mods/purge_all", { instance: selected });
      toast(t("toast_trash_cleared", { n: result.count }));
      await trashView();
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function orphansView() {
    if (!selected) return;
    try {
      const files = await api<string[]>(`/api/config/orphans?${query({ instance: selected })}`);
      setOrphansModal({ instance: selected, files });
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function trashOrphans(files: string[]) {
    const result = await post<{ count: number }>("/api/config/orphans/trash", {
      instance: orphansModal?.instance,
      files,
    });
    toast(t("toast_moved_config", { n: result.count }));
    await loadMods();
  }

  async function addMod() {
    setView("add");
    setHits([]);
    setSearchLoading(false);
    try {
      const response = await api<{ items: CartItem[] }>(`/api/cart/${encodeURIComponent(selected || "")}`);
      setCart(response.items || []);
    } catch (e) {
      setCart([]);
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function searchMods(params: { source: "modrinth" | "curseforge"; q: string; mc: string; loader: string }) {
    if (!selected) return;
    setSearchLoading(true);
    setHits([]);
    setHitsSource(params.source);
    try {
      const result = await api<SearchHit[]>(
        `/api/search?${query({
          source: params.source,
          instance: selected,
          q: params.q,
          mc: params.mc,
          loader: params.loader,
        })}`
      );
      setHits(result);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    } finally {
      setSearchLoading(false);
    }
  }

  async function addCart(
    hit: SearchHit,
    source: "modrinth" | "curseforge",
    mc: string,
    loader: string,
    deps: boolean
  ) {
    try {
      const result = await post<{ duplicate: boolean }>(
        `/api/cart/${encodeURIComponent(selected || "")}/add`,
        {
          source,
          project_id: String(hit.project_id),
          slug: hit.slug || "",
          title: hit.title || "",
          desc: hit.desc || "",
          author: hit.author || "",
          downloads: hit.downloads || 0,
          icon: hit.icon || "",
          mc,
          loader,
          deps,
        }
      );
      toast(result.duplicate ? t("toast_duplicate", { t: hit.title }) : t("toast_added_cart", { t: hit.title }));
      const response = await api<{ items: CartItem[] }>(`/api/cart/${encodeURIComponent(selected || "")}`);
      setCart(response.items || []);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function removeCart(item: CartItem) {
    try {
      await post(`/api/cart/${encodeURIComponent(selected || "")}/remove`, {
        source: item.source,
        project_id: String(item.project_id),
      });
      const response = await api<{ items: CartItem[] }>(`/api/cart/${encodeURIComponent(selected || "")}`);
      setCart(response.items || []);
      toast(t("toast_removed_cart", { t: item.title }));
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function clearCart() {
    if (!cart.length) {
      toast(t("toast_cart_empty"));
      return;
    }
    if (!confirm(t("confirm_clear_cart"))) return;
    try {
      await post(`/api/cart/${encodeURIComponent(selected || "")}/clear`, {});
      setCart([]);
      toast(t("toast_cart_cleared"));
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function downloadCart() {
    if (!cart.length) {
      toast(t("toast_cart_empty"), true);
      return;
    }
    if (!confirm(t("confirm_download_cart", { n: cart.length }))) return;

    startProgress("install", `keranjang — ${cart.length} mod`, 0);
    let result: { installed: unknown[]; errors: Array<{ title: string; error: string }>; total: number; cart_remaining: number };
    try {
      result = await post<{ installed: unknown[]; errors: Array<{ title: string; error: string }>; total: number; cart_remaining: number }>(
        "/api/cart/" + encodeURIComponent(selected || "") + "/install_all",
        {}
      );
    } catch (e) {
      stopProgress();
      toast(e instanceof Error ? e.message : String(e), true);
      return;
    }

    try {
      await refreshCachedUpdates(selected!);
      await loadMods();
    } finally {
      stopProgress();
    }

    let message = t("toast_cart_result", { a: result.installed.length, b: result.total });
    if (result.errors?.length) {
      message += "\n" + t("toast_cart_fail", { n: result.errors.length }) + "\n" + result.errors.map((row) => `${row.title} — ${row.error}`).join("\n");
    }
    if (result.cart_remaining) message += "\n" + t("toast_cart_remaining", { n: result.cart_remaining });
    toast(message, Boolean(result.errors?.length));

    try {
      const response = await api<{ items: CartItem[] }>(`/api/cart/${encodeURIComponent(selected || "")}`);
      setCart(response.items || []);
    } catch {
      setCart([]);
    }
  }

  async function diffView() {
    setView("diff");
    try {
      await loadInstances();
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  async function compareInstances(a: string, b: string): Promise<DiffRow[]> {
    const result = await api<{ rows: DiffRow[] }>(
      `/api/diff?${query({ a, b })}`
    );
    return result.rows;
  }

  async function pickRoot() {
    try {
      const result = await post<{ root: string; picked: string }>("/api/root/pick", { start: root });
      toast(t("toast_root", { p: result.root }));
      await loadRoot();
      setInstances([]);
      setSelected(null);
      const list = await api<InstanceInfo[]>("/api/instances");
      setInstances(list);
      if (list.length) {
        await pick(list[0].name);
      }
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      if (message.includes("dibatalkan")) return;
      if (message.includes("unavailable")) {
      toast(t("toast_native_unavailable"));
        setRootPicker(true);
      } else {
        toast(message, true);
      }
    }
  }

  async function chooseRoot(path: string) {
    await post("/api/config/root", { path });
    toast(t("toast_root", { p: path }));
    await loadRoot();
    setInstances([]);
    setSelected(null);
    const list = await api<InstanceInfo[]>("/api/instances");
    setInstances(list);
    if (list.length) await pick(list[0].name);
  }

  async function setCfKey() {
    const key = prompt(t("cf_prompt"));
    if (key === null) return;
    try {
      const result = await post<{ curseforge: boolean }>("/api/config/curseforge", { key });
      toast(result.curseforge ? t("toast_cf_active") : t("toast_cf_removed"));
      await loadSources();
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), true);
    }
  }

  useEffect(() => {
    void loadSources();
    void loadRoot();
    void (async () => {
      try {
        const list = await api<InstanceInfo[]>("/api/instances");
        setInstances(list);
        if (list.length) await pick(list[0].name);
      } catch (e) {
        toast(e instanceof Error ? e.message : String(e), true);
      }
    })();
    return () => {
      if (pollTimer.current) clearInterval(pollTimer.current);
    };
  }, []);

  const instance = instances.find((row) => row.name === selected) || null;

  return (
    <>
      <header>
        <h1>mcman</h1>
        <span className="sub">{t("tagline")} · Modrinth</span>
        <span style={{ flex: 1 }} />
      </header>

      <div id="wrap">
        <Sidebar
          instances={instances}
          selected={selected}
          onPick={(name) => void pick(name)}
          onDiff={() => void diffView()}
          onSettings={() => setSettingsModal(true)}
        />

        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          {view === "mods" && instance ? (
            <ModsView
              instance={instance}
              mods={mods}
              updates={updates}
              selectedFiles={selectedFiles}
              loading={loadingMods}
              filter={filter}
              statusFilter={statusFilter}
              sortKey={sortKey}
              sortDir={sortDir}
              statusText={statusText}
              onFilterChange={setFilter}
              onStatusFilterChange={(value) => setStatusFilter(value)}
              onToggle={(file) => void toggle(file)}
              onTrash={(file) => void trashOne(file)}
              onOpenConfig={openConfig}
              onOpenUrl={(url) => void openUrl(url)}
              onPickVersion={setVersionModal}
              onCheckUpdates={() => void checkUpdates()}
              onUpdateAll={() => void updateAll()}
              onUpdateSelected={() => void updateSelected()}
              onTrashSelected={() => void trashSelected()}
              onClearSelection={() => setSelectedFiles(new Set())}
              onSelectAll={(checked) => {
                const visible = mods.filter((mod) => {
                  const q = filter.toLowerCase();
                  const textMatch = !q || mod.file.toLowerCase().includes(q) || mod.name.toLowerCase().includes(q);
                  const statusMatch =
                    statusFilter === "all" ||
                    (statusFilter === "enabled" && !mod.disabled) ||
                    (statusFilter === "disabled" && mod.disabled);
                  return textMatch && statusMatch;
                });
                setSelectedFiles((prev) => {
                  const next = new Set(prev);
                  visible.forEach((mod) => {
                    if (checked) next.add(mod.file);
                    else next.delete(mod.file);
                  });
                  return next;
                });
              }}
              onSelectRow={(file, checked) => {
                setSelectedFiles((prev) => {
                  const next = new Set(prev);
                  if (checked) next.add(file);
                  else next.delete(file);
                  return next;
                });
              }}
              onSort={(key) => {
                if (key === sortKey) setSortDir((dir) => (dir === 1 ? -1 : 1));
                else {
                  setSortKey(key);
                  setSortDir(key === "size" || key === "mtime" ? -1 : 1);
                }
              }}
              onAddMod={() => void addMod()}
              onTrashView={() => void trashView()}
              onOrphans={() => void orphansView()}
            />
          ) : view === "add" && instance ? (
            <AddModView
              instance={instance}
              defaultSource={cfOn ? "curseforge" : "modrinth"}
              hits={hits}
              hitsSource={hitsSource}
              loading={searchLoading}
              cart={cart}
              onSearch={searchMods}
              onAddCart={(hit, source, mc, loader, deps) => void addCart(hit, source, mc, loader, deps)}
              onRemoveCart={(item) => void removeCart(item)}
              onClearCart={() => void clearCart()}
              onDownloadCart={() => void downloadCart()}
              onOpenUrl={(url) => void openUrl(url)}
              onBack={() => void pick(selected || "")}
            />
          ) : view === "trash" && instance ? (
            <TrashView
              instance={instance}
              entries={trashEntries}
              loading={trashLoading}
              onRestore={(entry) => void restoreEntry(entry)}
              onPurge={(entry) => void purgeEntry(entry)}
              onPurgeAll={() => void purgeAll()}
              onBack={() => void pick(selected || "")}
            />
          ) : view === "diff" ? (
            <DiffView
              instances={instances}
              onBack={() => void pick(selected || instances[0]?.name || "")}
              onCompare={compareInstances}
            />
          ) : (
            <div id="main"><div className="dim">{t("loading")}</div></div>
          )}
        </div>
      </div>

      <CheckProgressOverlay job={checkProgress} />
      <ProgressOverlay job={progress} />

      <div id="msg">
        {toasts.map((item) => (
          <div key={item.id} className={`toast${item.error ? " err" : ""}`}>{item.message}</div>
        ))}
      </div>

      {configModal ? (
        <ConfigModal
          file={configModal.file}
          configs={configModal.configs}
          onOpenFile={(rel) => void openConfigFile(rel)}
          onOpenDir={(dir) => void openConfigDir(dir)}
          onClose={() => setConfigModal(null)}
        />
      ) : null}

      {versionModal && instance ? (
        <VersionModal
          instance={instance}
          file={versionModal}
          mods={mods}
          updates={updates}
          onApply={applyVersion}
          onClose={() => setVersionModal(null)}
        />
      ) : null}

      {orphansModal ? (
        <OrphansModal
          instance={orphansModal.instance}
          files={orphansModal.files}
          onTrash={trashOrphans}
          onClose={() => setOrphansModal(null)}
        />
      ) : null}

      {rootPicker ? (
        <RootPicker
          initialPath={root}
          onSelect={chooseRoot}
          onClose={() => setRootPicker(false)}
        />
      ) : null}

      {settingsModal ? (
        <SettingsModal
          onClose={() => setSettingsModal(false)}
          root={root}
          cfOn={cfOn}
          onPickRoot={() => {
            setSettingsModal(false);
            void pickRoot();
          }}
          onSetCfKey={() => void setCfKey()}
        />
      ) : null}
    </>
  );
}
