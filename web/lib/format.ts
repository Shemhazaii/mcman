export function formatBytes(n: number): string {
  if (n >= 1048576) return `${(n / 1048576).toFixed(1)} MB`;
  return `${(n / 1024).toFixed(0)} KB`;
}

export function formatDate(t: number): string {
  return new Date(t * 1000).toISOString().slice(0, 10);
}

export function initials(name: string): string {
  return (name || "?").slice(0, 2).toUpperCase();
}

export const PHASE_LABEL: Record<string, string> = {
  preparing: "menyiapkan metadata",
  resolving: "mencari versi untuk loader/MC",
  download: "mengunduh jar",
  dependencies: "mengunduh dependencies",
  installing: "memasang jar / memindahkan jar lama ke trash",
  swapping: "memindahkan jar lama ke trash",
  cache: "menulis data update",
  done: "selesai — memuat ulang mod list…",
  error: "gagal",
};
