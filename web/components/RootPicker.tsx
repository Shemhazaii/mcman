import { useEffect, useState } from "react";
import { api, query } from "../lib/api";
import { Modal } from "./Modal";
import { Button } from "./ui/button";
import { useSettings } from "../lib/i18n";

interface BrowseResponse {
  path: string;
  parent: string | null;
  drives?: boolean;
  dirs: Array<{ name: string; path: string; hidden: boolean }>;
}

interface RootPickerProps {
  initialPath: string;
  onSelect: (path: string) => Promise<void>;
  onClose: () => void;
}

export function RootPicker({ initialPath, onSelect, onClose }: RootPickerProps) {
  const [path, setPath] = useState(initialPath);
  const [data, setData] = useState<BrowseResponse | null>(null);
  const [selected, setSelected] = useState(initialPath);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const { t } = useSettings();

  async function browse(target: string) {
    setLoading(true);
    setError(null);
    try {
      const response = await api<BrowseResponse>(`/api/browse?${query({ path: target })}`);
      setData(response);
      setPath(response.path || "");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void browse(initialPath);
  }, [initialPath]);

  async function choose() {
    if (!selected) return;
    try {
      await onSelect(selected);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  return (
    <Modal onClose={onClose} width="640px">
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <b>{t("instances_folder")}</b>
        <span style={{ flex: 1 }} />
        <Button variant="ghost" size="icon" onClick={onClose}>✕</Button>
      </div>
      <div className="dim" style={{ wordBreak: "break-all" }}>{t("path")}: {data?.path || (data?.drives ? t("drives") : "")}</div>
      <div style={{ flex: 1, minHeight: 240, overflow: "auto", border: "1px solid var(--bd)", borderRadius: 6, marginTop: 8 }}>
        {loading ? (
          <div className="dim" style={{ padding: 10 }}>{t("loading")}</div>
        ) : error ? (
          <div className="badge err" style={{ margin: 10 }}>{error}</div>
        ) : (
          <>
            {data?.parent !== null && data?.parent !== undefined ? (
              <div className="hit" style={{ cursor: "pointer" }} onClick={() => void browse(data.parent || "")}>{t("up")}</div>
            ) : null}
            {data?.dirs.map((dir) => (
              <div className="hit" key={dir.path} style={{ cursor: "pointer" }} onClick={() => { setSelected(dir.path); void browse(dir.path); }}>
                📁 {dir.name}{dir.hidden ? <span className="dim"> {t("hidden")}</span> : null}
              </div>
            ))}
            {!data?.dirs.length && !loading ? <div className="dim" style={{ padding: 10 }}>{t("no_folders")}</div> : null}
          </>
        )}
      </div>
      <div style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 10 }}>
        <span className="dim" style={{ flex: 1, wordBreak: "break-all" }}>{t("selected_label")}: {selected || t("selected_nothing")}</span>
        <Button size="sm" onClick={choose} disabled={!selected}>{t("select_folder")}</Button>
      </div>
    </Modal>
  );
}
