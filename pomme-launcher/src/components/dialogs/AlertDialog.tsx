import { useState } from "react";
import { localized, localeOf } from "../../lib/i18n";
import { useAppStateContext } from "../../lib/state";

export type AlertDialogProps = {
  title: string;
  message: string;
  onClose?: () => void | Promise<void>;
};

export default function AlertDialog(dialogProps: AlertDialogProps) {
  const { setOpenedDialog, launcherSettings } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const [loading, setLoading] = useState(false);

  return (
    <div className="dialog" lang={locale} onClick={(e) => e.stopPropagation()}>
      <h2 className="dialog-title">{dialogProps.title}</h2>
      <div className="dialog-fields">
        <p className="dialog-text">{dialogProps.message}</p>
      </div>
      <div className="dialog-actions">
        <button
          className="dialog-confirm"
          disabled={loading}
          onClick={async () => {
            if (loading) return;
            setLoading(true);
            try {
              await dialogProps.onClose?.();
            } catch (e) {
              console.error(e);
            } finally {
              setLoading(false);
              setOpenedDialog(null);
            }
          }}
        >
          {loading ? "..." : localized(locale, "OK", "OK")}
        </button>
      </div>
    </div>
  );
}
