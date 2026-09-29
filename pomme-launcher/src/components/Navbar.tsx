import { getVersion } from "@tauri-apps/api/app";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useEffect, useState } from "react";
import {
  HiArrowRightOnRectangle,
  HiClipboardDocument,
  HiChevronDown,
  HiCog6Tooth,
  HiHome,
  HiNewspaper,
  HiPuzzlePiece,
  HiServer,
  HiSquares2X2,
  HiTrash,
  HiUserGroup,
  HiUserPlus,
} from "react-icons/hi2";
import { localeOf, translate } from "../lib/i18n";
import { useAppStateContext } from "../lib/state";
import { Page } from "../lib/types";

interface NavItem {
  id: Page;
  labelKey: "home" | "installations" | "servers" | "friends" | "mods" | "news";
  icon: React.ReactNode;
  soon?: boolean;
}

const NAV_ITEMS: Array<NavItem> = [
  { id: "home", labelKey: "home", icon: <HiHome /> },
  { id: "installations", labelKey: "installations", icon: <HiSquares2X2 /> },
  { id: "servers", labelKey: "servers", icon: <HiServer /> },
  { id: "friends", labelKey: "friends", icon: <HiUserGroup /> },
  { id: "mods", labelKey: "mods", icon: <HiPuzzlePiece />, soon: true },
  { id: "news", labelKey: "news", icon: <HiNewspaper /> },
];

//
//
// this space intentionally left blank
//
//

interface NavProps {
  startAddAccount: () => void;
  switchAccount: (index: number) => void;
  removeAccount: (uuid: string) => void;
}

export default function Navbar({ startAddAccount, switchAccount, removeAccount }: NavProps) {
  const {
    accountDropdown,

    account,
    accounts,
    page,
    setPage,
    skinUrl,

    activeIndex,
    authLoading,
    authUrl,
    friendsList,
    launcherSettings,
  } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);

  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion).catch(console.error);
  }, []);

  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(timer);
  }, [copied]);

  const copyAuthUrl = () => {
    if (!authUrl) return;
    writeText(authUrl)
      .then(() => setCopied(true))
      .catch((e) => console.error("Failed to copy sign-in link:", e));
  };

  const incomingCount = friendsList.incomingRequests?.length ?? 0;

  const { ref: accountDropdownRef } = accountDropdown;

  return (
    <nav className="sidebar">
      <div className="sidebar-brand">
        <div className="brand-icon">
          <img src="/pomme.png" alt="Pomme" className="brand-logo" />
        </div>
        <div className="brand-text">
          <span className="brand-name">POMME</span>
          <span className="brand-sub">LAUNCHER</span>
        </div>
        <span className="brand-version">{version && `v${version}`}</span>
      </div>

      <div className="sidebar-nav">
        {NAV_ITEMS.map((item) => (
          <button
            key={item.id}
            className={`nav-btn ${page === item.id ? "active" : ""}`}
            onClick={() => setPage(item.id)}
          >
            <span className="nav-icon">{item.icon}</span>
            <span className="nav-text" lang={locale}>
              {translate(locale, item.labelKey)}
            </span>
            {item.soon && <span className="nav-soon">SOON</span>}
            {item.id === "friends" && incomingCount > 0 && (
              <span className="nav-badge">{incomingCount}</span>
            )}
          </button>
        ))}
      </div>

      <div className="sidebar-bottom">
        {account ? (
          <div className="account-switcher" ref={accountDropdownRef}>
            <button className="account-bar" onClick={accountDropdown.toggle}>
              <div
                className="mc-head"
                style={skinUrl ? { backgroundImage: `url("${skinUrl}")` } : undefined}
              />
              <span className="account-username">{account.username}</span>
              <HiChevronDown className={`account-arrow ${accountDropdown.isOpen ? "open" : ""}`} />
            </button>
            {accountDropdown.isOpen && (
              <div className="account-dropdown-menu">
                {accounts.map((acc, i) => (
                  <div
                    key={acc.uuid}
                    className={`account-option ${i === activeIndex ? "active" : ""}`}
                  >
                    <button className="account-option-btn" onClick={() => switchAccount(i)}>
                      {acc.username}
                    </button>
                    <button className="account-remove" onClick={() => removeAccount(acc.uuid)}>
                      <HiTrash />
                    </button>
                  </div>
                ))}
                <button className="account-add" onClick={startAddAccount} disabled={authLoading}>
                  <HiUserPlus />
                  <span>{authLoading ? "Signing in..." : "Add account"}</span>
                </button>
                <button
                  className="account-menu-btn logout"
                  onClick={() => {
                    if (account) removeAccount(account.uuid);
                  }}
                >
                  <HiArrowRightOnRectangle />
                  <span>Log out</span>
                </button>
              </div>
            )}
          </div>
        ) : (
          <button className="sign-in-sidebar" onClick={startAddAccount} disabled={authLoading}>
            {authLoading ? "Signing in..." : "SIGN IN"}
          </button>
        )}
        <button className="account-menu-btn" onClick={() => setPage("settings")}>
          <HiCog6Tooth />
          <span lang={locale}>{translate(locale, "settingsNav")}</span>
        </button>
        {authLoading && authUrl && (
          <button className="auth-copy-link" onClick={copyAuthUrl}>
            <HiClipboardDocument />
            <span>{copied ? "Copied!" : "Link didn't open? Copy it"}</span>
          </button>
        )}
      </div>
    </nav>
  );
}
