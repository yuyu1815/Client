import { HiArrowPath, HiCheck, HiCog6Tooth, HiPlay, HiPlus, HiXMark } from "react-icons/hi2";
import { localized, localeOf } from "../lib/i18n";
import { Friend, isOffline, PresenceEntry } from "../lib/friends";
import { useAppStateContext } from "../lib/state";
import { handleLaunchType } from "../lib/types";

export default function FriendsPage({ handleLaunch }: { handleLaunch: handleLaunchType }) {
  const {
    launcherSettings,
    account,
    friendsList,
    friendsSorted,
    friendsError,
    friendsSkins,
    friendsPresence,
    sendFriendRequest,
    acceptFriendRequest,
    removeFriend,
    refreshPresence,
    clearFriendsError,
    setOpenedDialog,
  } = useAppStateContext();
  const locale = localeOf(launcherSettings.language);
  const t = (en: string, ja: string) => localized(locale, en, ja);

  if (!account) {
    return (
      <div className="page friends-page" lang={locale}>
        <h2 className="page-heading">{t("FRIENDS", "フレンド")}</h2>
        <p className="servers-empty">
          {t(
            "Sign in to view your friends list.",
            "フレンドリストを見るにはサインインしてください。",
          )}
        </p>
      </div>
    );
  }

  const friends = friendsSorted;
  const incoming = friendsList.incomingRequests ?? [];
  const outgoing = friendsList.outgoingRequests ?? [];

  const openAddDialog = () =>
    setOpenedDialog({
      name: "add_friend_dialog",
      props: { onSubmit: sendFriendRequest },
    });

  return (
    <div className="page friends-page" lang={locale}>
      <div className="friends-header">
        <h2 className="page-heading">{t("FRIENDS", "フレンド")}</h2>
        <div className="friends-header-actions">
          <button
            className="friends-settings-btn"
            onClick={refreshPresence}
            title={t("Refresh presence", "オンライン状態を更新")}
          >
            <HiArrowPath />
          </button>
          <button
            className="friends-settings-btn"
            onClick={() => setOpenedDialog({ name: "friend_settings_dialog", props: {} })}
            title={t("Friend settings", "フレンド設定")}
          >
            <HiCog6Tooth />
          </button>
          <button className="servers-add-btn" onClick={openAddDialog}>
            <HiPlus /> {t("Add Friend", "フレンドを追加")}
          </button>
        </div>
      </div>

      {friendsError && (
        <div className="friends-error" onClick={clearFriendsError}>
          {friendsError}
        </div>
      )}

      <FriendsSection
        title={t("Friends", "フレンド")}
        friends={friends}
        skinUrls={friendsSkins}
        presence={friendsPresence}
        locale={locale}
        emptyMessage={t("You haven't added any friends yet.", "フレンドはまだいません。")}
        renderActions={(uuid, p) => {
          const rawAddr = p?.status === "PLAYING_SERVER" ? p.joinInfo?.value : undefined;
          const joinAddress =
            rawAddr && /^[a-zA-Z0-9.\-:_[\]]+$/.test(rawAddr) ? rawAddr : undefined;
          return (
            <>
              {joinAddress && (
                <button
                  className="friends-btn accept"
                  onClick={() => handleLaunch({ serverIp: joinAddress })}
                  title={`${t("Join", "参加")} ${joinAddress}`}
                >
                  <HiPlay /> {t("Join", "参加")}
                </button>
              )}
              <button
                className="friends-btn"
                onClick={() => removeFriend(uuid)}
                title={t("Remove friend", "フレンドを削除")}
              >
                <HiXMark /> {t("Remove", "削除")}
              </button>
            </>
          );
        }}
      />

      <FriendsSection
        title={t("Incoming Requests", "受信したリクエスト")}
        friends={incoming}
        skinUrls={friendsSkins}
        presence={friendsPresence}
        locale={locale}
        hideWhenEmpty
        renderActions={(uuid) => (
          <>
            <button
              className="friends-btn accept"
              onClick={() => acceptFriendRequest(uuid)}
              title={t("Accept", "承認")}
            >
              <HiCheck /> {t("Accept", "承認")}
            </button>
            <button
              className="friends-btn"
              onClick={() => removeFriend(uuid)}
              title={t("Decline", "拒否")}
            >
              <HiXMark /> {t("Decline", "拒否")}
            </button>
          </>
        )}
      />

      <FriendsSection
        title={t("Outgoing Requests", "送信したリクエスト")}
        friends={outgoing}
        skinUrls={friendsSkins}
        presence={friendsPresence}
        locale={locale}
        hideWhenEmpty
        renderActions={(uuid) => (
          <button
            className="friends-btn"
            onClick={() => removeFriend(uuid)}
            title={t("Cancel request", "リクエストをキャンセル")}
          >
            <HiXMark /> {t("Cancel", "キャンセル")}
          </button>
        )}
      />
    </div>
  );
}

function FriendsSection({
  title,
  friends,
  skinUrls,
  presence,
  locale,
  emptyMessage,
  hideWhenEmpty,
  renderActions,
}: {
  title: string;
  friends: Friend[];
  skinUrls: Record<string, string>;
  presence: Record<string, PresenceEntry>;
  locale: "en" | "ja";
  emptyMessage?: string;
  hideWhenEmpty?: boolean;
  renderActions: (uuid: string, presence: PresenceEntry | undefined) => React.ReactNode;
}) {
  if (hideWhenEmpty && friends.length === 0) return null;

  return (
    <>
      <h3 className="mock-subheading">
        {title} — {friends.length}
      </h3>
      <div className="mock-list">
        {friends.length === 0 && emptyMessage && <p className="servers-empty">{emptyMessage}</p>}
        {friends.map((f) => (
          <FriendRow
            key={f.profileId}
            friend={f}
            skinUrl={skinUrls[f.profileId]}
            presence={presence[f.profileId]}
            locale={locale}
          >
            {renderActions(f.profileId, presence[f.profileId])}
          </FriendRow>
        ))}
      </div>
    </>
  );
}

function FriendRow({
  friend,
  skinUrl,
  presence,
  locale,
  children,
}: {
  friend: Friend;
  skinUrl: string | undefined;
  presence: PresenceEntry | undefined;
  locale: "en" | "ja";
  children: React.ReactNode;
}) {
  const offline = isOffline(presence);
  return (
    <div className="mock-friend">
      <div
        className={`mc-head ${offline ? "off" : ""}`}
        style={skinUrl ? { backgroundImage: `url("${skinUrl}")` } : undefined}
      />
      <div className="mock-friend-info">
        <span className={`mock-friend-name ${offline ? "off" : ""}`}>{friend.name}</span>
        <span className="mock-friend-status">{formatStatus(presence, locale)}</span>
      </div>
      <div className={`mock-dot ${offline ? "off" : "on"}`} />
      <div className="friends-actions">{children}</div>
    </div>
  );
}

function formatStatus(presence: PresenceEntry | undefined, locale: "en" | "ja"): string {
  const t = (en: string, ja: string) => localized(locale, en, ja);
  if (!presence || presence.status === "OFFLINE") {
    const seen = formatLastSeen(presence?.lastUpdated, locale);
    return seen ? `${t("Offline", "オフライン")} · ${seen}` : t("Offline", "オフライン");
  }
  switch (presence.status) {
    case "ONLINE":
      return t("Online", "オンライン");
    case "PLAYING_OFFLINE":
      return t("In singleplayer", "シングルプレイ中");
    case "PLAYING_REALMS":
      return t("Playing Realms", "Realmsをプレイ中");
    case "PLAYING_SERVER":
      return presence.joinInfo?.value
        ? `${t("Playing:", "プレイ中:")} ${presence.joinInfo.value}`
        : t("Playing multiplayer", "マルチプレイ中");
    case "PLAYING_HOSTED_SERVER":
      return t("Hosting local world", "ローカルワールドをホスト中");
    default:
      return presence.status;
  }
}

function formatLastSeen(iso: string | null | undefined, locale: "en" | "ja"): string {
  if (!iso) return "";
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const deltaSec = Math.max(0, (Date.now() - then) / 1000);
  if (deltaSec < 60) return localized(locale, "just now", "たった今");
  if (deltaSec < 3600)
    return localized(
      locale,
      `${Math.floor(deltaSec / 60)}m ago`,
      `${Math.floor(deltaSec / 60)}分前`,
    );
  if (deltaSec < 86400)
    return localized(
      locale,
      `${Math.floor(deltaSec / 3600)}h ago`,
      `${Math.floor(deltaSec / 3600)}時間前`,
    );
  if (deltaSec < 604800)
    return localized(
      locale,
      `${Math.floor(deltaSec / 86400)}d ago`,
      `${Math.floor(deltaSec / 86400)}日前`,
    );
  return new Date(then).toLocaleDateString(locale === "ja" ? "ja-JP" : "en-US", {
    month: "short",
    day: "numeric",
  });
}
