import { BiSolidDownload } from "react-icons/bi";
import { HiChevronDown, HiCube, HiPlay } from "react-icons/hi2";
import { PatchNote } from "../bindings/pomme_launcher/commands";
import SkinRunner from "../components/SkinRunner";
import { useDropdown } from "../lib/hooks";
import { localeOf, localized } from "../lib/i18n";
import { useAppStateContext } from "../lib/state";
import { handleLaunchType } from "../lib/types";

interface HomepageProps {
  handleLaunch: handleLaunchType;
  openPatchNote: (item: PatchNote) => Promise<void>;
}

export default function Homepage({ handleLaunch, openPatchNote }: HomepageProps) {
  const {
    launchingStatus,
    installations,
    activeInstall,
    setActiveInstall,
    news,
    status,
    downloadedVersions,
    downloadProgress,
    skinUrl,
    setOpenedDialog,
    launcherSettings,
  } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);

  const { ref: versionDropdownRef, ...versionDropdown } = useDropdown();

  return (
    <div className="page home-page" lang={locale}>
      <div className="hero-banner">
        <div className="hero-overlay" />
        <div className="hero-content">
          <h1 className="hero-title">POMME</h1>
          <p className="hero-subtitle">
            {localized(locale, "RUST-NATIVE MINECRAFT CLIENT", "Rust製Minecraftクライアント")}
          </p>
        </div>
      </div>

      <div className="launch-bar">
        <button
          className={`play-button ${
            launchingStatus === "installing" || launchingStatus === "checking_assets"
              ? "installing"
              : launchingStatus === "launching"
                ? "launching"
                : ""
          }`}
          onClick={() => handleLaunch()}
          disabled={launchingStatus !== null}
        >
          {launchingStatus === null && downloadedVersions.has(activeInstall?.version ?? "") ? (
            <HiPlay className="play-icon" />
          ) : (
            <BiSolidDownload className="download-icon" />
          )}
          <span className="play-text">
            {launchingStatus === null
              ? downloadedVersions.has(activeInstall?.version ?? "")
                ? localized(locale, "PLAY", "プレイ")
                : localized(locale, "INSTALL", "インストール")
              : launchingStatus === "checking_assets"
                ? localized(locale, "Checking assets...", "アセットを確認中...")
                : launchingStatus === "installing"
                  ? localized(locale, "Installing...", "インストール中...")
                  : localized(locale, "Launching...", "起動中...")}
          </span>
        </button>
      </div>

      <div className="version-badge-wrapper" ref={versionDropdownRef}>
        <button className="version-badge" onClick={versionDropdown.toggle}>
          <HiCube className="version-badge-icon" />
          <span className="version-item-id">
            {activeInstall?.name ||
              localized(locale, "No installation selected", "インストールが選択されていません")}
          </span>
          <span className="version-item-type" hidden={!activeInstall}>
            {activeInstall?.version || ""}
          </span>
          <HiChevronDown
            className={`version-badge-arrow ${versionDropdown.isOpen ? "open" : ""}`}
          />
        </button>
        {versionDropdown.isOpen && (
          <div className="version-dropdown">
            <div className="version-list">
              {installations.length === 0 ? (
                <button
                  className={`version-item`}
                  onClick={() => {
                    versionDropdown.close();
                    setOpenedDialog({ name: "installation_dialog", props: { type: "new" } });
                  }}
                >
                  <span className="version-item-id">
                    {localized(locale, "Create a new installation", "新しいインストールを作成")}
                  </span>
                </button>
              ) : (
                installations.map((inst) => (
                  <button
                    key={inst.id}
                    className={`version-item ${inst.id === activeInstall?.id ? "active" : ""}`}
                    onClick={() => {
                      setActiveInstall(inst);
                      versionDropdown.close();
                    }}
                  >
                    <span className="version-item-id">{inst.name}</span>
                    <span className="version-item-type">{inst.version}</span>
                  </button>
                ))
              )}
            </div>
          </div>
        )}
      </div>

      {downloadProgress && (
        <div className="download-progress">
          <div className="download-progress-text">{downloadProgress.status}</div>
          <div className="download-progress-bar">
            <SkinRunner
              skinUrl={skinUrl}
              progress={
                downloadProgress.total > 0
                  ? downloadProgress.downloaded / downloadProgress.total
                  : 0
              }
            />
            <div
              className="download-progress-fill"
              style={{
                width:
                  downloadProgress.total > 0
                    ? `${(downloadProgress.downloaded / downloadProgress.total) * 100}%`
                    : "0%",
              }}
            />
          </div>
        </div>
      )}
      {!downloadProgress && status && <div className="status-toast">{status}</div>}

      <div className="news-section">
        <h2 className="news-heading">{localized(locale, "LATEST NEWS", "最新ニュース")}</h2>
        <div className="news-grid">
          {news.slice(0, 3).map((item) => (
            <div className="news-card" key={item.version} onClick={() => openPatchNote(item)}>
              <div className="news-card-img">
                <img src={item.image_url} alt={item.title} lang="en" className="news-card-img-bg" />
                <span className="news-type-badge" lang="en">
                  {item.entry_type}
                </span>
              </div>

              <div className="news-card-body">
                <div className="news-card-meta">
                  <span className="news-date">{item.date.replace(/-/g, ".")}</span>
                  <span className="news-card-arrow">→</span>
                </div>

                <h3 className="news-title" lang="en">
                  {item.title}
                </h3>
                <hr className="news-rule" />
                <p className="news-desc" lang="en">
                  {item.summary}
                </p>
              </div>
            </div>
          ))}
          {news.length === 0 && (
            <p className="news-loading">
              {localized(locale, "Loading patch notes...", "パッチノートを読み込み中...")}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
