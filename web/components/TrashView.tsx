import type { InstanceInfo, TrashEntry } from "../lib/types";
import { formatBytes, formatDate } from "../lib/format";
import { SkBar } from "./Skeletons";
import { Button } from "./ui/button";
import { useSettings } from "../lib/i18n";

interface TrashViewProps {
  instance: InstanceInfo | null;
  entries: TrashEntry[];
  loading: boolean;
  onRestore: (entry: TrashEntry) => void;
  onPurge: (entry: TrashEntry) => void;
  onPurgeAll: () => void;
  onBack: () => void;
}

export function TrashView(props: TrashViewProps) {
  const { t } = useSettings();
  return (
    <div id="main">
      <div className="toolbar">
        <b style={{ fontSize: 16 }}>{t("trash_title")} — {props.instance?.name ?? "—"}</b>
        <span className="badge">{t("item_count", { n: props.entries.length })}</span>
        <span style={{ flex: 1 }} />
        <Button variant="destructive" size="sm" onClick={props.onPurgeAll} disabled={!props.entries.length}>
          {t("delete_all")} ({props.entries.length})
        </Button>
        <Button variant="outline" size="sm" onClick={props.onBack}>{t("back")}</Button>
      </div>

      <div className="pane">
        <table>
          <thead>
            <tr>
              <th>{t("col_file")}</th>
              <th>{t("col_kind")}</th>
              <th>{t("col_size")}</th>
              <th>{t("col_deleted")}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {props.loading ? (
              Array.from({ length: 6 }).map((_, index) => (
                <tr key={index}>
                  <td><SkBar width="60%" /></td>
                  <td><SkBar width="52px" /></td>
                  <td className="dim"><SkBar width="46px" /></td>
                  <td className="dim"><SkBar width="68px" /></td>
                  <td />
                </tr>
              ))
            ) : props.entries.length === 0 ? (
              <tr><td colSpan={5} className="dim">{t("empty")}</td></tr>
            ) : (
              props.entries.map((entry) => (
                <tr key={`${entry.kind}-${entry.file}`}>
                  <td>{entry.file}</td>
                  <td><span className="badge">{entry.kind}</span></td>
                  <td className="dim">{formatBytes(entry.size)}</td>
                  <td className="dim">{formatDate(entry.mtime)}</td>
                  <td>
                    <div className="acts">
                      <Button variant="outline" size="sm" onClick={() => props.onRestore(entry)}>{t("restore")}</Button>
                      <Button variant="destructive" size="sm" onClick={() => props.onPurge(entry)}>{t("delete")}</Button>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
