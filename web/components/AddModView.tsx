import { useState } from "react";
import type { CartItem, InstanceInfo, SearchHit } from "../lib/types";
import { initials } from "../lib/format";
import { HitSkeleton } from "./Skeletons";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { useSettings } from "../lib/i18n";

interface AddModViewProps {
  instance: InstanceInfo | null;
  defaultSource: "modrinth" | "curseforge";
  hits: SearchHit[];
  hitsSource: "modrinth" | "curseforge";
  loading: boolean;
  cart: CartItem[];
  onSearch: (params: { source: "modrinth" | "curseforge"; q: string; mc: string; loader: string }) => void;
  onAddCart: (hit: SearchHit, source: "modrinth" | "curseforge", mc: string, loader: string, deps: boolean) => void;
  onRemoveCart: (item: CartItem) => void;
  onClearCart: () => void;
  onDownloadCart: () => void;
  onOpenUrl: (url: string) => void;
  onBack: () => void;
}

function cartUrl(item: CartItem): string {
  const slug = item.slug || item.project_id;
  return item.source === "curseforge"
    ? `https://www.curseforge.com/minecraft/mc-mods/${encodeURIComponent(slug)}`
    : `https://modrinth.com/mod/${encodeURIComponent(slug)}`;
}

export function AddModView(props: AddModViewProps) {
  const [source, setSource] = useState<"modrinth" | "curseforge">(props.defaultSource);
  const [loader, setLoader] = useState("");
  const [mc, setMc] = useState("");
  const [query, setQuery] = useState("");
  const [deps, setDeps] = useState(true);
  const { t } = useSettings();

  function search() {
    props.onSearch({ source, q: query, mc: mc.trim(), loader });
  }

  return (
    <div id="main">
      <div className="toolbar">
        <b style={{ fontSize: 16 }}>{t("add_mod_title")} → {props.instance?.name ?? "—"}</b>
        <span style={{ flex: 1 }} />
        <Button variant="outline" size="sm" onClick={props.onBack}>{t("back")}</Button>
      </div>

      <div className="toolbar">
        <select value={source} onChange={(event) => setSource(event.target.value as "modrinth" | "curseforge")}>
          <option value="modrinth">Modrinth</option>
          <option value="curseforge">CurseForge</option>
        </select>
        <select value={loader} onChange={(event) => setLoader(event.target.value)} title={t("override_loader")}>
          <option value="">{t("loader_auto")}</option>
          <option value="fabric">fabric</option>
          <option value="quilt">quilt</option>
          <option value="neoforge">neoforge</option>
          <option value="forge">forge</option>
          <option value="paper">paper</option>
          <option value="velocity">velocity</option>
        </select>
        <Input
          value={mc}
          onChange={(event) => setMc(event.target.value)}
          placeholder={t("mc_auto")}
          className="w-24"
          title={t("override_mc")}
        />
        <Input
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("search_placeholder")}
          className="w-80"
          onKeyDown={(event) => { if (event.key === "Enter") search(); }}
        />
        <Button size="sm" onClick={search}>{t("search")}</Button>
        <span className="dim">{props.loading ? t("searching") : t("results", { n: props.hits.length })}</span>
        <label className="depsbox">
          <input type="checkbox" checked={deps} onChange={(event) => setDeps(event.target.checked)} />
          <span>
            <b>{t("auto_deps")}</b>
            <span className="hint">{t("required_only")}</span>
          </span>
        </label>
      </div>

      <div className="pane">
        {props.loading ? (
          <HitSkeleton />
        ) : props.hits.length === 0 ? (
          <div className="dim">{t("no_results")}</div>
        ) : (
          props.hits.map((hit) => (
            <div className="hit" key={`${props.hitsSource}-${hit.project_id}`}>
              {hit.icon ? (
                <img className="hitimg" src={hit.icon} alt="" loading="lazy" />
              ) : (
                <div className="hitimg" style={{ display: "flex", alignItems: "center", justifyContent: "center", fontSize: 12, fontWeight: 700, color: "var(--dim)" }}>
                  {initials(hit.title)}
                </div>
              )}
              <div style={{ flex: 1 }}>
                <b
                  className="modtitle"
                  onClick={() => {
                    const slug = hit.slug;
                    const url = props.hitsSource === "curseforge"
                      ? `https://www.curseforge.com/minecraft/mc-mods/${encodeURIComponent(slug)}`
                      : `https://modrinth.com/mod/${encodeURIComponent(slug)}`;
                    props.onOpenUrl(url);
                  }}
                  title={t("open_mod_page_tip", { s: props.hitsSource })}
                >
                  {hit.title}
                </b>{" "}
                <span className="badge">{props.hitsSource}</span>
                <div className="dim">{hit.desc}</div>
                <div className="dim">
                  ⬇ {hit.downloads?.toLocaleString?.() ?? hit.downloads} · {hit.author || ""}
                </div>
              </div>
              <Button size="sm" onClick={() => props.onAddCart(hit, props.hitsSource, mc.trim(), loader, deps)}>
                {t("add_cart")}
              </Button>
            </div>
          ))
        )}
      </div>

      <div className="sep" />

      <div className="cartzone">
        <div className="toolbar" style={{ marginBottom: 10 }}>
          <b>{t("cart")}</b>
          <span className="badge">{props.cart.length}</span>
          <span style={{ flex: 1 }} />
          <Button variant="primary" size="sm" onClick={props.onDownloadCart} disabled={props.cart.length === 0}>
            {t("download_all")} ({props.cart.length})
          </Button>
          <Button variant="outline" size="sm" onClick={props.onClearCart} disabled={props.cart.length === 0}>{t("clear")}</Button>
        </div>

        <div className="dim" style={{ maxHeight: 180, overflow: "auto" }}>
          {props.cart.length === 0 ? (
            <div className="dim">{t("cart_empty")}</div>
          ) : (
            props.cart.map((item, index) => (
              <div className="hit" key={`${item.source}-${item.project_id}`}>
                {item.icon ? (
                  <img className="hitimg" src={item.icon} alt="" loading="lazy" />
                ) : (
                  <div className="hitimg" style={{ display: "flex", alignItems: "center", justifyContent: "center", fontSize: 12, fontWeight: 700, color: "var(--dim)" }}>
                    {initials(item.title)}
                  </div>
                )}
                <div style={{ flex: 1 }}>
                  <b className="modtitle" onClick={() => props.onOpenUrl(cartUrl(item))} title={t("open_mod_page")}>
                    {item.title}
                  </b>{" "}
                  <span className="badge">{item.source}</span>
                  <div className="dim">
                    {item.desc}
                    {item.author ? ` · ${item.author}` : ""}
                  </div>
                </div>
                <div className="acts">
                  <Button variant="ghost" size="icon" onClick={() => props.onRemoveCart(item)} title={t("remove_cart_tip")}>✕</Button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
