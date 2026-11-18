import type { InstanceInfo } from "../lib/types";
import { useSettings } from "../lib/i18n";

interface SidebarProps {
  instances: InstanceInfo[];
  selected: string | null;
  onPick: (name: string) => void;
  onDiff: () => void;
  onSettings: () => void;
}

export function Sidebar({ instances, selected, onPick, onDiff, onSettings }: SidebarProps) {
  const { t } = useSettings();
  return (
    <div id="side">
      <div className="instlist">
        {instances.map((instance) => (
          <div
            key={instance.name}
            className={`inst${instance.name === selected ? " sel" : ""}`}
            onClick={() => onPick(instance.name)}
          >
            <b>{instance.name}</b>
            <div className="meta">
              {instance.loader || "?"} {instance.mcver || ""} {instance.port ? `:${instance.port}` : ""}
            </div>
          </div>
        ))}
        <div className="inst" onClick={onDiff}>
          <b>{t("diff_instances")}</b>
        </div>
      </div>
      <button className="settings-side" onClick={onSettings}>
        {t("settings")}
      </button>
    </div>
  );
}
