import { Modal } from "./Modal";
import { Button } from "./ui/button";
import { useSettings } from "../lib/i18n";

interface SettingsModalProps {
  onClose: () => void;
  root: string;
  cfOn: boolean;
  onPickRoot: () => void;
  onSetCfKey: () => void;
}

export function SettingsModal({ onClose, root, cfOn, onPickRoot, onSetCfKey }: SettingsModalProps) {
  const { lang, theme, setLang, setTheme, t } = useSettings();

  return (
    <Modal onClose={onClose} width="420px">
      <div style={{ display: "flex", alignItems: "flex-start", gap: 10 }}>
        <b>{t("settings_title")}</b>
        <span style={{ flex: 1 }} />
        <Button variant="ghost" size="icon" onClick={onClose}>✕</Button>
      </div>

      <div className="grid2">
        <div className="card">
          <div className="dim" style={{ marginBottom: 6 }}>{t("language_label")}</div>
          <select value={lang} onChange={(event) => setLang(event.target.value as "id" | "en")}>
            <option value="id">Indonesia</option>
            <option value="en">English</option>
          </select>
        </div>
        <div className="card">
          <div className="dim" style={{ marginBottom: 6 }}>{t("theme_label")}</div>
          <select value={theme} onChange={(event) => setTheme(event.target.value as "dark" | "light" | "system")}>
            <option value="dark">{t("theme_dark")}</option>
            <option value="light">{t("theme_light")}</option>
            <option value="system">{t("theme_system")}</option>
          </select>
        </div>
      </div>

      <div className="grid2" style={{ marginTop: 10 }}>
        <div className="card">
          <div className="dim" style={{ marginBottom: 6 }}>{t("settings_root_label")}</div>
          <div className="dim" style={{ wordBreak: "break-all", marginBottom: 8 }}>
            {root || t("no_root")}
          </div>
          <Button variant="outline" size="sm" onClick={onPickRoot}>{t("change_folder")}</Button>
        </div>
        <div className="card">
          <div className="dim" style={{ marginBottom: 6 }}>{t("settings_api_label")}</div>
          <div className="dim" style={{ marginBottom: 8 }}>
            {cfOn ? t("api_key_active") : t("api_key_missing")}
          </div>
          <Button variant="outline" size="sm" onClick={onSetCfKey}>{t("cf_set_key")}</Button>
        </div>
      </div>

      <div style={{ marginTop: 12, display: "flex", justifyContent: "flex-end" }}>
        <Button variant="outline" size="sm" onClick={onClose}>{t("close")}</Button>
      </div>
    </Modal>
  );
}
