import { Modal } from "./Modal";
import { Button } from "./ui/button";
import { useSettings } from "../lib/i18n";

interface ConfigModalProps {
  file: string;
  configs: string[];
  onOpenFile: (rel: string) => void;
  onOpenDir: (dir: string) => void;
  onClose: () => void;
}

export function ConfigModal({ file, configs, onOpenFile, onOpenDir, onClose }: ConfigModalProps) {
  const nestedDir = configs.find((rel) => rel.includes("/"))?.split("/")[0] ?? "";
  const { t } = useSettings();

  return (
    <Modal onClose={onClose}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 10 }}>
        <div style={{ flex: 1 }}>
          <b>{t("config_title")}</b> <span className="dim">{t("config_count", { n: configs.length })}</span>
        </div>
        <Button variant="ghost" size="icon" onClick={onClose}>✕</Button>
      </div>
      <div className="dim" style={{ margin: "4px 0 10px" }}>{file}</div>
      <Button variant="outline" size="sm" onClick={() => onOpenDir(nestedDir)}>{t("open_config_folder")}</Button>
      <div style={{ maxHeight: "56vh", overflow: "auto", marginTop: 10 }}>
        {configs.map((rel) => (
          <div className="hit" key={rel}>
            <div className="dim" style={{ flex: 1 }}>{rel}</div>
            <Button variant="outline" size="sm" onClick={() => onOpenFile(rel)}>{t("open")}</Button>
          </div>
        ))}
      </div>
    </Modal>
  );
}
