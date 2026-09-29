import { localeOf, translate } from "../lib/i18n";
import { useAppStateContext } from "../lib/state";

export default function SettingsPage() {
  const { launcherSettings } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);

  return (
    <div className="page settings-page" lang={locale}>
      <h2 className="page-heading">{t("settings")}</h2>

      <div className="settings-section">
        <h3 className="settings-section-title">{t("general")}</h3>

        <div className="settings-row">
          <div className="settings-row-info">
            <label className="settings-row-label" htmlFor="launcher-language">
              {t("language")}
            </label>
            <span className="settings-row-desc">{t("languageDescription")}</span>
          </div>
          <div className="settings-row-control">
            <select
              id="launcher-language"
              className="settings-select"
              value={locale}
              onChange={(event) => launcherSettings.setLanguage(event.target.value)}
            >
              <option value="en">English</option>
              <option value="ja">日本語</option>
            </select>
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-row-info">
            <span className="settings-row-label">{t("keepLauncherOpen")}</span>
            <span className="settings-row-desc">{t("keepLauncherOpenDescription")}</span>
          </div>
          <div className="settings-row-control">
            <button
              className={`settings-toggle ${launcherSettings.keepLauncherOpen ? "on" : ""}`}
              onClick={() =>
                launcherSettings.setKeepLauncherOpen(!launcherSettings.keepLauncherOpen)
              }
              aria-label={t("keepLauncherOpen")}
              aria-pressed={launcherSettings.keepLauncherOpen}
            >
              <div className="settings-toggle-knob" />
            </button>
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-row-info">
            <span className="settings-row-label">{t("launchWithConsole")}</span>
            <span className="settings-row-desc">{t("launchWithConsoleDescription")}</span>
          </div>
          <div className="settings-row-control">
            <button
              className={`settings-toggle ${launcherSettings.launchWithConsole ? "on" : ""}`}
              onClick={() =>
                launcherSettings.setLaunchWithConsole(!launcherSettings.launchWithConsole)
              }
              aria-label={t("launchWithConsole")}
              aria-pressed={launcherSettings.launchWithConsole}
            >
              <div className="settings-toggle-knob" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
