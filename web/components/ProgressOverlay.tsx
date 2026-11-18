import type { ProgressJob } from "../lib/types";
import { formatBytes } from "../lib/format";
import { useSettings } from "../lib/i18n";

function barWidth(job: ProgressJob): number {
  const total = Number(job.bytes_total) || 0;
  const done = Number(job.bytes_done) || 0;
  if (job.status === "done") return 100;
  if (total > 0) return Math.min(100, (done / total) * 100);
  return 40;
}

export function ProgressOverlay({ job }: { job: ProgressJob | null }) {
  const { t } = useSettings();
  if (!job) return null;
  const finished = job.status === "done";
  const total = Number(job.bytes_total) || 0;
  const done = Number(job.bytes_done) || 0;
  const pct = total > 0 ? Math.min(100, (done / total) * 100) : null;
  const fileCount = job.files_total ? `${job.files_done}/${job.files_total}` : "";
  const stage = t(`phase_${job.phase}`) || job.phase;

  return (
    <div className="dlpop">
      <h4>{job.kind === "install" ? t("install_new") : t("update")} — {job.instance}</h4>
      <div className="dim">{job.label}</div>
      <div className="file">
        {finished ? t("done_reload") : stage}
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
        {total ? ` · ${formatBytes(done)} / ${formatBytes(total)}` : ""}
        {fileCount ? ` · file ${fileCount}` : ""}
      </div>
      {job.error ? <div style={{ color: "var(--red)", marginTop: 6 }}>{job.error}</div> : null}
    </div>
  );
}
