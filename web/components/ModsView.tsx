import type { ReactNode } from "react";
import type { InstanceInfo, ModRow, UpdatesState } from "../lib/types";
import { formatBytes, formatDate, initials } from "../lib/format";
import { ModSkeletonRows } from "./Skeletons";
import { Button } from "./ui/button";
import { Checkbox } from "./ui/checkbox";
import { Switch } from "./ui/switch";
import { Input } from "./ui/input";
import { useSettings } from "../lib/i18n";

type SortKey = "name" | "version" | "size" | "mtime" | "status";
type StatusFilter = "all" | "enabled" | "disabled";

interface ModsViewProps {
  instance: InstanceInfo | null;
  mods: ModRow[];
  updates: UpdatesState | null;
  selectedFiles: Set<string>;
  loading: boolean;
  filter: string;
  statusFilter: StatusFilter;
  sortKey: SortKey;
  sortDir: 1 | -1;
  statusText: string;
  onFilterChange: (value: string) => void;
  onStatusFilterChange: (value: StatusFilter) => void;
  onToggle: (file: string) => void;
  onTrash: (file: string) => void;
  onOpenConfig: (file: string) => void;
  onOpenUrl: (url: string) => void;
  onPickVersion: (file: string) => void;
  onCheckUpdates: () => void;
  onUpdateAll: () => void;
  onUpdateSelected: () => void;
  onTrashSelected: () => void;
  onClearSelection: () => void;
  onSelectAll: (checked: boolean) => void;
  onSelectRow: (file: string, checked: boolean) => void;
  onSort: (key: SortKey) => void;
  onAddMod: () => void;
  onTrashView: () => void;
  onOrphans: () => void;
}

function arrow(key: SortKey, sortKey: SortKey, sortDir: 1 | -1): string {
  if (key !== sortKey) return "";
  return sortDir > 0 ? " ▲" : " ▼";
}

function naturalCompare(a: string, b: string): number {
  return a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });
}

const COLGROUP = (
  <colgroup>
    <col style={{ width: "40px" }} />
    <col style={{ width: "50px" }} />
    <col style={{ width: "70px" }} />
    <col />
    <col style={{ width: "33%" }} />
    <col style={{ width: "78px" }} />
    <col style={{ width: "86px" }} />
    <col style={{ width: "98px" }} />
    <col style={{ width: "96px" }} />
  </colgroup>
);

function HeaderRow({
  sortKey,
  sortDir,
  onSort,
  onSelectAll,
  allChecked,
}: {
  sortKey: SortKey;
  sortDir: 1 | -1;
  onSort: (key: SortKey) => void;
  onSelectAll: (checked: boolean) => void;
  allChecked: boolean;
}) {
  const { t } = useSettings();
  return (
    <tr>
      <th>
        <Checkbox
          checked={allChecked}
          onCheckedChange={onSelectAll}
          title={t("select_all_title")}
        />
      </th>
      <th />
      <th />
      <th className="s" onClick={() => onSort("name")}>{t("col_mod")}{arrow("name", sortKey, sortDir)}</th>
      <th className="s vh" onClick={() => onSort("version")}>{t("col_version")}{arrow("version", sortKey, sortDir)}</th>
      <th className="s" onClick={() => onSort("status")}>{t("col_status")}{arrow("status", sortKey, sortDir)}</th>
      <th className="s" onClick={() => onSort("size")}>{t("col_size")}{arrow("size", sortKey, sortDir)}</th>
      <th className="s" onClick={() => onSort("mtime")}>{t("col_modified")}{arrow("mtime", sortKey, sortDir)}</th>
      <th />
    </tr>
  );
}

export function ModsView(props: ModsViewProps) {
  const { instance, mods, updates, selectedFiles, loading, filter, statusFilter, sortKey, sortDir } = props;
  const { t } = useSettings();

  const visible = mods.filter((mod) => {
    const q = filter.toLowerCase();
    const textMatch = !q || mod.file.toLowerCase().includes(q) || mod.name.toLowerCase().includes(q);
    const statusMatch =
      statusFilter === "all" ||
      (statusFilter === "enabled" && !mod.disabled) ||
      (statusFilter === "disabled" && mod.disabled);
    return textMatch && statusMatch;
  });

  const hasUpdate = (mod: ModRow) => Boolean(updates?.items?.[mod.file]?.update);

  const rows = [...visible].sort((a, b) => {
    if (sortKey === "status") return (Number(a.disabled) - Number(b.disabled)) * sortDir;
    if (updates) {
      const diff = Number(hasUpdate(b)) - Number(hasUpdate(a));
      if (diff) return diff;
    }
    if (sortKey === "version") {
      const ua = hasUpdate(a) ? updates?.items?.[a.file]?.update ?? "" : a.version || "";
      const ub = hasUpdate(b) ? updates?.items?.[b.file]?.update ?? "" : b.version || "";
      return naturalCompare(ua, ub) * sortDir;
    }
    if (sortKey === "size") return (a.size - b.size) * sortDir;
    if (sortKey === "mtime") return (a.mtime - b.mtime) * sortDir;
    return naturalCompare(a.name || a.file, b.name || b.file) * sortDir;
  });

  const allChecked = rows.length > 0 && rows.every((mod) => selectedFiles.has(mod.file));
  const updateCount = updates ? Object.values(updates.items || {}).filter((item) => item.update).length : 0;
  const selectedUpdateCount = updates
    ? [...selectedFiles].filter((file) => updates.items?.[file]?.update).length
    : 0;

  return (
    <div id="main">
      <div className="toolbar">
        <b style={{ fontSize: 16 }}>{instance?.name ?? "—"}</b>
        <span className="badge">{instance?.loader || "?"} {instance?.mcver || ""}</span>
        <span className="badge">{t("files_count", { n: mods.length })}</span>
        <span style={{ flex: 1 }} />
        <Input
          type="search"
          placeholder={t("filter_placeholder")}
          value={filter}
          onChange={(event) => props.onFilterChange(event.target.value)}
          className="w-40"
        />
        <Button variant="outline" size="sm" onClick={props.onCheckUpdates}>{t("check_updates")}</Button>
        {updateCount > 0 ? (
          <Button variant="accent" size="sm" className="btn-accent" onClick={props.onUpdateAll}>{t("update_all")} ({updateCount})</Button>
        ) : null}
        <span className="dim">{props.statusText}</span>
        <span id="selbar" style={{ display: "contents" }}>
          {selectedFiles.size > 0 ? (
            <>
              <span className="badge">
                {t("selected_count", { n: selectedFiles.size })}
                {updates ? t("updatable", { n: selectedUpdateCount }) : ""}
              </span>
              <Button variant="accent" size="sm" className="btn-accent" onClick={props.onUpdateSelected}>
                {t("update_selected")}{updates ? ` (${selectedUpdateCount})` : ""}
              </Button>
              <Button variant="destructive" size="sm" onClick={props.onTrashSelected}>
                {t("trash_selected")} ({selectedFiles.size})
              </Button>
              <Button variant="ghost" size="sm" onClick={props.onClearSelection}>✕</Button>
            </>
          ) : null}
        </span>
        <Button size="sm" onClick={props.onAddMod}>{t("add_mod")}</Button>
        <Button size="sm" onClick={props.onTrashView}>{t("trash")}</Button>
        <Button size="sm" onClick={props.onOrphans}>{t("orphans")}</Button>
      </div>

      <div className="paneTopPad" />
      <div className="pane">
        <div className="modshead">
          <table className="hdr">
            {COLGROUP}
            <thead>
              <HeaderRow
                sortKey={sortKey}
                sortDir={sortDir}
                onSort={props.onSort}
                onSelectAll={props.onSelectAll}
                allChecked={allChecked}
              />
            </thead>
          </table>
        </div>
        <table className="mods">
          {COLGROUP}
          <tbody>
            {loading ? (
              <ModSkeletonRows />
            ) : rows.length === 0 ? (
              <tr><td colSpan={9} className="dim">{t("no_mods")}</td></tr>
            ) : (
              rows.map((mod) => {
                const update = updates?.items?.[mod.file];
                let versionBadges: ReactNode = <span className="badge">{mod.version || "—"}</span>;
                if (update?.update) {
                  versionBadges = (
                    <>
                      {versionBadges}
                      <span
                        className="badge upd"
                        title={t("update_tip")}
                        onClick={() => props.onPickVersion(mod.file)}
                      >
                        → {update.update} ⬆
                      </span>
                    </>
                  );
                } else if (update?.known && update.latest && update.latest === update.installed) {
                  versionBadges = (
                    <>
                      {versionBadges}
                      <span className="dim" style={{ fontSize: 11 }}> {t("latest_badge")}</span>
                    </>
                  );
                }
                if (update?.blocked) {
                  versionBadges = (
                    <>
                      {versionBadges}
                      <span className="badge block" title={t("blocked_tip")}>
                        {t("blocked_badge")}
                      </span>
                    </>
                  );
                }
                if (!update || update.known) {
                  versionBadges = (
                    <>
                      {versionBadges}
                      <span
                        className="badge"
                        title={t("version_picker_tip")}
                        style={{ cursor: "pointer" }}
                        onClick={() => props.onPickVersion(mod.file)}
                      >
                        {t("version_picker")}
                      </span>
                    </>
                  );
                }
                if (update?.source === "curseforge") {
                  versionBadges = (
                    <>
                      {versionBadges}
                      <span className="badge plugin" title="source: CurseForge">CF</span>
                    </>
                  );
                }

                return (
                  <tr key={mod.file} className={mod.disabled ? "off" : ""}>
                    <td>
                      <Checkbox
                        checked={selectedFiles.has(mod.file)}
                        onCheckedChange={(checked) => props.onSelectRow(mod.file, checked)}
                      />
                    </td>
                    <td>
                      <Switch
                        checked={!mod.disabled}
                        onCheckedChange={() => props.onToggle(mod.file)}
                      />
                    </td>
                    <td>
                      {mod.icon ? (
                        <img className="mic" src={mod.icon} alt="" loading="lazy" />
                      ) : (
                        <div className="mic ini">{initials(mod.name)}</div>
                      )}
                    </td>
                    <td>
                      <div className="name">
                        {mod.url ? (
                          <b
                            className="modtitle"
                            onClick={() => props.onOpenUrl(mod.url!)}
                            title={t("open_mod_tip")}
                          >
                            {mod.name}
                          </b>
                        ) : (
                          <b>{mod.name}</b>
                        )}{" "}
                        {mod.kind === "plugin" ? <span className="badge plugin">{t("plugin")}</span> : null}
                        {mod.dep_for ? (
                          <span className="badge" title={t("dep_title", { p: mod.dep_for })}>+dep</span>
                        ) : null}
                      </div>
                      <div className="file">{mod.file}</div>
                    </td>
                    <td className="v"><div className="vwrap">{versionBadges}</div></td>
                    <td>
                      <span className={`badge ${mod.disabled ? "offstat" : "onstat"}`}>
                        {mod.disabled ? t("disabled") : t("enabled")}
                      </span>
                    </td>
                    <td className="dim">{formatBytes(mod.size)}</td>
                    <td className="dim">{formatDate(mod.mtime)}</td>
                    <td>
                      <div className="acts">
                        <Button variant="destructive" size="icon" onClick={() => props.onTrash(mod.file)}>🗑</Button>
                        {mod.configs && mod.configs.length > 0 ? (
                          <Button
                            variant="outline"
                            size="icon"
                            onClick={() => props.onOpenConfig(mod.file)}
                            title={t("config_tip", { n: mod.configs.length })}
                          >
                            ⚙
                          </Button>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
