import { useState } from "react";
import type { DiffRow, InstanceInfo } from "../lib/types";
import { SkBar } from "./Skeletons";
import { Button } from "./ui/button";
import { useSettings } from "../lib/i18n";

interface DiffViewProps {
  instances: InstanceInfo[];
  onBack: () => void;
  onCompare: (a: string, b: string) => Promise<DiffRow[]>;
}

export function DiffView({ instances, onBack, onCompare }: DiffViewProps) {
  const [a, setA] = useState(instances[0]?.name ?? "");
  const [b, setB] = useState(instances[1]?.name ?? instances[0]?.name ?? "");
  const [rows, setRows] = useState<DiffRow[] | null>(null);
  const [loading, setLoading] = useState(false);
  const { t } = useSettings();

  async function compare() {
    setLoading(true);
    setRows(null);
    try {
      setRows(await onCompare(a, b));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div id="main">
      <div className="toolbar">
        <b style={{ fontSize: 16 }}>{t("diff_instances")}</b>
        <select value={a} onChange={(event) => setA(event.target.value)}>
          {instances.map((instance) => <option key={instance.name} value={instance.name}>{instance.name}</option>)}
        </select>
        <span className="dim">vs</span>
        <select value={b} onChange={(event) => setB(event.target.value)}>
          {instances.map((instance) => <option key={instance.name} value={instance.name}>{instance.name}</option>)}
        </select>
        <Button size="sm" onClick={compare}>{t("compare")}</Button>
        <span style={{ flex: 1 }} />
        <Button variant="outline" size="sm" onClick={onBack}>{t("back")}</Button>
      </div>

      <div className="pane">
        {loading ? (
          <table>
            <thead><tr><th>{t("col_mod")}</th><th>{t("col_type")}</th><th>{a}</th><th>{b}</th></tr></thead>
            <tbody>
              {Array.from({ length: 6 }).map((_, index) => (
                <tr key={index}>
                  <td><SkBar width="70%" /></td>
                  <td><SkBar width="82px" /></td>
                  <td className="dim"><SkBar width="48px" /></td>
                  <td className="dim"><SkBar width="48px" /></td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : rows ? (
          <>
            <div className="dim">{t("differences", { n: rows.length })}</div>
            <table>
              <thead><tr><th>{t("col_mod")}</th><th>{t("col_type")}</th><th>{a}</th><th>{b}</th></tr></thead>
              <tbody>
                {rows.map((row, index) => (
                  <tr key={`${row.name}-${index}`}>
                    <td>{row.name}</td>
                    <td>
                      {row.type === "only_a" ? (
                        <span className="badge err">{t("only_a", { a })}</span>
                      ) : row.type === "only_b" ? (
                        <span className="badge plugin">{t("only_b", { b })}</span>
                      ) : (
                        <span className="badge upd">{t("version_differs")}</span>
                      )}
                    </td>
                    <td>{row.a || "—"}{row.ad ? " " + t("off") : ""}</td>
                    <td>{row.b || "—"}{row.bd ? " " + t("off") : ""}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        ) : (
          <div className="dim">pilih dua instance lalu Compare</div>
        )}
      </div>
    </div>
  );
}
