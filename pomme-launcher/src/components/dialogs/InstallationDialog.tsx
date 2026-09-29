import { open as openNativeDialog } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { HiChevronDown, HiFolder } from "react-icons/hi2";
import { commands } from "../../bindings";
import { Installation, InstallationError } from "../../bindings/pomme_launcher/installations";
import { isAbsolutePath, normalizeDirectoryName } from "../../lib/helpers";
import { useDropdown } from "../../lib/hooks";
import { localized, localeOf } from "../../lib/i18n";
import { useAppStateContext } from "../../lib/state";

export type InstallationDialogProps =
  | { type: "new" }
  | { type: "edit"; installation: Installation }
  | { type: "dupl"; installation: Installation; original_id: string };

function mapInstallationError(
  error: InstallationError,
  locale: "en" | "ja",
): { name?: string; dir?: string } {
  const t = (en: string, ja: string) => localized(locale, en, ja);
  switch (error.kind) {
    case "InvalidName":
      return { name: t("Invalid name", "無効な名前です") };
    case "NameTooLong":
      return {
        name: t(
          `Name too long (max ${error.detail} characters)`,
          `名前が長すぎます（最大${error.detail}文字）`,
        ),
      };
    case "InvalidPath":
      return { dir: t("Invalid path", "無効なパスです") };
    case "InvalidCharacter":
      return { dir: t(`Invalid character: ${error.detail}`, `無効な文字: ${error.detail}`) };
    case "ReservedName":
      return { dir: t(`Reserved name: ${error.detail}`, `予約された名前: ${error.detail}`) };
    case "DirectoryAlreadyExists":
      return { dir: t("Directory already exists", "ディレクトリは既に存在します") };
    case "InstallNotFound":
      return {
        dir: t(
          `Install ${error.detail} not found.`,
          `インストール「${error.detail}」が見つかりません。`,
        ),
      };
    case "Io":
      return { dir: t(`IO error: ${error.detail}`, `IOエラー: ${error.detail}`) };
    case "Json":
      return { dir: t(`JSON error: ${error.detail}`, `JSONエラー: ${error.detail}`) };
    case "Other":
      return { dir: t(`Unexpected error: ${error.detail}`, `予期しないエラー: ${error.detail}`) };
  }
}

export function InstallationDialog({ ...dialogProps }: InstallationDialogProps) {
  const {
    launcherSettings,
    versions,
    setInstallations,
    setActiveInstall,
    setPage,
    setVersions,
    setStatus,
    setDownloadProgress,
    setOpenedDialog,
  } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (en: string, ja: string) => localized(locale, en, ja);

  function createEmptyInstallation(): Installation {
    return {
      id: "",
      name: "",
      version: versions[0]?.id || "",
      last_played: null,
      directory: "",
      width: 854,
      height: 480,
      is_latest: false,
      created_at: 0,
    };
  }

  const dialogType = dialogProps.type;

  const { ref: versionDropdownRef, ...versionDropdown } = useDropdown();
  const [directoryTouched, setDirectoryTouched] = useState(dialogType === "new" ? false : true);
  const [showSnapshots, setShowSnapshots] = useState(false);

  const [nameError, setNameError] = useState<string | null>(null);
  const [dirError, setDirError] = useState<string | null>(null);
  const [versionError, setVersionError] = useState<string | null>(null);

  const [editingInstall, setEditingInstall] = useState<Installation>(() =>
    dialogType !== "new" ? { ...dialogProps.installation } : createEmptyInstallation(),
  );

  return (
    <div
      className="dialog"
      lang={locale}
      onClick={(e) => {
        e.stopPropagation();
        if (versionDropdown.isOpen) versionDropdown.close();
      }}
    >
      <h2 className="dialog-title">
        {dialogType === "edit"
          ? t("Edit Installation", "インストールを編集")
          : dialogType === "dupl"
            ? t("Duplicate Installation", "インストールを複製")
            : t("New Installation", "新規インストール")}
      </h2>

      <div className="dialog-fields">
        <div className="dialog-field">
          <label>{t("NAME", "名前")}</label>
          <input
            disabled={editingInstall.is_latest}
            value={editingInstall.name}
            onChange={(e) => {
              const name = e.target.value;
              setNameError(null);
              if (!directoryTouched) setDirError(null);
              setEditingInstall((prev) => {
                if (!prev) return prev;
                return {
                  ...prev,
                  name,
                  directory: directoryTouched ? prev.directory : normalizeDirectoryName(name),
                };
              });
            }}
            placeholder={t("My Installation", "マイインストール")}
            autoFocus
          />
          <span className={`dialog-field-info ${nameError ? "error" : ""}`}>{nameError}</span>
        </div>
        <div className="dialog-field">
          <label>{t("VERSION", "バージョン")}</label>
          <div className="custom-select-wrapper" ref={versionDropdownRef}>
            <button className="custom-select" onClick={versionDropdown.toggle} type="button">
              <span>{editingInstall.version}</span>
              <HiChevronDown
                className={`custom-select-arrow ${versionDropdown.isOpen ? "open" : ""}`}
              />
            </button>
            {versionDropdown.isOpen && (
              <div className="custom-select-dropdown">
                <label className="custom-select-toggle">
                  <input
                    type="checkbox"
                    checked={showSnapshots}
                    onChange={(e) => {
                      setShowSnapshots(e.target.checked);
                      commands.getVersions(e.target.checked).then((res) => {
                        if (res.ok) {
                          setVersions(res.value);
                        } else {
                          console.error("Failed to fetch versions: ", res.error);
                        }
                      });
                    }}
                  />
                  <span>{t("Show snapshots", "スナップショットを表示")}</span>
                </label>
                <div className="custom-select-list">
                  {versions.map((v) => (
                    <button
                      key={v.id}
                      className={`custom-select-item ${v.id === editingInstall.version ? "active" : ""}`}
                      onClick={() => {
                        setEditingInstall((prev) => ({ ...prev, version: v.id }));
                        versionDropdown.close();
                      }}
                    >
                      <span>{v.id}</span>
                      {v.version_type !== "release" && (
                        <span className="custom-select-tag">{v.version_type}</span>
                      )}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
          <span className={`dialog-field-info error`}>{versionError || ""}</span>
        </div>
        <div className="dialog-field">
          <label>{t("GAME DIRECTORY", "ゲームディレクトリ")}</label>
          <div className="dialog-browse">
            <input
              value={editingInstall.directory}
              onChange={(e) => {
                const dirname = e.target.value;
                setDirError(null);
                setDirectoryTouched(dirname !== "");
                setEditingInstall((prev) => ({ ...prev, directory: dirname }));
              }}
              placeholder="my-installation"
            />
            <button
              className="dialog-browse-btn"
              aria-label={t("Browse for directory", "ディレクトリを参照")}
              onClick={async () => {
                const path = await openNativeDialog({ directory: true });
                if (path) {
                  setDirectoryTouched(true);
                  setEditingInstall((prev) => ({ ...prev, directory: path }));
                }
              }}
            >
              <HiFolder />
            </button>
          </div>
          <span className={`dialog-field-info ${dirError ? "error" : ""}`}>
            {dirError ||
              (!isAbsolutePath(editingInstall.directory) &&
                editingInstall.directory !== normalizeDirectoryName(editingInstall.directory) &&
                t("Will be created as: ", "作成先: ") +
                  normalizeDirectoryName(editingInstall.directory || "my-installation"))}
          </span>
        </div>
        <div className="dialog-field">
          <label>{t("RESOLUTION", "解像度")}</label>
          <div className="dialog-resolution">
            <input
              type="number"
              value={editingInstall.width}
              onChange={(e) =>
                setEditingInstall((prev) => ({
                  ...prev,
                  width: parseInt(e.target.value) || 854,
                }))
              }
              placeholder="854"
            />
            <span className="dialog-resolution-x">×</span>
            <input
              type="number"
              value={editingInstall.height}
              onChange={(e) =>
                setEditingInstall((prev) => ({
                  ...prev,
                  height: parseInt(e.target.value) || 480,
                }))
              }
              placeholder="480"
            />
          </div>
        </div>
      </div>

      <div className="dialog-actions">
        <button className="dialog-cancel" onClick={() => setOpenedDialog(null)}>
          {t("Cancel", "キャンセル")}
        </button>
        <button
          className="dialog-save"
          onClick={async () => {
            const editedInstall: Installation = {
              ...editingInstall,
              name: editingInstall.name || "My Installation",
              version: editingInstall.version || versions[0]?.id || "",
              width: editingInstall.width || 854,
              height: editingInstall.height || 480,
            };
            editedInstall.directory = isAbsolutePath(editingInstall.directory)
              ? editingInstall.directory
              : normalizeDirectoryName(editingInstall.directory || editedInstall.name);

            if (editingInstall.version === "") {
              setVersionError(t("Invalid version", "無効なバージョンです"));
              return;
            }

            if (dialogType !== "edit") {
              const installResult = await (dialogType === "new"
                ? commands.createInstallation(editedInstall)
                : commands.duplicateInstallation(dialogProps.original_id, editedInstall));

              if (!installResult.ok) {
                const mapped = mapInstallationError(installResult.error, locale);
                if (mapped.name) setNameError(mapped.name);
                if (mapped.dir) setDirError(mapped.dir);
                return;
              }
              const install = installResult.value;
              setInstallations((prev) => [...prev, install]);
              setActiveInstall(install);

              setOpenedDialog(null);
              setPage("home");
              setDownloadProgress({
                downloaded: 0,
                total: 1,
                status: t("Starting install...", "インストールを開始中..."),
              });

              const ensureAssetsResult = await commands.ensureAssets(install.version);
              if (ensureAssetsResult.ok) {
                setStatus(t(`${install.name} ready`, `${install.name} 準備完了`));
              } else {
                setStatus(
                  t(
                    `Install failed: ${ensureAssetsResult.error}`,
                    `インストールに失敗しました: ${ensureAssetsResult.error}`,
                  ),
                );
              }

              setDownloadProgress(null);
              setTimeout(() => setStatus(""), 3000);
            } else {
              const editInstallResult = await commands.editInstallation(
                editingInstall.id,
                editedInstall,
              );
              if (!editInstallResult.ok) {
                const mapped = mapInstallationError(editInstallResult.error, locale);
                if (mapped.name) setNameError(mapped.name);
                if (mapped.dir) setDirError(mapped.dir);
                return;
              }
              setInstallations((prev) =>
                prev.map((i) => (i.id === editingInstall.id ? editedInstall : i)),
              );
              setActiveInstall(editedInstall);
              setOpenedDialog(null);
            }
          }}
        >
          {dialogType === "new"
            ? t("Install", "インストール")
            : dialogType === "edit"
              ? t("Save", "保存")
              : t("Duplicate", "複製")}
        </button>
      </div>
    </div>
  );
}
