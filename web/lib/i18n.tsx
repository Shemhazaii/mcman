"use client";

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type Lang = "id" | "en";
export type ThemeMode = "dark" | "light" | "system";

type Entry = { id: string; en: string };

const DICT: Record<string, Entry> = {
  diff_instances: { id: "⇄ Diff instance", en: "⇄ Diff instances" },
  check_updates: { id: "Cek update", en: "Check updates" },
  update_all: { id: "⬆ Update semua", en: "⬆ Update all" },
  selected_count: { id: "{n} dipilih", en: "{n} selected" },
  updatable: { id: "· {n} bisa diupdate", en: "· {n} updatable" },
  update_selected: { id: "⬆ Update dipilih", en: "⬆ Update selected" },
  trash_selected: { id: "🗑 Hapus dipilih", en: "🗑 Delete selected" },
  add_mod: { id: "+ Tambah mod", en: "+ Add mod" },
  trash: { id: "Trash", en: "Trash" },
  orphans: { id: "🧹 config tanpa pemilik", en: "🧹 configs with no owner" },
  col_mod: { id: "Mod", en: "Mod" },
  col_version: { id: "Versi", en: "Version" },
  col_status: { id: "Status", en: "Status" },
  col_size: { id: "Ukuran", en: "Size" },
  col_modified: { id: "Diubah", en: "Modified" },
  col_type: { id: "Tipe", en: "Type" },
  col_file: { id: "File", en: "File" },
  col_kind: { id: "Jenis", en: "Kind" },
  col_deleted: { id: "Dihapus", en: "Deleted" },
  select_all_title: { id: "pilih semua mod yang terlihat", en: "select all visible mods" },
  no_mods: { id: "tidak ada mod", en: "no mods" },
  latest_badge: { id: "✓ terbaru", en: "✓ latest" },
  version_picker: { id: "versi ▾", en: "version ▾" },
  update_tip: { id: "update — klik untuk pilih versi", en: "update — click to pick a version" },
  blocked_tip: {
    id: "penulis mod memblokir akses launcher pihak ketiga",
    en: "mod author blocks third-party launcher access",
  },
  blocked_badge: { id: "⚠ diblokir", en: "⚠ blocked" },
  version_picker_tip: { id: "lihat / pasang versi spesifik", en: "view / install a specific version" },
  open_mod_tip: { id: "klik untuk buka halaman mod", en: "click to open the mod page" },
  dep_title: { id: "deps otomatis dari {p}", en: "auto dependency of {p}" },
  plugin: { id: "plugin", en: "plugin" },
  enabled: { id: "Aktif", en: "Enabled" },
  disabled: { id: "Nonaktif", en: "Disabled" },
  config_tip: { id: "buka file config mod ({n} file)", en: "open mod config file ({n} files)" },
  no_root: { id: "(root belum diatur)", en: "(no folder set)" },
  change_folder: { id: "ganti 📁", en: "change 📁" },
  tagline: { id: "Manajer mod server Minecraft", en: "Minecraft server mod manager" },
  cf_set_key: { id: "🔑 atur key", en: "🔑 set key" },
  settings: { id: "⚙ Pengaturan", en: "⚙ Settings" },
  back: { id: "← kembali", en: "← back" },
  compare: { id: "Bandingkan", en: "Compare" },
  differences: { id: "{n} perbedaan", en: "{n} differences" },
  only_a: { id: "hanya {a}", en: "only {a}" },
  only_b: { id: "hanya {b}", en: "only {b}" },
  version_differs: { id: "versi berbeda", en: "version differs" },
  off: { id: "(nonaktif)", en: "(off)" },
  add_mod_title: { id: "Tambah mod", en: "Add mod" },
  loader_auto: { id: "loader: otomatis", en: "loader: auto" },
  mc_auto: { id: "MC: otomatis", en: "MC: auto" },
  search_placeholder: { id: "cari mod…", en: "search mod…" },
  search: { id: "Cari", en: "Search" },
  searching: { id: "mencari…", en: "searching…" },
  results: { id: "{n} hasil", en: "{n} results" },
  auto_deps: { id: "unduh dependency otomatis", en: "auto-download dependencies" },
  required_only: { id: "hanya dependency wajib (required)", en: "required dependencies only" },
  no_results: { id: "belum ada hasil — cari mod dulu", en: "no results yet — search a mod first" },
  open_mod_page_tip: { id: "klik untuk buka halaman mod di {s}", en: "click to open the mod page on {s}" },
  add_cart: { id: "＋ Keranjang", en: "＋ Cart" },
  cart: { id: "Keranjang", en: "Cart" },
  download_all: { id: "⬇ Download semua", en: "⬇ Download all" },
  clear: { id: "Kosongkan", en: "Clear" },
  cart_empty: {
    id: "keranjang kosong — tambah mod dari hasil search",
    en: "cart empty — add mods from search results",
  },
  remove_cart_tip: { id: "hapus dari keranjang", en: "remove from cart" },
  open_mod_page: { id: "buka halaman mod", en: "open mod page" },
  trash_title: { id: "Trash", en: "Trash" },
  item_count: { id: "{n} item", en: "{n} item" },
  delete_all: { id: "Hapus semua", en: "Delete all" },
  empty: { id: "kosong", en: "empty" },
  restore: { id: "Pulihkan", en: "Restore" },
  delete: { id: "Hapus", en: "Delete" },
  versions_available: { id: "Versi tersedia", en: "Available versions" },
  versions_count: { id: "{n} versi (terbaru dulu)", en: "{n} versions (newest first)" },
  apply: { id: "terapkan", en: "apply" },
  download_from_other: {
    id: "download dari loader/versi MC lain",
    en: "download from another loader/MC version",
  },
  no_versions: {
    id: "tidak ada versi yang cocok untuk loader/MC ini",
    en: "no matching version for this loader/MC",
  },
  installed: { id: "installed", en: "installed" },
  install_latest: { id: "install ⬆", en: "install ⬆" },
  install: { id: "install", en: "install" },
  current_version: { id: "current", en: "current" },
  confirm_replace: { id: "Ganti {file}\nJar lama masuk trash.", en: "Replace {file}\nThe old jar goes to trash." },
  config_title: { id: "Config", en: "Config" },
  config_count: { id: "{n} file · klik untuk buka", en: "{n} files · click to open" },
  open_config_folder: { id: "📁 buka folder config", en: "📁 open config folder" },
  open: { id: "buka", en: "open" },
  orphans_title: { id: "Config tanpa pemilik", en: "Configs with no owner" },
  orphans_desc: {
    id: "{n} file tidak terhubung ke mod mana pun di {inst} (bisa jadi config global/loader, atau mod yang id-nya tidak terbaca)",
    en: "{n} files are not linked to any mod in {inst} (may be global/loader configs, or mods whose id cannot be read)",
  },
  no_orphans: { id: "tidak ada config yatim", en: "no orphan configs" },
  select_all: { id: "Pilih semua", en: "Select all" },
  move_selected: { id: "Pindah terpilih ke trash", en: "Move selected to trash" },
  instances_folder: { id: "📁 Instances folder", en: "📁 Instances folder" },
  drives: { id: "(drives)", en: "(drives)" },
  loading: { id: "memuat…", en: "loading…" },
  up: { id: "⬆ .. (up)", en: "⬆ .. (up)" },
  hidden: { id: "(hidden)", en: "(hidden)" },
  no_folders: { id: "no folders here", en: "no folders here" },
  selected_label: { id: "selected", en: "selected" },
  selected_nothing: { id: "(nothing)", en: "(nothing)" },
  select_folder: { id: "Pilih folder ini", en: "Select this folder" },
  install_new: { id: "Instalasi mod baru", en: "New mod install" },
  update: { id: "Update", en: "Update" },
  done_reload: { id: "✓ selesai — memuat ulang mod list…", en: "✓ done — reloading mod list…" },
  phase_preparing: { id: "menyiapkan metadata", en: "preparing metadata" },
  phase_resolving: { id: "mencari versi untuk loader/MC", en: "resolving version for loader/MC" },
  phase_download: { id: "mengunduh jar", en: "downloading jar" },
  phase_dependencies: { id: "mengunduh dependencies", en: "downloading dependencies" },
  phase_installing: {
    id: "memasang jar / memindahkan jar lama ke trash",
    en: "installing jar / moving old jar to trash",
  },
  phase_swapping: { id: "memindahkan jar lama ke trash", en: "moving old jar to trash" },
  phase_cache: { id: "menulis data update", en: "writing update data" },
  phase_done: { id: "selesai", en: "done" },
  phase_error: { id: "gagal", en: "error" },

  toast_root: { id: "root → {p}", en: "root → {p}" },
  toast_native_unavailable: {
    id: "native picker tidak tersedia — pakai browser bawaan",
    en: "native picker unavailable — using built-in browser",
  },
  toast_cf_active: {
    id: "CurseForge aktif ✓ — jalankan ulang Check updates buat match mod CF-only",
    en: "CurseForge active ✓ — re-run Check updates to match CF-only mods",
  },
  toast_cf_removed: { id: "Key CurseForge dihapus", en: "CurseForge key removed" },
  toast_moved_config: { id: "Dipindah ke trash: {n} config", en: "Moved to trash: {n} configs" },
  toast_added_cart: { id: "Masuk keranjang: {t}", en: "Added to cart: {t}" },
  toast_duplicate: { id: "Sudah ada di keranjang: {t}", en: "Already in cart: {t}" },
  toast_removed_cart: { id: "Dihapus dari keranjang: {t}", en: "Removed from cart: {t}" },
  toast_cart_empty: { id: "Keranjang kosong", en: "Cart empty" },
  toast_cart_cleared: { id: "Keranjang dikosongkan", en: "Cart cleared" },
  confirm_clear_cart: { id: "Kosongkan keranjang?", en: "Clear the cart?" },
  confirm_download_cart: {
    id: "Download {n} mod dari keranjang? Jar lama masuk trash.",
    en: "Download {n} mods from the cart? Old jars go to trash.",
  },
  toast_cart_result: { id: "Keranjang: {a}/{b} mod terpasang", en: "Cart: {a}/{b} mods installed" },
  toast_cart_fail: { id: "{n} gagal:", en: "{n} failed:" },
  toast_cart_remaining: { id: "{n} item gagal tetap di keranjang", en: "{n} item(s) failed and stay in the cart" },
  toast_open: { id: "Membuka {url}", en: "Opening {url}" },
  toast_restored: { id: "Restored: {file}", en: "Restored: {file}" },
  toast_purged: { id: "Purged: {file} ({n} item)", en: "Deleted: {file} ({n} item)" },
  toast_trash_empty: { id: "Trash kosong", en: "Trash empty" },
  confirm_purge_entry: {
    id: "Hapus permanen {file}? Tidak bisa di-restore",
    en: "Permanently delete {file}? Cannot be restored",
  },
  confirm_purge_all: {
    id: "Hapus permanen SEMUA {n} item di trash {inst}? (jar + config, tidak bisa di-restore)",
    en: "Permanently delete ALL {n} items in trash {inst}? (jars + configs, cannot be restored)",
  },
  toast_trash_cleared: {
    id: "Trash dibersihkan: {n} item dihapus permanen",
    en: "Trash cleared: {n} items permanently deleted",
  },
  toast_no_files: { id: "Tidak ada file dipilih", en: "No files selected" },
  confirm_move_orphans: {
    id: "Pindah {n} config ke trash? (bisa di-restore dari Trash)",
    en: "Move {n} configs to trash? (restorable from Trash)",
  },
  settings_title: { id: "Pengaturan", en: "Settings" },
  settings_root_label: { id: "Folder instances", en: "Instances folder" },
  settings_api_label: { id: "CurseForge API key", en: "CurseForge API key" },
  api_key_active: { id: "Aktif ✓", en: "Active ✓" },
  api_key_missing: { id: "Belum diatur", en: "Not set" },
  status_checking: { id: "memeriksa…", en: "checking…" },
  status_check_failed: { id: "Update check gagal: {e}", en: "Update check failed: {e}" },
  status_summary: { id: "{u} update · {time}", en: "{u} update(s) · {time}" },
  status_unknown: { id: " · {n} tidak dikenal", en: " · {n} unknown" },
  confirm_trash_one: { id: "Pindah ke trash: {file}", en: "Move to trash: {file}" },
  toast_trashed: { id: "Ditrash: {file}", en: "Trashed: {file}" },
  toast_trashed_configs: { id: " · {n} config ikut ke trash", en: " · {n} configs moved to trash" },
  toast_no_selected_mods: { id: "Tidak ada mod terpilih", en: "No mods selected" },
  confirm_trash_selected: {
    id: "Pindah {n} mod ke trash? Config miliknya ikut ke trash (bisa di-restore).",
    en: "Move {n} mods to trash? Their owned configs move too (restorable).",
  },
  toast_trashed_bulk: { id: "Ditrash: {n} mod", en: "Trashed: {n} mods" },
  toast_configs_moved: { id: " · {n} config ikut", en: " · {n} configs moved" },
  toast_failed_count: { id: " · {n} gagal", en: " · {n} failed" },
  toast_run_check: { id: "Jalankan Check updates dulu", en: "Run Check updates first" },
  toast_no_update: { id: "Tidak ada update", en: "No updates" },
  confirm_update_all: {
    id: "Update {n} mod sekaligus?\nJar lama dipindah ke trash (bisa di-restore).",
    en: "Update {n} mods at once?\nOld jars go to trash (restorable).",
  },
  status_updating: { id: "updating {done}/{total}… {cur}", en: "updating {done}/{total}… {cur}" },
  toast_update_all_ok: { id: "Update all: {ok} mod berhasil ✓", en: "Update all: {ok} mods updated ✓" },
  toast_update_all_fail: { id: "Update all: {ok} ok, {n} gagal:", en: "Update all: {ok} ok, {n} failed:" },
  toast_update_sel_ok: { id: "Update terpilih: {ok} mod berhasil ✓", en: "Update selected: {ok} mods updated ✓" },
  toast_update_sel_fail: { id: "Update terpilih: {ok} ok, {n} gagal:", en: "Update selected: {ok} ok, {n} failed:" },
  toast_no_updatable: { id: "Tidak ada mod terpilih yang punya update", en: "No selected mods have an update" },
  confirm_update_selected: {
    id: "Update {n} mod terpilih? Jar lama masuk trash (bisa di-restore).",
    en: "Update {n} selected mods? Old jars go to trash (restorable).",
  },
  toast_installed: { id: "terpasang: {file}", en: "installed: {file}" },
  toast_no_config: { id: "tidak ada file config untuk {file}", en: "no config file for {file}" },
  toast_open_config: { id: "Membuka config {rel}", en: "Opening config {rel}" },
  toast_open_folder: { id: "Membuka folder {opened}", en: "Opening folder {opened}" },
  cf_prompt: {
    id: "CurseForge API key — buat gratis di https://console.curseforge.com/?#/api-keys (login, menu API keys, Regenerate/Create).\nKosongkan untuk menghapus key.",
    en: "CurseForge API key — create one free at https://console.curseforge.com/?#/api-keys (login, API keys menu, Regenerate/Create).\nLeave empty to remove the key.",
  },
  filter_placeholder: { id: "filter…", en: "filter…" },
  files_count: { id: "{n} file", en: "{n} files" },
  override_loader: { id: "override loader", en: "override loader" },
  override_mc: { id: "override versi MC", en: "override MC version" },
  purge_suffix: { id: " — config yang ikut juga dihapus", en: " — its configs are also deleted" },
  language_label: { id: "Bahasa", en: "Language" },
  theme_label: { id: "Tema", en: "Theme" },
  theme_dark: { id: "Gelap", en: "Dark" },
  theme_light: { id: "Terang", en: "Light" },
  theme_system: { id: "Ikut sistem", en: "Follow system" },
  close: { id: "Tutup", en: "Close" },
};

interface SettingsContextValue {
  lang: Lang;
  theme: ThemeMode;
  setLang: (value: Lang) => void;
  setTheme: (value: ThemeMode) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

function readStored(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) || fallback;
  } catch {
    return fallback;
  }
}

function applyTheme(mode: ThemeMode) {
  const root = document.documentElement;
  if (mode === "system") {
    root.dataset.theme = window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  } else {
    root.dataset.theme = mode;
  }
}

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [lang, setLang] = useState<Lang>("id");
  const [theme, setTheme] = useState<ThemeMode>("dark");

  useEffect(() => {
    const storedLang = readStored("mcman-lang", "id") as Lang;
    const storedTheme = readStored("mcman-theme", "dark") as ThemeMode;
    if (storedLang !== lang) setLang(storedLang);
    if (storedTheme !== theme) setTheme(storedTheme);
  }, []);

  useEffect(() => {
    applyTheme(theme);
    try {
      localStorage.setItem("mcman-lang", lang);
      localStorage.setItem("mcman-theme", theme);
    } catch {}
  }, [lang, theme]);

  useEffect(() => {
    if (theme !== "system") return;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const onChange = () => applyTheme("system");
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [theme]);

  function t(key: string, params?: Record<string, string | number>): string {
    const entry = DICT[key];
    let text = entry ? entry[lang] : key;
    if (params) {
      for (const [name, value] of Object.entries(params)) {
        text = text.replaceAll(`{${name}}`, String(value));
      }
    }
    return text;
  }

  return (
    <SettingsContext.Provider value={{ lang, theme, setLang, setTheme, t }}>
      {children}
    </SettingsContext.Provider>
  );
}

export function useSettings(): SettingsContextValue {
  const value = useContext(SettingsContext);
  if (!value) throw new Error("useSettings must be used inside SettingsProvider");
  return value;
}
