import { useState } from "react";
import { Modal } from "./Modal";
import { Button } from "./ui/button";
import { useSettings } from "../lib/i18n";

interface OrphansModalProps {
  instance: string;
  files: string[];
  onTrash: (files: string[]) => Promise<void>;
  onClose: () => void;
}

export function OrphansModal({ instance, files, onTrash, onClose }: OrphansModalProps) {
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const { t } = useSettings();

  function toggle(rel: string) {
    setPicked((prev) => {
      const next = new Set(prev);
      if (next.has(rel)) next.delete(rel);
      else next.add(rel);
      return next;
    });
  }

  async function move() {
    const list = [...picked];
    if (!list.length) return alert(t("toast_no_files"));
    if (!confirm(t("confirm_move_orphans", { n: list.length }))) return;
    setBusy(true);
    try {
      await onTrash(list);
      onClose();
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal onClose={onClose}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 10 }}>
        <div style={{ flex: 1 }}>
          <b>{t("orphans_title")}</b>{" "}
          <span className="dim">
            {t("orphans_desc", { n: files.length, inst: instance })}
          </span>
        </div>
        <Button variant="ghost" size="icon" onClick={onClose}>✕</Button>
      </div>

      {files.length === 0 ? (
        <div className="dim" style={{ marginTop: 10 }}>{t("no_orphans")}</div>
      ) : (
        <>
          <div style={{ maxHeight: "56vh", overflow: "auto", marginTop: 10 }}>
            {files.map((rel) => (
              <div className="hit" key={rel}>
                <input
                  type="checkbox"
                  checked={picked.has(rel)}
                  onChange={() => toggle(rel)}
                  style={{ flex: "none" }}
                />
                <div className="dim" style={{ flex: 1 }}>{rel}</div>
              </div>
            ))}
          </div>
          <div style={{ marginTop: 10, display: "flex", gap: 8 }}>
            <Button variant="outline" size="sm" onClick={() => setPicked(new Set(files))}>{t("select_all")}</Button>
            <Button variant="outline" size="sm" onClick={() => setPicked(new Set())}>{t("clear")}</Button>
            <Button variant="destructive" size="sm" disabled={busy} onClick={() => void move()}>{t("move_selected")}</Button>
          </div>
        </>
      )}
    </Modal>
  );
}
