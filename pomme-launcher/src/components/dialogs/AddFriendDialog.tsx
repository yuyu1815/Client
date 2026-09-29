import { useState } from "react";
import { localized, localeOf } from "../../lib/i18n";
import { shouldSubmitEnter } from "../../lib/shouldSubmitEnter.mjs";
import { useAppStateContext } from "../../lib/state";

export type AddFriendDialogProps = {
  onSubmit: (name: string) => Promise<void>;
};

export function AddFriendDialog(dialogProps: AddFriendDialogProps) {
  const { setOpenedDialog, launcherSettings } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (en: string, ja: string) => localized(locale, en, ja);
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async () => {
    const trimmed = name.trim();
    if (!trimmed || loading) return;
    setLoading(true);
    try {
      await dialogProps.onSubmit(trimmed);
      setOpenedDialog(null);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="dialog" lang={locale} onClick={(e) => e.stopPropagation()}>
      <h2 className="dialog-title">{t("Add Friend", "フレンドを追加")}</h2>

      <div className="dialog-fields">
        <div className="dialog-field">
          <label>{t("JAVA PROFILE NAME", "Javaプロフィール名")}</label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => shouldSubmitEnter(e) && handleSubmit()}
            placeholder={t("Notch", "Notch")}
            autoFocus
          />
        </div>
      </div>

      <div className="dialog-actions">
        <button className="dialog-cancel" disabled={loading} onClick={() => setOpenedDialog(null)}>
          {t("Cancel", "キャンセル")}
        </button>
        <button className="dialog-save" disabled={loading} onClick={handleSubmit}>
          {loading ? "..." : t("Send Request", "リクエストを送信")}
        </button>
      </div>
    </div>
  );
}
