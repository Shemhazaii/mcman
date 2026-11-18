import { useEffect, useState } from "react";
import { api, query } from "../lib/api";
import type { InstanceInfo, ModRow, UpdatesState } from "../lib/types";
import { Modal } from "./Modal";
import { SkBar } from "./Skeletons";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { useSettings } from "../lib/i18n";

interface VersionRow {
  id: string;
  version: string;
  filename: string;
  date: string;
  type: string;
  url: string;
}

interface VersionModalProps {
  instance: InstanceInfo;
  file: string;
  mods: ModRow[];
  updates: UpdatesState | null;
  onApply: (file: string, versionId: string, mc: string, loader: string) => Promise<void>;
  onClose: () => void;
}

export function VersionModal({ instance, file, mods, updates, onApply, onClose }: VersionModalProps) {
  const [mc, setMc] = useState("");
  const [loader, setLoader] = useState("");
  const [rows, setRows] = useState<VersionRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [applying, setApplying] = useState(false);
  const { t } = useSettings();

  async function load(keepOverrides = false) {
    setLoading(true);
    setError(null);
    try {
      const params = query({
        instance: instance.name,
        file,
        mc: keepOverrides ? mc : "",
        loader: keepOverrides ? loader : "",
      });
      const versions = await api<VersionRow[]>(`/api/mod/versions?${params}`);
      setRows(versions);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setRows([]);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load(false);
  }, [file, instance.name]);

  const current = mods.find((mod) => mod.file === file);
  const latestFile = updates?.items?.[file]?.latest_filename;

  async function installVersion(versionId: string) {
    if (!confirm(t("confirm_replace", { file }))) return;
    setApplying(true);
    try {
      await onApply(file, versionId, mc, loader);
      onClose();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      setApplying(false);
    }
  }

  return (
    <Modal onClose={onClose}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 10 }}>
        <div style={{ flex: 1 }}>
          <b>{t("versions_available")} — {file}</b>{" "}
          <span className="dim">{loading ? t("loading") : t("versions_count", { n: rows.length })}</span>
        </div>
        <Button variant="ghost" size="icon" onClick={onClose}>✕</Button>
      </div>

      <div style={{ margin: "8px 0" }}>
        <select value={loader} onChange={(event) => setLoader(event.target.value)}>
          <option value="">{t("loader_auto")}</option>
          <option value="fabric">fabric</option>
          <option value="quilt">quilt</option>
          <option value="neoforge">neoforge</option>
          <option value="forge">forge</option>
          <option value="paper">paper</option>
          <option value="velocity">velocity</option>
        </select>
        <Input
          value={mc}
          onChange={(event) => setMc(event.target.value)}
          placeholder={t("mc_auto")}
          className="w-24 ml-1"
        />
        <Button variant="outline" size="sm" className="ml-1" onClick={() => void load(true)}>{t("apply")}</Button>
        <span className="dim" style={{ marginLeft: 6 }}>{t("download_from_other")}</span>
      </div>

      {error ? <div className="badge err">{error}</div> : null}

      <div style={{ marginTop: 10 }}>
        {loading ? (
          Array.from({ length: 4 }).map((_, index) => (
            <div className="vrow" key={index}>
              <span style={{ minWidth: 150 }}><SkBar width="80px" /></span>
              <span className="dim" style={{ minWidth: 90 }}><SkBar width="55px" /></span>
              <span style={{ flex: 1 }}><SkBar width="90%" /></span>
              <SkBar width="78px" height={24} />
            </div>
          ))
        ) : rows.length === 0 ? (
          <span className="dim">{t("no_versions")}</span>
        ) : (
          rows.map((version) => {
            const installed = version.filename === file;
            const latest = latestFile === version.filename;
            return (
              <div className="vrow" key={version.id}>
                <span style={{ minWidth: 150 }}>
                  <b>{version.version}</b>
                  {version.type && version.type !== "release" ? (
                    <span className="badge plugin">{version.type}</span>
                  ) : null}
                </span>
                <span className="dim" style={{ minWidth: 90 }}>{version.date || ""}</span>
                <span className="dim" style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }} title={version.filename}>
                  {version.filename}
                </span>
                {installed ? (
                  <span className="badge">{t("installed")}</span>
                ) : latest ? (
                  <Button variant="primary" size="sm" disabled={applying} onClick={() => void installVersion(version.id)}>{t("install_latest")}</Button>
                ) : (
                  <Button size="sm" disabled={applying} onClick={() => void installVersion(version.id)}>{t("install")}</Button>
                )}
              </div>
            );
          })
        )}
      </div>

      {current ? <div className="dim" style={{ marginTop: 8 }}>{t("current_version")}: {current.version}</div> : null}
    </Modal>
  );
}
