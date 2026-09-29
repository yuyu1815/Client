import { useState } from "react";
import { HiChevronDown } from "react-icons/hi2";
import { useDropdown } from "../../lib/hooks";
import { localized, localeOf } from "../../lib/i18n";
import { shouldSubmitEnter } from "../../lib/shouldSubmitEnter.mjs";
import { useAppStateContext } from "../../lib/state";
import { Server } from "../../lib/types";

const UNCATEGORIZED = "Uncategorized";

type ServerCategoryInputProps = {
  category: string;
  setCategory: (category: string) => void;
  existingCategories: string[];
  customCategory: boolean;
  setCustomCategory: (custom: boolean) => void;
};

function ServerCategoryInput({
  category,
  setCategory,
  existingCategories,
  customCategory,
  setCustomCategory,
}: ServerCategoryInputProps) {
  const { ref: categoryDropdownRef, ...categoryDropdown } = useDropdown();
  const { launcherSettings } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (en: string, ja: string) => localized(locale, en, ja);

  return (
    <div className="dialog-field">
      <label>{t("CATEGORY", "カテゴリー")}</label>
      <div className="custom-select-wrapper" ref={categoryDropdownRef}>
        <div className="creatable-select">
          {customCategory ? (
            <>
              <input
                className="creatable-select-input"
                placeholder={t("New category name", "新しいカテゴリー名")}
                value={category}
                onChange={(e) => setCategory(e.target.value)}
                autoFocus
              />
              <button className="creatable-select-toggle" onClick={() => categoryDropdown.toggle()}>
                <HiChevronDown
                  className={`custom-select-arrow ${categoryDropdown.isOpen ? "open" : ""}`}
                />
              </button>
            </>
          ) : (
            <button
              className="creatable-select-selected"
              onClick={categoryDropdown.toggle}
              type="button"
            >
              <span>{category === UNCATEGORIZED ? t(UNCATEGORIZED, "未分類") : category}</span>
              <HiChevronDown
                className={`custom-select-arrow ${categoryDropdown.isOpen ? "open" : ""}`}
              />
            </button>
          )}
        </div>
        {categoryDropdown.isOpen && (
          <div className="custom-select-dropdown">
            <div className="custom-select-list">
              <button
                key={UNCATEGORIZED}
                className={`custom-select-item ${category === UNCATEGORIZED ? "active" : ""}`}
                onClick={() => {
                  setCustomCategory(false);
                  setCategory(UNCATEGORIZED);
                  categoryDropdown.close();
                }}
              >
                <span>{t(UNCATEGORIZED, "未分類")}</span>
              </button>
              {existingCategories.map((cat) => (
                <button
                  key={cat}
                  className={`custom-select-item ${category === cat ? "active" : ""}`}
                  onClick={() => {
                    setCustomCategory(false);
                    setCategory(cat);
                    categoryDropdown.close();
                  }}
                >
                  <span>{cat}</span>
                </button>
              ))}
              <button
                className="custom-select-item"
                onClick={() => {
                  setCustomCategory(true);
                  setCategory("");
                  categoryDropdown.close();
                }}
              >
                <span>+ {t("New category", "新しいカテゴリー")}</span>
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export type ServerDialogProps = { type: "new" } | { type: "edit"; server: Server };

export function ServerDialog(dialogProps: ServerDialogProps) {
  const { servers, addServer, editServer, setOpenedDialog, launcherSettings } =
    useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (en: string, ja: string) => localized(locale, en, ja);

  const [serverName, setServerName] = useState(
    dialogProps.type === "edit" ? dialogProps.server.name : "",
  );
  const [serverAddress, setServerAddress] = useState(
    dialogProps.type === "edit" ? dialogProps.server.ip : "",
  );
  const [category, setCategory] = useState(
    dialogProps.type === "edit" && !!dialogProps.server.category
      ? dialogProps.server.category
      : UNCATEGORIZED,
  );
  const [customCategory, setCustomCategory] = useState(false);

  const existingCategories = [...new Set(servers.map((s) => s.category).filter((c) => c))];

  const handleConfirm = () => {
    if (!serverAddress.trim()) return;

    const name = serverName.trim() || serverAddress.trim();
    const ip = serverAddress.trim();
    const cat = category.trim() === UNCATEGORIZED ? "" : category.trim();

    if (dialogProps.type === "new") {
      addServer(name, ip, cat);
    } else {
      editServer(dialogProps.server.id, name, ip, cat);
    }

    setOpenedDialog(null);
  };

  return (
    <div
      className="dialog"
      lang={locale}
      onClick={(e) => {
        e.stopPropagation();
      }}
    >
      <h2 className="dialog-title">
        {dialogProps.type === "edit"
          ? t("Edit Server", "サーバーを編集")
          : t("Add Server", "サーバーを追加")}
      </h2>

      <div className="dialog-fields">
        <div className="dialog-field">
          <label>{t("SERVER NAME", "サーバー名")}</label>
          <input
            value={serverName}
            onChange={(e) => setServerName(e.target.value)}
            placeholder={t("My Server", "マイサーバー")}
            autoFocus
          />
        </div>

        <div className="dialog-field">
          <label>{t("SERVER ADDRESS", "サーバーアドレス")}</label>
          <input
            value={serverAddress}
            onChange={(e) => setServerAddress(e.target.value)}
            placeholder="play.example.com"
            onKeyDown={(e) => shouldSubmitEnter(e) && handleConfirm()}
          />
        </div>

        <ServerCategoryInput
          category={category}
          setCategory={setCategory}
          customCategory={customCategory}
          setCustomCategory={setCustomCategory}
          existingCategories={existingCategories}
        />
      </div>

      <div className="dialog-actions">
        <button className="dialog-cancel" onClick={() => setOpenedDialog(null)}>
          {t("Cancel", "キャンセル")}
        </button>
        <button className="dialog-save" onClick={handleConfirm}>
          {dialogProps.type === "edit" ? t("Save", "保存") : t("Add", "追加")}
        </button>
      </div>
    </div>
  );
}
