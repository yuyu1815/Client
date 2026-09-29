import { useState } from "react";
import { localized, localeOf } from "../../lib/i18n";
import { useAppStateContext } from "../../lib/state";

export type FriendSettingsDialogProps = Record<string, never>;

export function FriendSettingsDialog(_props: FriendSettingsDialogProps) {
  const { friendsSettings, updateFriendSettings, setOpenedDialog, launcherSettings } =
    useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (en: string, ja: string) => localized(locale, en, ja);
  const [pending, setPending] = useState(false);

  const loading = friendsSettings === null;
  const settings = friendsSettings ?? { show_in_list: true, accept_invites: true };

  const apply = async (show: boolean, accept: boolean) => {
    if (loading || pending) return;
    setPending(true);
    try {
      await updateFriendSettings(show, accept);
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="dialog" lang={locale} onClick={(e) => e.stopPropagation()}>
      <h2 className="dialog-title">{t("Friend Settings", "フレンド設定")}</h2>

      <div className="dialog-fields">
        <SettingRow
          label={t("Show in Friends List", "フレンドリストに表示")}
          desc={t(
            "Other players can see you in their friends lists",
            "他のプレイヤーのフレンドリストに表示されます",
          )}
          value={settings.show_in_list}
          disabled={loading || pending}
          onToggle={() => apply(!settings.show_in_list, settings.accept_invites)}
        />
        <SettingRow
          label={t("Allow Requests", "リクエストを許可")}
          desc={t(
            "Other players can send you friend requests",
            "他のプレイヤーからフレンドリクエストを受け取ります",
          )}
          value={settings.accept_invites}
          disabled={loading || pending}
          onToggle={() => apply(settings.show_in_list, !settings.accept_invites)}
        />
      </div>

      <div className="dialog-actions">
        <button className="dialog-confirm" onClick={() => setOpenedDialog(null)}>
          {t("Close", "閉じる")}
        </button>
      </div>
    </div>
  );
}

function SettingRow({
  label,
  desc,
  value,
  disabled,
  onToggle,
}: {
  label: string;
  desc: string;
  value: boolean;
  disabled: boolean;
  onToggle: () => void;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-info">
        <span className="settings-row-label">{label}</span>
        <span className="settings-row-desc">{desc}</span>
      </div>
      <div className="settings-row-control">
        <button
          className={`settings-toggle ${value ? "on" : ""}`}
          disabled={disabled}
          onClick={onToggle}
        >
          <div className="settings-toggle-knob" />
        </button>
      </div>
    </div>
  );
}
