export type Locale = "en" | "ja";

const messages = {
  en: {
    settings: "SETTINGS",
    general: "General",
    language: "Language",
    languageDescription: "Display language for the launcher",
    keepLauncherOpen: "Keep launcher open",
    keepLauncherOpenDescription: "Keep the launcher open after the game starts",
    launchWithConsole: "Launch with console",
    launchWithConsoleDescription:
      "Automatically open a window with all output from the client- useful when debugging.",
    home: "HOME",
    installations: "INSTALLATIONS",
    servers: "SERVERS",
    friends: "FRIENDS",
    mods: "MODS",
    news: "NEWS & UPDATES",
    settingsNav: "Settings",
  },
  ja: {
    settings: "設定",
    general: "一般",
    language: "言語",
    languageDescription: "ランチャーの表示言語",
    keepLauncherOpen: "ランチャーを開いたままにする",
    keepLauncherOpenDescription: "ゲーム起動後もランチャーを開いたままにします",
    launchWithConsole: "コンソールを表示して起動",
    launchWithConsoleDescription:
      "デバッグに便利な、クライアントの出力を表示するウィンドウを自動で開きます。",
    home: "ホーム",
    installations: "インストール",
    servers: "サーバー",
    friends: "フレンド",
    mods: "MOD",
    news: "ニュースと更新",
    settingsNav: "設定",
  },
} satisfies Record<Locale, Record<string, string>>;

export function localized(locale: Locale, en: string, ja: string): string {
  return locale === "ja" ? ja : en;
}

export function localeOf(value: string | undefined): Locale {
  return value === "ja" || value === "Japanese" ? "ja" : "en";
}

export function translate(locale: Locale, key: keyof (typeof messages)["en"]): string {
  return messages[locale][key];
}
