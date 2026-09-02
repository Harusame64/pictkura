/**
 * ドイツ語辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **語は発明せず、既にドイツ語圏で使われているものへ合わせる**（2026-09-01 の判断）:
 *
 * - **⚑ / ✕ / U は Lightroom Classic の独語版に合わせる**——`Auswahl` /
 *   `Abgelehnt` / `Ohne Markierung`。pictkura の P / X / U はLightroomと同じ配列なので、
 *   ドイツの写真の人には**最初から通じる語**になる
 * - **OSの用語は引いてくる**。`Papierkorb`（ゴミ箱）・`Strg`（Ctrl）・
 *   `Umschalt`（Shift）・`Leertaste`（Space）・`HEVC-Videoerweiterungen`（Microsoft Store の名前）。
 *   ここは訳ではなく**正解が1つある場所**
 * - **金額は€に直す**。英語辞書が `数百円` を "a few dollars" にしているのと同じ扱いで、
 *   直訳しない
 * - 釦は不定詞、見出しとナビは名詞（独語UIの慣習）。名詞は大文字で始める
 *
 * **2026-09-01、独立した2つのレビューを通した**——**訳の読解に限った検算**で、
 * コードの2ゲート（codex / `/code-review`）はこの辞書には掛けていない
 * （遡って掛けたのは2026-09-01。`plan.md`）。見たのは（`updateOnStartNote` /
 * `videoCodecNote` / `settingsImportStructureNote` / `emptyManagedLibrary` 系の4つ
 * ——約束・お金・警告・「なぜ何も出ないか」を載せているキー）。直したのは3つ:
 *
 * - **課金する相手を名指しする**。`kostenpflichtige Erweiterung` だけだと
 *   pictkura の課金と読む余地がある。`aus dem Microsoft Store` を足した
 * - **警告の強さ**。`nur schwer ändern` は事実の記述に聞こえる。
 *   `nur mit erheblichem Aufwand` にした。ただし **`Wichtig:` のような札は付けない**
 *   ——日本語も英語も札を立てていないので、独語だけ声が大きくなる
 * - **「読めない」と「読まない」**。`liest nicht in eine solche hinein` は
 *   不自然なうえ、**故障と設計の区別が付かない**。`liest ... bewusst nicht ein` で
 *   意図だと言い切る。**同じ言い回しが4か所にあるので、まとめて直すこと**
 *
 * `updateOnStartNote` は両者とも所見なしで**据え置き**。あわせて
 * 「アップデート確認以外は通信しない」が本当かをコードで確かめた——
 * `ureq::` の呼び出しは `update.rs` の1か所だけ、UIに `fetch` は無く、
 * `tauri.conf.json` の CSP が `connect-src` を ipc に限っている。**約束は真。**
 */
import { folderExample } from "./folderExample";
import { num, one } from "./plural";
import type { Dict } from "./ja";

export const de: Dict = {
  appName: "pictkura",
  viewThumbnails: "Fotos",
  viewCalendar: "Kalender",
  searchPlaceholder: "Dateien, Ordner, Kameras, 2019-08 oder year:2019 suchen",
  searchClear: "Suche löschen (Esc)",
  commandPalette: "Befehlspalette",
  importFromUsb: "Von USB importieren",
  rescan: "Neu einlesen",
  size: "Größe",
  /**
   * 一覧の件数。**数と単位を1つのキーにする**（2026-09-02、ゲート2の指摘）。
   * 呼ぶ側で `${formatNumber(n)} {t.itemsSuffix}` と組んでいたので、
   * **1枚に絞ると「1 items」「1 Objekte」**と出ていた——このPRが潰しに来た
   * `full scan (1 files)` と同じ壊れ方が、画面で一番目立つ数字に残っていた。
   * 単位だけのキーでは、どの言語も単数形を書けない。
   */
  itemsCount: (n: number) => `${num(n)} ${one(n, "Objekt", "Objekte")}`,
  navPlaces: "Orte",
  navAllPhotos: "Alle Fotos",
  navFavorites: "★ Favoriten",
  navPicked: "⚑ Auswahl",
  navKinds: "Art",
  kindPhoto: "Fotos",
  kindRaw: "RAW",
  kindVideo: "Videos",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "Tastenkürzel (?)",
  keyCtrl: "Strg",
  actionShortcuts: "Tastenkürzel anzeigen",
  shortcutGroups: [
    {
      title: "Übersicht",
      keys: [
        ["Strg+K / ⌘K", "Befehlspalette (zu Datum oder Kamera springen, suchen, importieren)"],
        ["Strg+A / ⌘A", "Alles auswählen, was Suche und Filter gerade treffen"],
        ["Umschalt + Klick", "Alles zwischen dem zuletzt geklickten Foto und diesem auswählen"],
        ["Strg + Klick", "Ein Foto hinzufügen oder entfernen (⌘ + Klick unter macOS)"],
        ["Auf ein Datum klicken", "Den ganzen Tag auswählen (nochmal klicken hebt es auf)"],
        ["Esc", "Auswahl beenden"],
      ],
    },
    {
      title: "Große Ansicht",
      keys: [
        ["← / →", "Vorheriges / nächstes Foto"],
        ["P", "Als Auswahl markieren (⚑). Springt standardmäßig zum nächsten Foto"],
        ["X", "Als abgelehnt markieren (✕). Abgelehnte Fotos wandern beim Schließen in den Papierkorb"],
        ["U", "Markierung dieses Fotos aufheben (⚑ und ✕ entfernen)"],
        ["Strg+C / ⌘C", "Das angezeigte Bild in die Zwischenablage kopieren"],
        ["Strg+S / ⌘S", "Das angezeigte Bild als Datei speichern"],
        ["F", "Favorit (★) an- und abschalten"],
        ["I", "Aufnahmedaten (Kamera, Objektiv, Blende, ISO, GPS)"],
        ["Leertaste", "Diashow. Bei einem Video: abspielen / anhalten"],
        ["1 / 0", "Originalgröße 100 % / an das Fenster anpassen"],
        ["F11", "Vollbild"],
        ["Esc", "Schließen"],
      ],
    },
    {
      title: "Maus (in der großen Ansicht)",
      keys: [
        ["Doppelklick", "Originalgröße 100 % ⇔ an das Fenster anpassen"],
        ["Mausrad", "Vergrößern / verkleinern"],
        ["Ziehen", "Im vergrößerten Bild verschieben"],
        ["Rechtsklick", "Öffnen / Öffnen mit / im Ordner zeigen / in den Papierkorb"],
        ["Auf den Streifen klicken", "Zu diesem Foto springen"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "Kameras & Medien",
  navLibraryFolders: "Bibliotheksordner",
  navDrives: "Laufwerke",
  navAddFolder: "Ordner hinzufügen",
  add: "Hinzufügen",
  browse: "Durchsuchen…",
  addFolderPlaceholder: folderExample("z. B. ", "benutzer"),
  pickLibraryFolder: "Ordner wählen, der zur Bibliothek hinzugefügt wird",
  showMore: (n: number) => `${num(n)} weitere`,
  collapse: "Weniger anzeigen",
  photosCount: (n: number) => `${num(n)}`,
  memoriesTitle: (years: number) =>
    years === 1 ? "Heute vor 1 Jahr" : `Heute vor ${num(years)} Jahren`,
  viewerFavorite: "Favorit (F)",
  viewerPick: "Als Auswahl markieren (P)",
  viewerUnpick: "Markierung aufheben (U)",
  viewerPicked: "Als Auswahl markiert",
  judgeFav: "Favorit",
  judgeUnfav: "Favorit entfernt",
  judgePick: "Als Auswahl markiert",
  judgeUnflag: "Markierung aufgehoben",
  viewerReject: "Als abgelehnt markieren (X)",
  viewerRejected: "Abgelehnt",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "Abgelehnte Fotos durchsehen",
  rejectGateTitle: (n: number) =>
    n === 1
      ? "1 Foto in den Papierkorb verschieben"
      : `${num(n)} Fotos in den Papierkorb verschieben`,
  rejectGateNote:
    "Du kannst sie aus dem Papierkorb wiederherstellen (dann sind sie auch wieder in der Bibliothek).",
  rejectGateRestore: "Behalten",
  rejectGateBack: "Zurück",
  rejectGateDiscard: "Schließen, ohne zu löschen",
  rejectGateConfirm: (n: number) =>
    n === 1 ? "1 in den Papierkorb" : `${num(n)} in den Papierkorb`,
  rejectGateTrashing: (done: number, total: number) =>
    `Wird verschoben… (${num(done)} / ${num(total)})`,
  updateFound: (v: string) => `Version ${v} ist verfügbar`,
  updateOpenPage: "Download-Seite öffnen",
  updateLater: "Später",
  updateCheckNow: "Nach Updates suchen",
  updateChecking: "Wird geprüft…",
  updateUpToDate: "Du hast die aktuelle Version",
  updateFailed: "Prüfung nicht möglich",
  updateOnStart: "Beim Start nach Updates suchen",
  updateOnStartNote:
    "Fragt GitHub nach dem Namen der neuesten Version (einmal am Tag). Es werden keine Fotos und keine Dateinamen gesendet. Wenn du das ausschaltest, verlässt nichts diesen Rechner — außer wenn du „Nach Updates suchen“ drückst.",
  viewerSlideshow: "Diashow (Leertaste)",
  // 抽出（Issue #13）
  extractSave: (key: string) => `Dieses Bild als Datei speichern (${key})`,
  extractCopy: (key: string) => `Dieses Bild in die Zwischenablage kopieren (${key})`,
  extractSaveTitle: "Bild speichern unter",
  extractFilter: "Bild",
  extractSaved: "Gespeichert",
  extractCopied: "Kopiert",
  extractFailed: "Dieses Bild konnte nicht entnommen werden",
  extractSameFile: "Die Originaldatei kann nicht überschrieben werden",
  viewerExif: "Fotoinfos (I)",
  viewerFullscreen: "Vollbild (F11)",
  viewerClose: "Schließen (Esc)",
  viewerPrev: "Vorheriges (←)",
  viewerNext: "Nächstes (→)",
  viewerFitToScreen: "An das Fenster anpassen (0)",
  viewerActualSize: "Originalgröße, 100 % (1) — oder Doppelklick",
  actualSizeBadge: "1:1",
  // 動画（第9部）
  videoUnsupported: "Dieses Format kann in der App nicht abgespielt werden",
  videoMissing: "Diese Datei fehlt (sie wurde wohl verschoben oder gelöscht)",
  videoCloudOnly: "Dieses Video liegt in der Cloud",
  videoCloudOnlyNote:
    "Wenn du es hier abspielst, startet zuerst ein Download, und bis er fertig ist, ist nichts zu sehen. Öffnest du es in der Standard-App, kannst du den Fortschritt verfolgen.",
  videoFailed: "Dieses Video konnte nicht abgespielt werden",
  videoOpenExternal: "In der Standard-App öffnen",
  videoCodecNote:
    "Videos von iPhones und ähnlichen Kameras nutzen HEVC (H.265). Zum Abspielen braucht das System einen Decoder; unter Windows ist das eine kostenpflichtige Erweiterung aus dem Microsoft Store (ein paar Euro).",
  videoCodecNoteMac:
    "macOS decodiert HEVC von Haus aus, daher ist es vermutlich ein Aufnahmeformat, mit dem es nichts anfangen kann.",
  videoCodecNoteOther:
    "Auf deinem System fehlt vermutlich ein Decoder für dieses Aufnahmeformat.",
  videoCodecHelp: "HEVC-Videoerweiterungen holen (kostenpflichtig)",
  loading: "Wird geladen…",
  exifTitle: "Fotoinfos",
  exifCamera: "Kamera",
  exifLens: "Objektiv",
  exifAperture: "Blende",
  exifShutter: "Belichtungszeit",
  exifIso: "ISO",
  exifFocal: "Brennweite",
  exifLocation: "Ort",
  exifNone: "Keine EXIF-Daten",
  paletteInput: "Datum, Kamera, Suchwort oder Befehl…",
  paletteNoResults: "Keine Treffer",
  paletteGroupJumpDate: "Zu Datum springen",
  paletteGroupRecentDays: "Letzte Tage",
  paletteGroupCameras: "Nach Kamera filtern",
  paletteGroupSearch: "Suchen",
  paletteGroupActions: "Aktionen",
  paletteSearchFor: (q: string) => `Nach „${q}“ suchen`,
  paletteSearchHint: "Dateiname, Ordner, Kamera",
  paletteSelect: "Auswählen",
  paletteRun: "Ausführen",
  paletteCloseHint: "Schließen",
  actionShowFavorites: "Nur Favoriten zeigen",
  actionShowPicked: "Nur die Auswahl zeigen",
  actionShowAll: "Alle Fotos zeigen",
  actionCalendar: "Kalenderansicht",
  actionThumbnails: "Fotoübersicht",
  indexBuilding: "🔍 Suchindex wird aufgebaut… ",
  cameraScanning: "📷 Kameradaten werden gelesen… ",
  indexIncompleteWarning:
    "⚠ Der Suchindex wurde unterbrochen — Treffer können unvollständig sein (es geht beim nächsten Start weiter)",
  indexProgressSuffix: " % — bis das fertig ist, können Treffer fehlen",
  removeRoot: (path: string) => `${path} aus der Bibliothek entfernen`,
  importFrom: (path: string) => `Aus ${path} importieren`,
  filterByCamera: (name: string) => `Nur Fotos zeigen, die mit ${name} aufgenommen wurden`,
  jumpToYear: (year: number) => `Zu ${year} springen`,
  importing: (done: number, total: number) => `Wird importiert… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `Import fertig: ${num(copied)} kopiert, ${num(skipped)} übersprungen`,
  importFailed: (n: number) => `, ${num(n)} fehlgeschlagen`,
  importIncomplete:
    " ⚠ Einige Ordner konnten nicht gelesen werden — lösche die Karte noch nicht",
  syncDone: (added: number, changed: number, removed: number) =>
    `${num(added)} hinzugefügt, ${num(changed)} geändert, ${num(removed)} entfernt`,
  pickSource: "Ordner wählen, aus dem importiert wird (USB / DCIM)",
  pickDestination: "Zielordner wählen",
  wizardTitle: "Import",
  wizardSources: "Quelle",
  wizardOtherFolder: "Anderer Ordner…",
  wizardRefresh: "Laufwerke neu einlesen",
  wizardRemovable: "Wechselmedium",
  wizardNoDrives: "Keine Laufwerke gefunden",
  emptyTitle: "Noch keine Fotos",
  emptyTitleFailed: "Die Liste konnte nicht angezeigt werden",
  emptyTitleChecking: "Wird geprüft",
  emptyTitleStartupFailed: "Der Abgleich beim Start wurde nicht fertig",
  emptyStartupFailed:
    "Der Abgleich, der beim Start läuft, wurde nicht fertig. Es kann Fotos geben, die noch nicht in die Bibliothek aufgenommen wurden. Drücke „Neu einlesen“; hilft das nicht, öffne die App neu.",
  emptyTitleMissing: "Einige Orte sind nicht da",
  emptyTitleUnreadable: "Einige Orte ließen sich nicht öffnen",
  emptyNoRoots:
    "Es ist noch kein Bibliotheksordner eingerichtet. Importiere von einer Karte oder wähle einen Ordner, in dem Fotos liegen.",
  emptyMissing: (names: string) =>
    `Diese Orte sind nicht da: ${names}. Wenn das eine externe Festplatte ist, schließe sie an und drücke „Neu einlesen“.`,
  emptyUnreadableMac: (names: string) =>
    `Diese Orte ließen sich nicht öffnen: ${names}. Erlaube pictkura in den Systemeinstellungen unter „Datenschutz & Sicherheit“ den Zugriff auf diesen Ordner (Schreibtisch, Dokumente, externes Laufwerk). Liegt er im Netzwerk, prüfe die Verbindung und drücke „Neu einlesen“.`,
  emptyUnreadableWin: (names: string) =>
    `Diese Orte ließen sich nicht öffnen: ${names}. Prüfe die Berechtigungen des Ordners. Bei einem Netzlaufwerk prüfe die Verbindung und drücke „Neu einlesen“.`,
  emptyUnreadableOther: (names: string) =>
    `Diese Orte ließen sich nicht öffnen: ${names}. Prüfe, ob du sie lesen darfst, und drücke dann „Neu einlesen“.`,
  listSeparator: ", ",
  /**
   * **1のときは名詞を書く。名詞は `Ort`**（2026-09-02、ゲート2の指摘）。
   *
   * 2026-09-01 にはここを「受ける名詞が呼び出しごとに変わるから据え置く」と書いていて、
   * その根拠に `showMore`（`Kameras`）を挙げていた。**それが間違い**——`showMore` は
   * 別のキーで、`andMore` に来るのは `nameList()` からだけ（`App.tsx:1572`）。
   *
   * そして `nameList()` を呼ぶ4つの文は**すべて `Orte` と言っている**
   * （`emptyMissing` / `emptyStalled` / `emptyUnreadable*`）。`Ordner` と書くと
   * 文の途中で名詞が入れ替わるうえ、**嘘にもなる**——`emptyAllExcluded` の
   * `EntryKind::Excluded` は**ファイルにも付く**（`lib.rs` の `classify_entry`。
   * `*.heic` のような除外パターンなら中身はファイル）。`Ort` は男性なので
   * `weiterer` が取れて、フォルダにもファイルにも当たる。
   */
  andMore: (n: number) => (n === 1 ? "und 1 weiterer Ort" : `und ${num(n)} weitere`),
  emptyRootIsPackage:
    "Einer der Bibliotheksordner ist selbst eine Fotos-Mediathek. pictkura liest solche Mediatheken bewusst nicht ein, es wird also nie etwas daraus kommen. Wähle einen gewöhnlichen Ordner, in dem Fotos liegen, oder importiere von einer Karte.",
  emptyPhotoLibrary:
    "Außer der Mediathek der Fotos-App wurde nichts gefunden. pictkura liest die Fotos-Mediathek standardmäßig nicht ein — die meisten Originale liegen in iCloud, nicht auf diesem Mac. Importiere von einer Karte oder wähle einen gewöhnlichen Ordner, in dem Fotos liegen.",
  emptyManagedLibrary:
    "Außer Mediatheken von Fotoverwaltungen (Fotos, iPhoto oder Aperture) wurde nichts gefunden. pictkura liest solche Mediatheken bewusst nicht ein — es könnte ihren Inhalt zwar einmal indizieren, würde spätere Änderungen aber nie wieder mitbekommen. Importiere von einer Karte oder wähle einen gewöhnlichen Ordner, in dem Fotos liegen.",
  emptyRootIsManagedLibrary:
    "Einer der Bibliotheksordner ist selbst die Mediathek einer Fotoverwaltung (Fotos, iPhoto oder Aperture). pictkura liest solche Mediatheken bewusst nicht ein, es wird also nie etwas daraus kommen. Wähle einen gewöhnlichen Ordner, in dem Fotos liegen, oder importiere von einer Karte.",
  emptyAllExcluded: (names: string) =>
    `Alles Gefundene wird von den Ausschlussmustern übersprungen (zum Beispiel ${names}). Du kannst sie in der pictkura.toml im Einstellungsordner ändern.`,
  emptyNothingHere:
    "Es sind noch keine Fotos aufgetaucht, die pictkura lesen kann. Importiere von einer Karte oder wähle einen Ordner, in dem Fotos liegen.",
  calendarChecking: "Wird geprüft…",
  emptyTitleStalled: "Einige Orte antworten nicht",
  emptyStalled: (names: string) =>
    `Diese Orte antworten nicht: ${names}. Liegt einer davon im Netzwerk, prüfe die Verbindung und drücke „Neu einlesen“. Ist er endgültig weg, entferne ihn aus den Bibliotheksordnern, dann melden sich die übrigen.`,
  emptyChecking:
    "Die Ordner werden noch durchgesehen. Liegt einer davon im Netzwerk, prüfe die Verbindung und drücke „Neu einlesen“.",
  emptyLoadFailed:
    "Die Liste konnte nicht geladen werden. Der Grund steht in der Leiste oben. Drücke „Neu einlesen“ oder öffne die App neu.",
  wizardPickFolderHint: "Wähle links einen Ordner, um die Fotos darin zu sehen",
  wizardNoImages: "Keine Fotos in diesem Ordner",
  wizardUnreadable: "Dieser Ordner ließ sich nicht lesen (er wurde vielleicht entfernt)",
  wizardCounting: "Wird geladen…",
  wizardSelectAll: "Alle auswählen",
  wizardSelectNew: "Nur neue auswählen",
  wizardClearSelection: "Auswahl aufheben",
  wizardSelected: (n: number) => `${num(n)} ausgewählt`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "Schon importiert (dieselbe Datei liegt im Zielordner)",
  wizardDestination: "Ziel",
  wizardChangeDestination: "Ändern",
  wizardStructure: "Ablage",
  wizardImportButton: (n: number) => `${num(n)} importieren`,
  wizardImportAll: "Diesen ganzen Ordner importieren (samt Unterordnern)",
  wizardImportAllShort: "Ganzer Ordner",
  wizardDeep: "Unterordner einbeziehen",
  wizardDeepHint:
    "Durchsucht das ganze Medium, damit du nicht wissen musst, wo die Fotos liegen",
  wizardScanning: "Das Medium wird durchgesehen…",
  wizardTruncated: (n: number) =>
    `Es werden nur die ersten ${num(n)} gezeigt. Nimm „Ganzer Ordner“, um alles zu importieren`,
  wizardScanIncomplete:
    "⚠ Einige Ordner ließen sich nicht lesen (es können Fotos fehlen)",
  decoderHeifNotice: (n: number) =>
    `⚠ Für ${num(n)} ${one(n, "HEIC/HEIF-Foto", "HEIC/HEIF-Fotos")} gibt es hier keine Vorschau, und ${one(n, "öffnen lässt es sich", "öffnen lassen sie sich")} auch nicht. Dafür braucht es die kostenlosen HEIF-Bilderweiterungen und zusätzlich die kostenpflichtigen HEVC-Videoerweiterungen (ein paar Euro), die die Pixel decodieren`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ Für ${num(n)} ${one(n, "HEIC/HEIF-Foto", "HEIC/HEIF-Fotos")} gibt es hier keine Vorschau, und ${one(n, "öffnen lässt es sich", "öffnen lassen sie sich")} auch nicht`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ Für ${num(n)} ${one(n, "HEIC/HEIF-Foto", "HEIC/HEIF-Fotos")} gibt es hier keine Vorschau, und ${one(n, "öffnen lässt es sich", "öffnen lassen sie sich")} auch nicht. Auf deinem System fehlt vermutlich ein Decoder für HEIC/HEVC`,
  decoderHeifHow: "HEIF-Bilderweiterungen (kostenlos)",
  decoderHevcHow: "HEVC-Videoerweiterungen (kostenpflichtig)",
  decoderNoticeDismiss: "Nicht mehr anzeigen",
  wizardOfflineTitle:
    "Diese Datei liegt in der Cloud (hier keine Vorschau; beim Import wird sie geladen)",
  wizardHideImported: "Schon importierte ausblenden",
  wizardAllImported:
    "Hier ist nichts Neues (alles in diesem Ordner ist schon importiert)",
  // **名詞を省くと単数形が決まらない**（2026-09-02、ゲート2が2巡続けて挙げた）。
  // 省かれているのが `Datei`（女性）なら `1 schon importierte` で正しく、
  // `Foto`（中性）なら `1 schon importiertes` になる——読む人には区別が付かない。
  // 1のときだけ名詞を書けば、どちらの読みでも正しい。複数形は今までどおり
  wizardHiddenCount: (n: number) =>
    n === 1
      ? "1 schon importierte Datei ausgeblendet"
      : `${num(n)} schon importierte ausgeblendet`,
  wizardEtaSeconds: (n: number) => `noch etwa ${num(n)} s`,
  wizardEtaMinutes: (n: number) => `noch etwa ${num(n)} min`,
  wizardEtaCalculating: "Restzeit wird geschätzt…",
  wizardCapped: (n: number) => `${num(n)}+`,
  wizardMoreFiles: (n: number) => `${num(n)} weitere (zum Laden scrollen)`,
  menuOpen: "Öffnen",
  menuOpenWith: (name: string) => `Mit ${name} öffnen`,
  menuOpenWithOther: "Mit anderer App öffnen…",
  menuReveal: "Im Ordner zeigen",
  menuDelete: "Löschen (in den Papierkorb)",
  menuFavoriteOn: "Zu Favoriten hinzufügen",
  menuFavoriteOff: "Aus Favoriten entfernen",
  pickEditor: "App zum Bearbeiten wählen",
  deleteConfirm: (n: number) =>
    n === 1
      ? "Dieses Foto in den Papierkorb verschieben?"
      : `${num(n)} Fotos in den Papierkorb verschieben?`,
  deleted: (n: number) => `${num(n)} in den Papierkorb verschoben`,
  deletedSomeLeft: (n: number, left: number) =>
    `${num(n)} in den Papierkorb verschoben (${num(left)} ${one(left, "war", "waren")} nicht auffindbar und ${one(left, "blieb", "blieben")} unangetastet)`,
  // 複数選択と一括操作
  selectItem: "Auswählen",
  selectedCount: (n: number) => (n === 1 ? "1 ausgewählt" : `${num(n)} ausgewählt`),
  selectAll: "Alle auswählen",
  clearSelection: "Auswahl aufheben (Esc)",
  selectDay: "Diesen ganzen Tag auswählen",
  bulkFavoriteOn: "Zu Favoriten hinzufügen",
  bulkFavoriteOff: "Aus Favoriten entfernen",
  bulkDelete: "In den Papierkorb",
  bulkCopy: "In Ordner kopieren",
  bulkMove: "In Ordner verschieben",
  bulkViewer: "Die Auswahl ansehen",
  pickExportFolder: "Ordner zum Exportieren wählen",
  moveConfirm: (n: number) =>
    n === 1
      ? "Dieses Foto in einen Ordner verschieben, den du gleich wählst? Es verlässt seinen bisherigen Platz und die Bibliothek (★ und ⚑ werden nicht mitgenommen)."
      : `${num(n)} Fotos in einen Ordner verschieben, den du gleich wählst? Sie verlassen ihren bisherigen Platz und die Bibliothek (★ und ⚑ werden nicht mitgenommen).`,
  exporting: (done: number, total: number, name: string) =>
    `Wird exportiert… ${num(done)}/${num(total)} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, leftBehind: number) => {
    const parts = [done === 1 ? "1 Foto exportiert" : `${num(done)} Fotos exportiert`];
    if (skipped > 0) parts.push(`${num(skipped)} ${one(skipped, "war", "waren")} schon da`);
    if (failed > 0) parts.push(`${num(failed)} fehlgeschlagen`);
    if (leftBehind > 0)
      parts.push(`${num(leftBehind)} ${one(leftBehind, "ließ", "ließen")} sich am bisherigen Platz nicht entfernen`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "Als Auswahl markieren",
  bulkPickOff: "Markierung aufheben",
  bulkPickDone: (n: number) =>
    n === 1 ? "1 Foto als Auswahl markiert" : `${num(n)} Fotos als Auswahl markiert`,
  bulkUnpickDone: (n: number) =>
    n === 1 ? "Markierung von 1 Foto aufgehoben" : `Markierung von ${num(n)} Fotos aufgehoben`,
  bulkFavoriteDone: (n: number) =>
    n === 1
      ? "1 Foto zu Favoriten hinzugefügt"
      : `${num(n)} Fotos zu Favoriten hinzugefügt`,
  bulkUnfavoriteDone: (n: number) =>
    n === 1
      ? "1 Foto aus Favoriten entfernt"
      : `${num(n)} Fotos aus Favoriten entfernt`,
  settings: "Einstellungen",
  close: "Schließen",
  settingsTitle: "Einstellungen",
  settingsImportStructure: "Ordnerstruktur beim Import",
  settingsImportStructureNote:
    "Wie importierte Fotos nach Aufnahmedatum abgelegt werden. Das betrifft Tausende von Dateien und lässt sich später nur mit erheblichem Aufwand ändern. Wo ein Ordnername ein Datum trägt, steht in jeder Sprache das Jahr vorn, damit die Sortierung nach Namen zugleich nach Zeit sortiert.",
  settingsDestination: "Ziel",
  settingsDestinationUnset: "(nicht gesetzt — du wählst es beim ersten Import)",
  settingsFlatExample: "IMG_0001.JPG (keine Unterordner)",
  settingsCustomPattern: "Eigenes Muster",
  settingsCustomPatternNote:
    "{year} {month} {day} werden durch das Datum ersetzt. Mit / entstehen Ebenen. Unzulässige Zeichen und Sprünge in den übergeordneten Ordner (..) werden automatisch entfernt.",
  settingsCustomPatternResult: "Ergebnis",
  settingsViewer: "Wenn du ein Foto groß ansiehst",
  settingsAutoAdvanceToggle: "Nach P / U zum nächsten Foto springen",
  settingsAutoAdvanceNote:
    "In der großen Ansicht markiert P das Foto mit ⚑ (ein anderes Fach als ★ Favoriten), U hebt die Markierung auf. Ist das an, kommt sofort das nächste Foto — dann kostet das Aussortieren eine Taste pro Foto. Ist es aus, bleibst du beim selben Foto.",
  settingsAutoplay: "Wenn du ein USB-Laufwerk oder eine SD-Karte einsteckst",
  settingsAutoplayToggle: "pictkura in der automatischen Wiedergabe anbieten",
  settingsAutoplayNote:
    "Fügt pictkura den Optionen der automatischen Wiedergabe von Windows hinzu. Von allein startet es nie. Der Eintrag selbst ist auf Japanisch beschriftet. Beim Deinstallieren über den Installer wird er entfernt — die portable Version und die Kopien anderer Benutzer auf demselben PC aber nicht; schalte das in diesen Fällen aus, bevor du pictkura entfernst.",
  settingsAbout: "Über",
  settingsAboutLicense: "Veröffentlicht unter der MIT-Lizenz.",
  settingsManual: "Handbuch",
  settingsOssLicenses: "Verwendete Open-Source-Software",
  settingsDocNotBundled: "(in einem Entwicklungs-Build nicht enthalten)",
  settingsLanguage: "Sprache",
  settingsLanguageSystem: "Wie das System",
  settingsLanguageNote:
    "Beim Wechsel wird das Fenster neu geladen. Das Kopieren läuft im Hintergrund weiter, aber der Import-Assistent schließt sich und du siehst den Fortschritt nicht mehr — warte deshalb das Ende eines Imports ab, bevor du wechselst.",
  settingsTheme: "Erscheinungsbild",
  themeSystem: "Wie das System",
  themeLight: "Hell",
  themeDark: "Dunkel",
  settingsEditors: "Apps zum Bearbeiten",
  settingsEditorsNote: "Apps, die du unter „Mit anderer App öffnen…“ gewählt hast.",
  settingsForgetEditor: "Aus der Liste entfernen",
  calendarEmpty: "Keine Fotos",
  speedPrefix: (sec: string) => `⚡ Startprüfung in ${sec} s — `,
  speedUsn: "USN-Journal-Differenz: ",
  speedUsnNoChange: "keine Änderungen, keine Ordner durchlaufen",
  speedUsnDirty: (records: number, dirs: number) =>
    `${num(records)} ${one(records, "Journaleintrag", "Journaleinträge")} → nur ${num(dirs)} Ordner neu eingelesen`,
  speedPruned: (skipped: number) =>
    `beschnittener Durchlauf: ${num(skipped)} Ordner übersprungen`,
  speedFull: (total: number) =>
    `vollständiger Durchlauf (${num(total)} ${one(total, "Datei", "Dateien")})`,
  speedNoDiff: " — keine Änderungen",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` — ${num(added)} hinzugefügt, ${num(changed)} geändert, ${num(removed)} entfernt`,
};
