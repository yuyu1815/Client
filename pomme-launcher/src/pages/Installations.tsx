import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect } from "react";
import { BiSolidDownload } from "react-icons/bi";
import {
  HiCube,
  HiDocumentDuplicate,
  HiFolder,
  HiPencil,
  HiPlay,
  HiPlus,
  HiTrash,
} from "react-icons/hi2";
import { commands } from "../bindings";
import { formatRelativeDate } from "../lib/helpers";
import { localeOf, localized } from "../lib/i18n";
import { useAppStateContext } from "../lib/state";
import type { handleLaunchType } from "../lib/types";

interface InstallationsPageProps {
  handleLaunch: handleLaunchType;
}

export default function InstallationsPage({ handleLaunch }: InstallationsPageProps) {
  const {
    activeInstall,
    setActiveInstall,
    installations,
    setInstallations,
    setPage,
    setOpenedDialog,
    downloadedVersions,
    launcherSettings,
  } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);

  useEffect(() => {
    const interval = setInterval(() => {
      setInstallations((prev) => [...prev]);
    }, 60000);

    return () => clearInterval(interval);
  }, [setInstallations]);

  return (
    <div className="page installs-page" lang={locale}>
      <div className="installs-header">
        <h2 className="page-heading">{localized(locale, "INSTALLATIONS", "インストール")}</h2>
        <button
          className="installs-new-btn"
          onClick={() => {
            setOpenedDialog({ name: "installation_dialog", props: { type: "new" } });
          }}
        >
          <HiPlus /> {localized(locale, "New Installation", "新しいインストール")}
        </button>
      </div>

      <div className="installs-list">
        {installations.map((inst) => (
          <div
            key={inst.id}
            className={`install-card ${inst.id === activeInstall?.id ? "active" : ""}`}
            onClick={() => {
              setActiveInstall(inst);
            }}
          >
            <div className="install-card-icon">
              <HiCube />
            </div>
            <div className="install-card-info">
              <span className="install-card-name">{inst.name}</span>
              <span className="install-card-version">{inst.version}</span>
            </div>
            <span className="install-card-played">
              {inst.last_played
                ? formatRelativeDate(inst.last_played)
                : localized(locale, "Never", "なし")}
            </span>
            <button
              className="install-play-btn"
              onClick={() => {
                setActiveInstall(inst);
                setPage("home");
                handleLaunch({ install: inst });
              }}
            >
              {downloadedVersions.has(inst.version) ? (
                <>
                  <HiPlay /> {localized(locale, "Play", "プレイ")}
                </>
              ) : (
                <>
                  <BiSolidDownload /> {localized(locale, "Install", "インストール")}
                </>
              )}
            </button>
            <button
              className="install-folder-btn"
              onClick={async () => {
                await revealItemInDir(inst.directory);
              }}
            >
              <HiFolder />
            </button>
            <div className="install-card-actions">
              <button
                className="install-action-btn"
                onClick={() => {
                  setOpenedDialog({
                    name: "installation_dialog",
                    props: { type: "edit", installation: { ...inst } },
                  });
                }}
                title={localized(locale, "Edit", "編集")}
              >
                <HiPencil />
              </button>

              <button
                className="install-action-btn"
                title={localized(locale, "Duplicate", "複製")}
                onClick={() => {
                  const dup = {
                    ...inst,
                    id: "",
                    name: `${inst.name} (copy)`,
                    directory: `${inst.directory}-copy`,
                    is_latest: false,
                  };
                  setOpenedDialog({
                    name: "installation_dialog",
                    props: { type: "dupl", installation: dup, original_id: inst.id },
                  });
                }}
              >
                <HiDocumentDuplicate />
              </button>
              {!inst.is_latest && (
                <button
                  className="install-action-btn delete"
                  title={localized(locale, "Delete", "削除")}
                  onClick={() => {
                    setOpenedDialog({
                      name: "confirm_dialog",
                      props: {
                        title: localized(locale, `Deleting ${inst.name}`, `${inst.name}を削除`),
                        message: localized(
                          locale,
                          "Are you sure you want to delete this installation?",
                          "このインストールを削除してもよろしいですか？",
                        ),
                        onConfirm: async () => {
                          const index = installations.findIndex((i) => i.id === inst.id);
                          const res = await commands.deleteInstallation(inst.id);
                          if (res.ok || res.error.kind === "InstallNotFound") {
                            setInstallations((prev) => prev.filter((i) => i.id !== inst.id));
                            setActiveInstall((current) => {
                              if (current?.id !== inst.id) return current;
                              const newList = installations.filter((i) => i.id !== inst.id);
                              const next = newList[index] ?? newList[index - 1] ?? null;
                              if (
                                !next ||
                                (next &&
                                  newList.every(
                                    (i) => i.id === "latest-release" || i.id === "latest-snapshot",
                                  ))
                              ) {
                                return newList.find((i) => i.id === "latest-release") ?? next;
                              }
                              return next;
                            });
                          }
                        },
                      },
                    });
                  }}
                >
                  <HiTrash />
                </button>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
