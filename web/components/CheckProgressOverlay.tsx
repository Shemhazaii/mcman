import type { CheckProgressState } from "../lib/types";
import { useSettings } from "../lib/i18n";

export function CheckProgressOverlay({ job }: { job: CheckProgressState | null }) {
  const { t } = useSettings();
  if (!job) return null;

  const total = Number(job.total) || 0;
  const done = Number(job.done) || 0;
  const finished = job.status === "done";
  const pct = total > 0 ? Math.min(100, (done / total) * 100) : null;
  const stage = t(`phase_${job.phase}`) || job.phase;

  return (
    <div className="dlpop">
      <h4>{t("check_updates")} — {job.instance}</h4>
      <div className="file">
        {finished ? t("check_done") : stage}
        {job.cur && !finished ? ` · ${job.cur}` : ""}
      </div>
      <div className="bar">
        <i
          className={pct === null && !finished ? "ind" : ""}
          style={{ width: `${finished ? 100 : pct === null ? 40 : pct.toFixed(1)}%` }}
        />
      </div>
      <div className="dim">
        {finished ? "100%" : pct === null ? "?" : `${pct.toFixed(1)}%`}
        {total ? ` · ${done}/${total}` : ""}
      </div>
    </div>
  );
}
