import { HiListBullet, HiMagnifyingGlass, HiPuzzlePiece, HiSquares2X2 } from "react-icons/hi2";
import { localized, localeOf } from "../lib/i18n";
import { useAppStateContext } from "../lib/state";

export default function ModsPage() {
  const {
    modFilter,
    modSearch,
    setModSearch,
    setModFilter,
    modView,
    setModView,
    launcherSettings,
  } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (en: string, ja: string) => localized(locale, en, ja);

  const modDescriptionJa: Record<string, string> = {
    "Mod 1": "より高いフレームレートを実現するレンダリングエンジンの最適化",
    "Mod 2": "ダイナミックライティングとビジュアルの強化",
    "Mod 3": "ポストプロセス効果のためのシェーダーパックローダー",
    "Mod 4": "建築物の貼り付けや移動に使える設計図ツール",
    "Mod 5": "ウェイポイントとミニマップを備えたリアルタイムマップ",
    "Mod 6": "新しいバイオーム、モブ、ワールド生成を追加",
    "Mod 7": "インベントリの整理と管理ツール",
    "Mod 8": "ボリュームクラウドと大気効果",
  };

  const mods = [
    {
      name: "Mod 1",
      cat: "performance",
      desc: "Rendering engine optimization for better frame rates",
      version: "0.6.1",
      downloads: "38M",
      installed: true,
    },
    {
      name: "Mod 2",
      cat: "performance",
      desc: "Dynamic lighting and visual enhancement",
      version: "1.21.11",
      downloads: "142M",
      installed: false,
    },
    {
      name: "Mod 3",
      cat: "shaders",
      desc: "Shader pack loader for post-processing effects",
      version: "1.8.0",
      downloads: "25M",
      installed: false,
    },
    {
      name: "Mod 4",
      cat: "utility",
      desc: "Schematic building tools for pasting and moving structures",
      version: "0.19.0",
      downloads: "18M",
      installed: false,
    },
    {
      name: "Mod 5",
      cat: "utility",
      desc: "Real-time mapping with waypoints and minimap",
      version: "6.0.0",
      downloads: "52M",
      installed: true,
    },
    {
      name: "Mod 6",
      cat: "gameplay",
      desc: "Adds new biomes, creatures, and world generation",
      version: "2.3.0",
      downloads: "12M",
      installed: false,
    },
    {
      name: "Mod 7",
      cat: "utility",
      desc: "Inventory sorting and management tools",
      version: "1.4.2",
      downloads: "8M",
      installed: false,
    },
    {
      name: "Mod 8",
      cat: "shaders",
      desc: "Volumetric clouds and atmospheric effects",
      version: "3.1.0",
      downloads: "15M",
      installed: false,
    },
  ];
  const filtered = mods.filter(
    (m) =>
      (modFilter === "all" || m.cat === modFilter) &&
      m.name.toLowerCase().includes(modSearch.toLowerCase()),
  );
  return (
    <div className="page mock-page" lang={locale}>
      <div className="mock-banner">
        {t("This is a preview - functionality coming soon", "プレビューです - 機能は近日公開予定")}
      </div>
      <h2 className="page-heading">{t("MODS", "MOD")}</h2>
      <div className="mods-toolbar">
        <div className="mods-search">
          <HiMagnifyingGlass className="mods-search-icon" />
          <input
            className="mods-search-input"
            placeholder={t("Search mods...", "MODを検索...")}
            value={modSearch}
            onChange={(e) => setModSearch(e.target.value)}
          />
        </div>
        <div className="mods-filters">
          {["all", "performance", "shaders", "utility", "gameplay"].map((f) => (
            <button
              key={f}
              className={`mods-filter ${modFilter === f ? "active" : ""}`}
              onClick={() => setModFilter(f)}
            >
              {f === "all"
                ? t("All", "すべて")
                : f === "performance"
                  ? t("Performance", "パフォーマンス")
                  : f === "shaders"
                    ? t("Shaders", "シェーダー")
                    : f === "utility"
                      ? t("Utility", "ユーティリティ")
                      : t("Gameplay", "ゲームプレイ")}
            </button>
          ))}
        </div>
        <div className="mods-view-toggle">
          <button
            className={`mods-view-btn ${modView === "list" ? "active" : ""}`}
            aria-label={t("List view", "リスト表示")}
            onClick={() => setModView("list")}
          >
            <HiListBullet aria-hidden="true" />
          </button>
          <button
            className={`mods-view-btn ${modView === "grid" ? "active" : ""}`}
            aria-label={t("Grid view", "グリッド表示")}
            onClick={() => setModView("grid")}
          >
            <HiSquares2X2 aria-hidden="true" />
          </button>
        </div>
      </div>
      <div className={modView === "grid" ? "mods-grid" : "mock-list"}>
        {filtered.map((m) => (
          <div className={modView === "grid" ? "mock-mod-card" : "mock-mod"} key={m.name}>
            <div className="mock-mod-icon">
              <HiPuzzlePiece />
            </div>
            <div className="mock-mod-info">
              <span className="mock-mod-name">{m.name}</span>
              <span className="mock-mod-desc">{t(m.desc, modDescriptionJa[m.name] ?? m.desc)}</span>
              <div className="mock-mod-meta">
                <span>{m.version}</span>
                <span>
                  {m.downloads} {t("downloads", "ダウンロード")}
                </span>
              </div>
            </div>
            <button className={`mock-mod-btn ${m.installed ? "installed" : ""}`}>
              {m.installed ? t("Installed", "インストール済み") : t("Install", "インストール")}
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
