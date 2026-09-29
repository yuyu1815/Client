import { openUrl } from "@tauri-apps/plugin-opener";
import { HiArrowLeft, HiArrowTopRightOnSquare } from "react-icons/hi2";
import { PatchNote } from "../bindings/pomme_launcher/commands";
import { localized, localeOf } from "../lib/i18n";
import { useAppStateContext } from "../lib/state";

const MORE_PATCH_NOTES_URL = "https://aka.ms/MorePatchNotes";

export default function NewsPage({
  openPatchNote,
}: {
  openPatchNote: (note: PatchNote) => Promise<void>;
}) {
  const { selectedNote, setSelectedNote, news, launcherSettings } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (en: string, ja: string) => localized(locale, en, ja);

  return (
    <div className="page news-page" lang={locale}>
      {selectedNote ? (
        <div className="note-viewer">
          <button className="note-back" onClick={() => setSelectedNote(null)}>
            <HiArrowLeft /> {t("Back", "戻る")}
          </button>
          <div className="note-header-banner">
            <div className="note-header-img-wrap">
              <img
                src={selectedNote.image_url}
                alt={selectedNote.title}
                lang="en"
                className="note-header-img"
              />
            </div>
            <div className="note-header-content">
              <div className="note-header-meta">
                <span className="note-header-date">{selectedNote.date?.replace(/-/g, ".")}</span>
                <span className="note-header-meta-divider" />
                <span className="note-header-type" lang="en">
                  {selectedNote.entry_type}
                </span>
              </div>
              <h2 className="note-header-title" lang="en">
                {selectedNote.title}
              </h2>
            </div>
          </div>
          <div
            className="note-body"
            lang="en"
            dangerouslySetInnerHTML={{ __html: selectedNote.body }}
          />
        </div>
      ) : (
        <>
          <h2 className="page-heading">{t("NEWS & UPDATES", "ニュースと更新")}</h2>
          <div className="news-grid-full">
            {news.map((item) => (
              <div
                className="news-card-wide"
                key={item.version}
                onClick={() => openPatchNote(item)}
              >
                <div className="news-card-img-wide">
                  <img src={item.image_url} alt={item.title} lang="en" />
                  <span className="news-type-badge" lang="en">
                    {item.entry_type}
                  </span>
                </div>
                <div className="news-card-body-wide">
                  <div className="news-card-meta-wide">
                    <span className="news-date">{item.date.replace(/-/g, ".")}</span>
                    <span className="news-card-arrow">→</span>
                  </div>
                  <h3 className="news-title" lang="en">
                    {item.title}
                  </h3>
                  <hr className="news-rule" />
                  <p className="news-desc-full" lang="en">
                    {item.summary}
                  </p>
                  <span className="news-version">{item.version}</span>
                </div>
              </div>
            ))}
            {news.length === 0 && (
              <p className="news-loading">
                {t("Loading patch notes...", "パッチノートを読み込み中...")}
              </p>
            )}
          </div>

          <a
            className="news-more-link"
            href={MORE_PATCH_NOTES_URL}
            onClick={(e) => {
              e.preventDefault();
              openUrl(MORE_PATCH_NOTES_URL);
            }}
          >
            {t("More patch notes", "その他のパッチノート")} <HiArrowTopRightOnSquare />
          </a>
        </>
      )}
    </div>
  );
}
