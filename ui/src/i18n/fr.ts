/**
 * フランス語辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **フランス本国のフランス語（fr-FR）で書く。** 語は発明せず、既にフランス語圏で
 * 使われているものへ合わせる（独語・西語辞書と同じ方針）:
 *
 * - **⚑ / ✕ / U は Lightroom Classic の仏語版に合わせる**——`retenue` /
 *   `rejetée` / `sans marqueur`（P / X / U）。⚑ と ✕ の両方を覆う語は `marqueur`
 *   （Lightroom でも ✕ は「却下の marqueur」）。`judgeUnflag` と `U` の行は
 *   ✕ を外したときにも出るので、⚑ 限定の `retenue` を使わず `marqueur` にしてある
 * - **⚑ に `sélection` を使わない**。複数選択が `sélection` / `sélectionner` なので、
 *   西語の `indicador` と同じ理由で ⚑ 側は `retenir` 系に限る
 * - **アプリの蔵書は `bibliothèque`、写真アプリの蔵書は `photothèque`**
 *   （macOS の仏語版自身の語。西語の `biblioteca` / `fototeca` と同じ分け方）
 * - **OSの用語は引いてくる**。`Corbeille`（ゴミ箱。Windows・macOS とも）・`Ctrl`・
 *   `Maj`（Shift）・`Espace`・`Exécution automatique`（Windowsの自動再生）・
 *   `Réglages Système → Confidentialité et sécurité`（macOS）・
 *   `Afficher d’autres options`（Windows 11 のエクスプローラー）・`liens physiques`（ハードリンク）・
 *   Microsoft Store の `Extensions d’image HEIF` / `Extensions vidéo HEVC`
 * - **金額は€に直す**（`quelques euros`。独西と同じ扱いで、直訳しない）
 * - **二人称は vous**（仏語のソフトウェアUIの標準）。釦は不定詞
 *   （`Supprimer` / `Annuler` のような Apple・Microsoft の仏語の書き方）、見出しは名詞
 * - **仏語の約物**: `:` `;` `!` `?` の前と « » の内側に**改行しない空白**
 *   （U+00A0。ソースでは ` ` と書いて見えるようにしてある）。
 *   ja の「」・en の “ ” は « » にする。`100 %` も数と % の間を空ける。
 *   アポストロフィは ’
 *
 * **単複は `one()` ではなく、下の `un()` を使う。** 仏語は **0 と 1 が単数**
 * （`0 photo`, `1 photo`）——`one()` は `n === 1` だけを単数にするので、
 * 0 で `0 photos` と出てしまう。数を取るキーはすべて `un()` を通すこと。
 */
import { folderExample } from "./folderExample.ts";
import { num } from "./plural.ts";
import type { Dict } from "./ja.ts";

/** 仏語の単複。**0 と 1 が単数**（`one()` との違いは冒頭に） */
const un = (n: number, singular: string, plural: string): string => (n < 2 ? singular : plural);

export const fr: Dict = {
  appName: "pictkura",
  viewThumbnails: "Photos",
  viewCalendar: "Calendrier",
  searchPlaceholder: "Fichiers, dossiers, appareils, 2019-08 ou year:2019",
  searchClear: "Effacer la recherche (Échap)",
  commandPalette: "Palette de commandes",
  importFromUsb: "Importer depuis USB",
  rescan: "Réanalyser",
  size: "Taille",
  /**
   * 一覧の件数。**数と単位を1つのキーにする**（2026-09-02、ゲート2の指摘）。
   * 呼ぶ側で `${formatNumber(n)} {t.itemsSuffix}` と組んでいたので、
   * **1枚に絞ると「1 items」「1 Objekte」**と出ていた——このPRが潰しに来た
   * `full scan (1 files)` と同じ壊れ方が、画面で一番目立つ数字に残っていた。
   * 単位だけのキーでは、どの言語も単数形を書けない。
   */
  itemsCount: (n: number) => `${num(n)} ${un(n, "élément", "éléments")}`,
  navPlaces: "Emplacements",
  navAllPhotos: "Toutes les photos",
  navFavorites: "★ Favoris",
  navPicked: "⚑ Retenues",
  navOutgoing: "Envoi",
  outgoingGuide:
    "Choisissez ce dossier Envoi comme dossier à sauvegarder (synchroniser) dans l’application de votre service photo. Ensuite, chaque importation y dépose les photos, et le service les récupère. Si vous utilisez Google Photos : ajoutez ce dossier à la sauvegarde de dossiers de Google Photos.",
  navKinds: "Type",
  kindPhoto: "Photos",
  kindRaw: "RAW",
  kindVideo: "Vidéos",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "Raccourcis clavier (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "Afficher les raccourcis clavier",
  shortcutGroups: [
    {
      title: "Grille",
      keys: [
        ["Ctrl+K / ⌘K", "Palette de commandes (aller à une date ou un appareil, rechercher, importer)"],
        ["Ctrl+, / ⌘,", "Ouvrir les paramètres (comme la roue dentée de la barre d’outils)"],
        ["Ctrl+A / ⌘A", "Sélectionner tout ce qui correspond à la recherche et au filtre actuels"],
        ["Maj + clic", "Sélectionner tout, de la dernière vignette cliquée jusqu’à celle-ci"],
        ["Ctrl + clic", "Ajouter ou retirer une photo (⌘ + clic sous macOS)"],
        ["Cliquer sur un en-tête de date", "Sélectionner toute la journée (cliquer à nouveau pour annuler)"],
        ["Échap", "Annuler la sélection"],
      ],
    },
    {
      title: "Affichage en grand",
      keys: [
        ["← / →", "Photo précédente / suivante"],
        ["P", "Marquer comme retenue (⚑). Par défaut, passe ensuite à la photo suivante"],
        ["X", "Marquer comme rejetée (✕). Les photos rejetées vont à la Corbeille à la fermeture"],
        ["U", "Retirer le marqueur de cette photo (⚑ et ✕)"],
        ["Ctrl+C / ⌘C", "Copier l’image affichée dans le presse-papiers"],
        ["Ctrl+S / ⌘S", "Enregistrer l’image affichée dans un fichier"],
        ["Ctrl+P / ⌘P", "Imprimer la photo affichée"],
        ["F", "Ajouter aux favoris (★) ou l’en retirer"],
        ["I", "Informations sur le fichier et la prise de vue (appareil, objectif, ouverture, vitesse, ISO, focale, GPS)"],
        ["Espace", "Diaporama. Sur une vidéo : lecture / pause"],
        ["1 / 0", "1 bascule entre taille réelle 100 % et ajusté à l’écran ; 0 ajuste toujours"],
        ["F11", "Plein écran"],
        ["Échap", "Fermer"],
      ],
    },
    {
      title: "Souris (en affichage en grand)",
      keys: [
        ["Double-clic", "Taille réelle 100 % ⇔ ajuster à l’écran"],
        ["Molette", "Zoom avant / arrière"],
        ["Glisser", "Se déplacer dans l’image agrandie"],
        ["Clic droit", "Menu (ouvrir, copier, faire pivoter, imprimer, informations, supprimer, etc.)"],
        ["Cliquer sur le bandeau", "Aller à cette photo"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "Appareils & supports",
  navLibraryFolders: "Dossiers de la bibliothèque",
  navDrives: "Lecteurs",
  navAddFolder: "Ajouter un dossier",
  add: "Ajouter",
  browse: "Parcourir…",
  addFolderPlaceholder: folderExample("ex. : ", "utilisateur"),
  pickLibraryFolder: "Choisir un dossier à ajouter à la bibliothèque",
  showMore: (n: number) => `${num(n)} ${un(n, "autre", "autres")}`,
  collapse: "Afficher moins",
  photosCount: (n: number) => `${num(n)}`,
  memoriesTitle: (years: number) =>
    `Il y a ${num(years)} ${un(years, "an", "ans")}, jour pour jour`,
  viewerFavorite: "Favori (F)",
  viewerPick: "Retenir (P)",
  viewerUnpick: "Retirer le marqueur (U)",
  viewerPicked: "Photo retenue",
  judgeFav: "Favori",
  judgeUnfav: "Retirée des favoris",
  judgePick: "Retenue",
  judgeUnflag: "Marqueur retiré",
  viewerReject: "Rejeter (X)",
  viewerRejected: "Rejetée",
  favoriteMark: "Favori",
  viewerMutedHint: "Son coupé — cliquez pour l’activer",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "Passer en revue les photos rejetées",
  rejectGateTitle: (n: number) =>
    `Déplacer ${num(n)} ${un(n, "photo", "photos")} vers la Corbeille`,
  rejectGateNote:
    "Vous pouvez les restaurer depuis la Corbeille (elles reviennent aussi dans la bibliothèque).",
  rejectGateRestore: "Conserver",
  rejectGateBack: "Retour",
  rejectGateDiscard: "Fermer sans supprimer",
  rejectGateConfirm: (n: number) => `Mettre à la Corbeille (${num(n)})`,
  rejectGateTrashing: (done: number, total: number) =>
    `Déplacement… (${num(done)} / ${num(total)})`,
  updateFound: (v: string) => `La version ${v} est disponible`,
  updateOpenPage: "Ouvrir la page de téléchargement",
  updateLater: "Plus tard",
  updateCheckNow: "Rechercher des mises à jour",
  updateChecking: "Vérification…",
  updateUpToDate: "Vous avez la dernière version",
  updateFailed: "Vérification impossible",
  updateOnStart: "Rechercher des mises à jour au démarrage",
  updateOnStartNote:
    "Demande à GitHub le nom de la dernière version (une fois par jour). Aucune photo ni aucun nom de fichier n’est envoyé. Si vous désactivez cette option, rien ne quitte cet ordinateur, sauf lorsque vous cliquez sur « Rechercher des mises à jour ».",
  viewerSlideshow: "Diaporama (Espace)",
  // 抽出（Issue #13）
  extractSave: (key: string) => `Enregistrer cette image dans un fichier (${key})`,
  extractCopy: (key: string) => `Copier cette image dans le presse-papiers (${key})`,
  extractSaveTitle: "Enregistrer l’image sous",
  extractFilter: "Image",
  extractSaved: "Enregistrée",
  extractCopied: "Copiée",
  extractCopying: "Copie…",
  extractFailed: "Impossible d’extraire cette image",
  extractSameFile: "Impossible d’écraser le fichier d’origine",
  viewerExif: "Infos de la photo (I)",
  viewerFullscreen: "Plein écran (F11)",
  viewerClose: "Fermer (Échap)",
  viewerPrev: "Précédente (←)",
  viewerNext: "Suivante (→)",
  viewerFitToScreen: "Ajuster à l’écran (0)",
  viewerActualSize: "Taille réelle, 100 % (1) — ou double-clic",
  actualSizeBadge: "1:1",
  // 動画（第9部）
  videoUnsupported: "Ce format ne peut pas être lu dans l’application",
  fileMissing:
    "Ce fichier est introuvable (une clé USB ou une carte SD est peut-être débranchée, ou le fichier a été déplacé ou supprimé)",
  fileUnreachable:
    "Impossible d’ouvrir ce fichier (le lecteur n’est pas prêt, ou l’accès n’est pas autorisé)",
  fileNotShown:
    "Impossible d’afficher ce fichier (il est endommagé, ou pictkura ne sait pas lire ce format)",
  fileNotDownloaded:
    "Ce fichier n’a pas encore été téléchargé (rouvrez-le une fois connecté à Internet)",
  videoCloudOnly: "Cette vidéo se trouve dans le cloud",
  videoCloudOnlyNote:
    "La lire ici lance un téléchargement, et rien ne s’affiche tant qu’il n’est pas terminé. En l’ouvrant dans l’application par défaut, vous pouvez suivre la progression.",
  videoFailed: "Impossible de lire cette vidéo",
  videoOpenExternal: "Ouvrir dans l’application par défaut",
  videoCodecNote:
    "Les vidéos des iPhone et d’appareils similaires sont enregistrées en HEVC (H.265). Leur lecture nécessite un décodeur du système ; sous Windows, il s’agit d’une extension payante du Microsoft Store (quelques euros).",
  videoCodecNoteMac:
    "macOS lit nativement le HEVC ; il s’agit donc probablement d’un format d’enregistrement qu’il ne prend pas en charge.",
  videoCodecNoteOther:
    "Il manque peut-être sur votre système un décodeur pour ce format d’enregistrement.",
  videoCodecHelp: "Obtenir les Extensions vidéo HEVC (payantes)",
  loading: "Chargement…",
  exifTitle: "Infos de la photo",
  exifCamera: "Appareil",
  exifLens: "Objectif",
  exifAperture: "Ouverture",
  exifShutter: "Vitesse",
  exifIso: "ISO",
  exifFocal: "Focale",
  exifLocation: "Lieu",
  exifNone: "Aucune donnée EXIF",
  paletteInput: "Date, appareil, mot-clé ou commande…",
  paletteNoResults: "Aucun résultat",
  paletteGroupJumpDate: "Aller à une date",
  paletteGroupRecentDays: "Jours récents",
  paletteGroupCameras: "Filtrer par appareil",
  paletteGroupSearch: "Rechercher",
  paletteGroupActions: "Actions",
  paletteSearchFor: (q: string) => `Rechercher « ${q} »`,
  paletteSearchHint: "Nom de fichier, dossier, appareil",
  paletteSelect: "Sélectionner",
  paletteRun: "Exécuter",
  paletteCloseHint: "Fermer",
  actionShowFavorites: "Afficher uniquement les favoris",
  actionShowPicked: "Afficher uniquement les photos retenues",
  actionShowAll: "Afficher toutes les photos",
  actionCalendar: "Vue calendrier",
  actionThumbnails: "Grille de photos",
  indexBuilding: "🔍 Création de l’index de recherche… ",
  bgScanning: "Mise à jour de la bibliothèque…",
  bgThumbs: (n: number) => `Lecture des photos (encore ${num(n)})`,
  /* ヘッダーに出す短い形。全文（上の2つ）はマウスを乗せたときに出す（dev #64、2026-10-17 利用者の選択） */
  bgScanningShort: "Mise à jour",
  bgThumbsShort: (n: number) => `encore ${num(n)}`,
  cameraScanning: "📷 Lecture des informations de l’appareil… ",
  indexIncompleteWarning:
    "⚠ L’indexation a été interrompue — des résultats peuvent manquer (elle reprendra au prochain démarrage)",
  indexProgressSuffix: " % — tant que ce n’est pas terminé, des résultats peuvent manquer",
  removeRoot: (path: string) => `Retirer ${path} de la bibliothèque`,
  rootMissingNotice: (name: string, count: number) =>
    `Le dossier « ${name} » est introuvable${count > 0 ? ` (${num(count)} ${un(count, "photo qu’il contient ne peut pas être ouverte", "photos qu’il contient ne peuvent pas être ouvertes")})` : ""}. S’il se trouve sur une clé USB ou une carte SD, branchez-la, puis cliquez sur « Réanalyser ».`,
  rootsMissingNotice: (names: string, count: number) =>
    `Certains dossiers sont introuvables : ${names}${count > 0 ? ` (au total, ${num(count)} ${un(count, "photo ne peut pas être ouverte", "photos ne peuvent pas être ouvertes")})` : ""}. S’ils se trouvent sur une clé USB ou une carte SD, branchez-la, puis cliquez sur « Réanalyser ». Pour ne plus utiliser un dossier, retirez-le de la bibliothèque avec le ✕ de la liste de gauche (les fichiers des photos ne sont pas supprimés).`,
  rootRemoveFromLibrary: "Retirer de la bibliothèque",
  rootRemoveKeepsFiles:
    "Retire seulement le dossier de la bibliothèque. Les fichiers des photos ne sont pas supprimés",
  rootRemoveConfirm: (name: string) =>
    `Retirer « ${name} » de la bibliothèque ?\nLes fichiers des photos ne sont pas supprimés, mais les marques ★ et ⚑ de ses photos le seront.`,
  rootTempConfirm: (path: string) =>
    `« ${path} » se trouve dans un dossier temporaire.\nLe système ou d’autres applications peuvent y supprimer des fichiers sans prévenir. L’ajouter quand même à la bibliothèque ?`,
  rootTempConfirmOk: "Ajouter quand même",
  destTempConfirm: (path: string) =>
    `« ${path} » se trouve dans un dossier temporaire.\nLe système ou d’autres applications peuvent supprimer sans prévenir les photos importées ici. Une fois la carte effacée, elles n’existeront plus nulle part ailleurs. L’utiliser quand même comme destination ?`,
  destTempConfirmOk: "Utiliser comme destination",
  destTempWarning:
    "⚠ La destination se trouve dans un dossier temporaire. Les photos importées peuvent disparaître sans prévenir.",
  rootTempTip: (path: string) =>
    `${path} — se trouve dans un dossier temporaire. Le système ou d’autres applications peuvent y supprimer des fichiers sans prévenir`,
  rootMissingTempTip: (path: string) =>
    `${path} — introuvable. Il se trouvait dans un dossier temporaire ; le système ou une autre application l’a peut-être supprimé`,
  rootMissingTip: (path: string) =>
    `${path} — introuvable. S’il se trouve sur une clé USB ou une carte SD, branchez-la, puis cliquez sur « Réanalyser »`,
  importFrom: (path: string) => `Importer depuis ${path}`,
  filterByCamera: (name: string) => `Afficher uniquement les photos prises avec ${name}`,
  jumpToYear: (year: number) => `Aller à ${year}`,
  importing: (done: number, total: number) => `Importation… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `Importation terminée : ${num(copied)} ${un(copied, "copiée", "copiées")}, ${num(skipped)} ${un(skipped, "ignorée", "ignorées")}`,
  importFailed: (n: number) => `, ${num(n)} en échec`,
  importIncomplete:
    " ⚠ Certains dossiers n’ont pas pu être lus — n’effacez pas encore la carte",
  syncDone: (added: number, changed: number, removed: number) =>
    `${num(added)} ${un(added, "ajoutée", "ajoutées")}, ${num(changed)} ${un(changed, "modifiée", "modifiées")}, ${num(removed)} ${un(removed, "supprimée", "supprimées")}`,
  drivesReturnedScanned: (added: number, changed: number) =>
    `Dossiers du lecteur branché relus (${num(added)} ${un(added, "ajoutée", "ajoutées")}, ${num(changed)} ${un(changed, "modifiée", "modifiées")})`,
  pickSource: "Choisir le dossier à importer (USB / DCIM)",
  pickDestination: "Choisir le dossier de destination",
  wizardTitle: "Importation",
  wizardSources: "Source",
  wizardOtherFolder: "Autre dossier…",
  wizardRefresh: "Réanalyser les lecteurs",
  wizardRemovable: "Amovible",
  wizardNoDrives: "Aucun lecteur trouvé",
  emptyTitle: "Pas encore de photos",
  emptyTitleFailed: "Impossible d’afficher la liste",
  emptyTitleChecking: "Vérification",
  emptyTitleStartupFailed: "La synchronisation au démarrage ne s’est pas terminée",
  emptyStartupFailed:
    "La synchronisation lancée au démarrage ne s’est pas terminée. Il peut y avoir des photos qui ne sont pas encore dans la liste. Cliquez sur « Réanalyser » ; si cela ne suffit pas, rouvrez l’application.",
  emptyTitleMissing: "Certains emplacements sont introuvables",
  emptyTitleUnreadable: "Certains emplacements n’ont pas pu être ouverts",
  emptyNoRoots:
    "Aucun dossier de bibliothèque n’est encore configuré. Importez depuis une carte, ou choisissez un dossier qui contient des photos.",
  emptyMissing: (names: string) =>
    `Ces emplacements sont introuvables : ${names}. S’ils se trouvent sur une clé USB ou une carte SD, branchez-la, puis cliquez sur « Réanalyser ».`,
  emptyUnreadableMac: (names: string) =>
    `Ces emplacements n’ont pas pu être ouverts : ${names}. Dans Réglages Système → Confidentialité et sécurité, autorisez pictkura à accéder à ce dossier (Bureau, Documents, disque externe, etc.). S’il s’agit d’un dossier réseau, vérifiez qu’il est connecté, puis cliquez sur « Réanalyser ».`,
  emptyUnreadableWin: (names: string) =>
    `Ces emplacements n’ont pas pu être ouverts : ${names}. Vérifiez les autorisations du dossier. S’il s’agit d’un lecteur réseau, vérifiez qu’il est connecté, puis cliquez sur « Réanalyser ».`,
  emptyUnreadableOther: (names: string) =>
    `Ces emplacements n’ont pas pu être ouverts : ${names}. Vérifiez que vous avez le droit de les lire, puis cliquez sur « Réanalyser ».`,
  listSeparator: ", ",
  andMore: (n: number) => `et ${num(n)} ${un(n, "autre", "autres")}`,
  emptyRootIsPackage:
    "L’un des dossiers de la bibliothèque est lui-même une photothèque de l’application Photos. pictkura ne lit volontairement pas l’intérieur de ces photothèques ; aucune photo n’en proviendra donc jamais. Choisissez un dossier qui contient des photos, ou importez depuis une carte.",
  emptyPhotoLibrary:
    "Rien d’autre que la photothèque de l’application Photos n’a été trouvé. Par défaut, pictkura ne lit pas l’intérieur de Photos (la plupart des originaux sont dans iCloud, pas sur ce Mac). Importez depuis une carte, ou choisissez un dossier qui contient des photos.",
  emptyManagedLibrary:
    "Rien d’autre que des photothèques d’applications de gestion de photos (Photos, iPhoto, Aperture, etc.) n’a été trouvé. pictkura ne lit volontairement pas l’intérieur de ces photothèques (il pourrait en indexer le contenu une fois, mais ne verrait jamais les modifications ultérieures). Importez depuis une carte, ou choisissez un dossier qui contient des photos.",
  emptyRootIsManagedLibrary:
    "L’un des dossiers de la bibliothèque est lui-même la photothèque d’une application de gestion de photos (Photos, iPhoto, Aperture, etc.). pictkura ne lit volontairement pas l’intérieur de ces photothèques ; aucune photo n’en proviendra donc jamais. Choisissez un dossier qui contient des photos, ou importez depuis une carte.",
  emptyAllExcluded: (names: string) =>
    `Tout ce qui a été trouvé est ignoré par les règles d’exclusion (par exemple ${names}). Vous pouvez les modifier dans pictkura.toml, dans le dossier des paramètres.`,
  emptyNothingHere:
    "Aucune photo lisible par pictkura n’a encore été trouvée. Importez depuis une carte, ou choisissez un dossier qui contient des photos.",
  calendarChecking: "Vérification…",
  emptyTitleStalled: "Certains emplacements ne répondent pas",
  emptyStalled: (names: string) =>
    `Ces emplacements ne répondent pas : ${names}. S’il s’agit d’un dossier réseau, vérifiez qu’il est connecté, puis cliquez sur « Réanalyser ». S’il n’est plus disponible, retirez-le des dossiers de la bibliothèque pour voir les résultats des autres emplacements.`,
  emptyChecking:
    "Vérification des dossiers en cours. S’il s’agit d’un dossier réseau, vérifiez qu’il est connecté, puis cliquez sur « Réanalyser ».",
  emptyLoadFailed:
    "Impossible de charger la liste. Cliquez sur « Réanalyser », ou rouvrez l’application.",
  wizardPickFolderHint: "Choisissez un dossier à gauche pour voir les photos qu’il contient",
  wizardNoImages: "Aucune photo dans ce dossier",
  wizardUnreadable: "Impossible de lire ce dossier (il a peut-être été retiré)",
  wizardCounting: "Chargement…",
  wizardSelectAll: "Tout sélectionner",
  wizardSelectNew: "Nouvelles uniquement",
  wizardClearSelection: "Désélectionner",
  wizardSelected: (n: number) => `${num(n)} ${un(n, "sélectionnée", "sélectionnées")}`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "Déjà importée (le même fichier existe dans la destination)",
  wizardUnsureBadge: "?",
  wizardUnsureTitle:
    "Ce nom figure plusieurs fois sur la carte. Impossible de savoir si le fichier de la destination est cette photo avant d’en lire le contenu (l’importation le vérifie et ne crée pas de doublon d’une photo déjà présente)",
  wizardDestination: "Destination",
  wizardChangeDestination: "Modifier",
  wizardStructure: "Classement",
  wizardImportButton: (n: number) => `Importer (${num(n)})`,
  wizardImportAll: "Importer tout ce dossier (sous-dossiers compris)",
  wizardImportAllShort: "Dossier entier",
  wizardDeep: "Inclure les sous-dossiers",
  wizardDeepHint:
    "Parcourt tout le support, pour que vous n’ayez pas à savoir où se trouvent les photos",
  wizardScanning: "Exploration du support…",
  wizardTruncated: (n: number) =>
    `Seules les ${num(n)} premières sont affichées. Utilisez « Dossier entier » pour tout importer`,
  wizardScanIncomplete:
    "⚠ Certains dossiers n’ont pas pu être lus (des photos peuvent manquer)",
  decoderHeifNotice: (n: number) =>
    `⚠ ${num(n)} ${un(n, "photo HEIC/HEIF n’a", "photos HEIC/HEIF n’ont")} pas de miniature ici et ne ${un(n, "peut", "peuvent")} pas non plus être ${un(n, "ouverte", "ouvertes")}. Il faut les « Extensions d’image HEIF » (gratuites) et, en plus, les « Extensions vidéo HEVC » (payantes, quelques euros, sur le Microsoft Store), qui décodent les pixels`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ ${num(n)} ${un(n, "photo HEIC/HEIF n’a", "photos HEIC/HEIF n’ont")} pas de miniature ici et ne ${un(n, "peut", "peuvent")} pas non plus être ${un(n, "ouverte", "ouvertes")}`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ ${num(n)} ${un(n, "photo HEIC/HEIF n’a", "photos HEIC/HEIF n’ont")} pas de miniature ici et ne ${un(n, "peut", "peuvent")} pas non plus être ${un(n, "ouverte", "ouvertes")}. Il manque peut-être sur votre système un décodeur HEIC/HEVC`,
  decoderHeifHow: "Extensions d’image HEIF (gratuites)",
  decoderHevcHow: "Extensions vidéo HEVC (payantes)",
  decoderNoticeDismiss: "Ne plus afficher",
  wizardOfflineTitle:
    "Ce fichier se trouve dans le cloud (pas d’aperçu ici ; il sera téléchargé à l’importation)",
  wizardHideImported: "Masquer celles déjà importées",
  wizardAllImported: "Rien de nouveau (tout ce dossier est déjà importé)",
  wizardHiddenCount: (n: number) =>
    `${num(n)} ${un(n, "photo déjà importée masquée", "photos déjà importées masquées")}`,
  wizardEtaSeconds: (n: number) => `encore environ ${num(n)} s`,
  wizardEtaMinutes: (n: number) => `encore environ ${num(n)} min`,
  wizardEtaCalculating: "estimation du temps restant…",
  wizardCapped: (n: number) => `${num(n)}+`,
  wizardMoreFiles: (n: number) => `${num(n)} de plus (faites défiler pour charger)`,
  menuOpen: "Ouvrir",
  menuOpenWith: (name: string) => `Ouvrir avec ${name}`,
  menuOpenWithOther: "Ouvrir avec une autre application…",
  menuReveal: "Afficher dans le dossier",
  menuMoreOptions: "Afficher d’autres options",
  menuCopyImage: "Copier l’image dans le presse-papiers",
  menuRotateRight: "Faire pivoter à droite",
  menuRotateLeft: "Faire pivoter à gauche",
  menuRotateRestore: "Rétablir l’orientation d’origine",
  menuPrint: "Imprimer…",
  printTitle: "Imprimer",
  printFitContain: "Ajuster à la page",
  printFitContainHint:
    "La photo entière tient sur la page. Si ses proportions diffèrent de celles du papier, des marges restent.",
  printFitCover: "Recadrer pour remplir",
  printFitCoverHint: "Pas de marges, mais les parties de la photo qui dépassent sont coupées.",
  printFitStretch: "Remplir la page (sans respecter les proportions)",
  printFitStretchHint: "Pas de marges et rien n’est coupé, mais la photo est étirée.",
  printBorderless:
    "Pour imprimer sans bordure, choisissez un format de papier sans bordure dans la fenêtre d’impression suivante.",
  printGo: "Imprimer…",
  printPreparing: "Préparation de l’impression…",
  printSheetTitle: (n: number) => `Imprimer ${num(n)} ${un(n, "photo", "photos")}`,
  printPerPage: "Photos par page",
  printCaptions: "Ajouter le nom du fichier et la date de prise de vue",
  printTooMany: (max: number) =>
    `Avec ce nombre de photos par page, vous pouvez imprimer au maximum ${num(max)} ${un(max, "photo", "photos")} à la fois. Choisissez plus de photos par page, ou sélectionnez-en moins.`,
  printPreparingN: (done: number, total: number) =>
    `Préparation de l’impression… ${num(done)}/${num(total)}`,
  printSkipped: (n: number) =>
    `${num(n)} ${un(n, "élément n’a pas pu être imprimé", "éléments n’ont pas pu être imprimés")} (vidéos, par exemple) et ${un(n, "a été retiré", "ont été retirés")}.`,
  errPrint: "Impossible de préparer la photo pour l’impression.",
  errShellMenu: "Impossible d’afficher le menu de l’Explorateur.",
  menuDelete: "Supprimer (vers la Corbeille)",
  menuInfo: "Infos",
  fileInfoTitle: "Fichier",
  fileInfoName: "Nom",
  fileInfoFolder: "Emplacement",
  fileInfoSize: "Taille",
  fileInfoTaken: "Prise de vue",
  fileInfoTakenUnknown: "Inconnue",
  fileInfoModified: "Modifié",
  fileInfoDimensions: "Dimensions",
  fileInfoDuration: "Durée",
  fileInfoType: "Type",
  selectionTotal: (size: string) => `${size} au total`,
  menuFavoriteOn: "Ajouter aux favoris",
  menuFavoriteOff: "Retirer des favoris",
  pickEditor: "Choisir une application de retouche",
  deleteConfirm: (n: number) =>
    n === 1
      ? "Déplacer cette photo vers la Corbeille ?"
      : `Déplacer ${num(n)} ${un(n, "photo", "photos")} vers la Corbeille ?`,
  deleted: (n: number) => `${num(n)} ${un(n, "déplacée", "déplacées")} vers la Corbeille`,
  deletedSomeLeft: (n: number, left: number) =>
    `${num(n)} ${un(n, "déplacée", "déplacées")} vers la Corbeille (${num(left)} ${un(left, "introuvable, laissée", "introuvables, laissées")} en place)`,
  deletedMissing: (n: number, missing: number) =>
    `${num(n)} ${un(n, "déplacée", "déplacées")} vers la Corbeille (${num(missing)} ${un(missing, "n’était plus dans la bibliothèque et n’a pas été déplacée", "n’étaient plus dans la bibliothèque et n’ont pas été déplacées")})`,
  // 複数選択と一括操作
  selectItem: "Sélectionner",
  selectedCount: (n: number) => `${num(n)} ${un(n, "sélectionnée", "sélectionnées")}`,
  selectAll: "Tout sélectionner",
  clearSelection: "Désélectionner (Échap)",
  selectDay: "Sélectionner toute la journée",
  bulkFavoriteOn: "Ajouter aux favoris",
  bulkFavoriteOff: "Retirer des favoris",
  bulkDelete: "Mettre à la Corbeille",
  bulkCopy: "Copier dans un dossier",
  bulkMove: "Déplacer vers un dossier",
  bulkViewer: "Voir la sélection",
  pickExportFolder: "Choisir le dossier d’exportation",
  pickMoveFolder: "Choisir le dossier de destination du déplacement",
  moveConfirm: (n: number) =>
    n === 1
      ? "Déplacer cette photo vers un dossier que vous allez choisir ?\nElle quittera son emplacement actuel et la bibliothèque.\nLes marques ★ et ⚑ ne sont pas conservées."
      : `Déplacer ${num(n)} ${un(n, "photo", "photos")} vers un dossier que vous allez choisir ?\nElles quitteront leur emplacement actuel et la bibliothèque.\nLes marques ★ et ⚑ ne sont pas conservées.`,
  confirmCancel: "Annuler",
  deleteConfirmOk: "Mettre à la Corbeille",
  moveConfirmOk: "Choisir la destination",
  rootRemoveConfirmOk: "Retirer",
  exporting: (done: number, total: number, name: string) =>
    `Exportation… ${num(done)}/${num(total)} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, missing: number) => {
    const parts: string[] = [];
    // 書き出すものが無かったときに「0枚を書き出しました」から始めない（pictkura-dev #33）
    if (done > 0 || skipped + failed + missing === 0)
      parts.push(`${num(done)} ${un(done, "photo exportée", "photos exportées")}`);
    if (skipped > 0) parts.push(`${num(skipped)} ${un(skipped, "était déjà présente", "étaient déjà présentes")}`);
    if (failed > 0) parts.push(`${num(failed)} en échec`);
    if (missing > 0)
      parts.push(
        `${num(missing)} ${un(missing, "n’était plus dans la bibliothèque et n’a pas été exportée", "n’étaient plus dans la bibliothèque et n’ont pas été exportées")}`,
      );
    return parts.join(". ") + ".";
  },
  moving: (done: number, total: number, name: string) =>
    `Déplacement… ${num(done)}/${num(total)} ${name}`,
  moveDone: (moved: number, skipped: number, failed: number, leftBehind: number, missing: number) => {
    // 何も動かなかったときに「0枚を移動しました」から始めない（#159 のゲート2）
    const parts: string[] = [];
    if (moved > 0 || skipped + failed + leftBehind + missing === 0)
      parts.push(`${num(moved)} ${un(moved, "photo déplacée", "photos déplacées")}`);
    if (skipped > 0)
      parts.push(
        `${num(skipped)} ${un(skipped, "était déjà présente dans la destination et reste à son emplacement d’origine", "étaient déjà présentes dans la destination et restent à leur emplacement d’origine")}`,
      );
    if (failed > 0)
      parts.push(
        `${num(failed)} ${un(failed, "n’a pas pu être déplacée et reste à son emplacement d’origine", "n’ont pas pu être déplacées et restent à leur emplacement d’origine")}`,
      );
    if (leftBehind > 0)
      parts.push(
        `${num(leftBehind)} ${un(leftBehind, "a été copiée dans la destination, mais l’original n’a pas pu être supprimé", "ont été copiées dans la destination, mais les originaux n’ont pas pu être supprimés")}`,
      );
    if (missing > 0)
      parts.push(
        `${num(missing)} ${un(missing, "n’était plus dans la bibliothèque et n’a pas été déplacée", "n’étaient plus dans la bibliothèque et n’ont pas été déplacées")}`,
      );
    return parts.join(". ") + ".";
  },
  bulkPickOn: "Retenir",
  bulkPickOff: "Ne plus retenir",
  bulkPickDone: (n: number) => `${num(n)} ${un(n, "photo retenue", "photos retenues")}`,
  bulkUnpickDone: (n: number) =>
    `Marqueur retiré de ${num(n)} ${un(n, "photo", "photos")}`,
  bulkFavoriteDone: (n: number) =>
    `${num(n)} ${un(n, "photo ajoutée", "photos ajoutées")} aux favoris`,
  bulkUnfavoriteDone: (n: number) =>
    `${num(n)} ${un(n, "photo retirée", "photos retirées")} des favoris`,
  settings: "Paramètres",
  close: "Fermer",
  settingsTitle: "Paramètres",
  settingsImportStructure: "Structure des dossiers d’importation",
  settingsImportStructureNote:
    "Comment les photos importées sont classées selon leur date de prise de vue. Cela concerne des milliers de fichiers, et le changer plus tard demande un travail considérable. Lorsqu’un nom de dossier contient une date, la date s’écrit dans l’ordre année, mois, jour, quelle que soit la langue, afin que le tri par nom soit aussi un tri chronologique.",
  settingsDestination: "Destination",
  settingsDestinationUnset: "(non définie — vous la choisirez lors de la première importation)",
  settingsFlatExample: "IMG_0001.JPG (sans sous-dossiers)",
  settingsCustomPattern: "Personnalisé",
  settingsCustomPatternNote:
    "{year} {month} {day} sont remplacés par la date. Le caractère / crée des sous-dossiers. Les caractères non autorisés et les remontées vers le dossier parent (..) sont supprimés automatiquement.",
  settingsCustomPatternResult: "Dossier obtenu",
  settingsGrid: "Grille de photos",
  settingsStackRawJpegToggle: "Empiler les paires RAW+JPEG en une seule vignette",
  settingsStackRawJpegNote:
    "Un RAW et un JPEG portant le même nom dans le même dossier s’affichent en une seule vignette, celle du JPEG. Dans la grille, ★, ⚑, la suppression et la sélection s’appliquent aux deux. Le fichier montré en grand se règle sous « Lorsque vous affichez une photo en grand ».",
  stackRawChipTitle: (files: number) =>
    `RAW et JPEG empilés en une seule vignette (${num(files)} ${un(files, "fichier", "fichiers")})`,
  settingsStackBurstsToggle: "Empiler les rafales en une seule vignette",
  settingsStackBurstsNote:
    "Les photos prises en succession rapide avec le même appareil s’affichent en une seule vignette. Seules les photos dont l’heure de prise de vue est enregistrée à la fraction de seconde près sont concernées (certains appareils ne l’enregistrent pas). Dans la grille, ★ et ⚑ posés depuis le menu du clic droit ne s’appliquent qu’à la photo de couverture ; la suppression et la sélection s’appliquent à toute la rafale. En affichage en grand, les rafales ne sont pas regroupées : on les parcourt photo par photo.",
  settingsBurstGap: "Écart maximal au sein d’une rafale",
  settingsOutgoing: "Envoi",
  settingsGoogleToggle: "Placer aussi les photos importées dans le dossier Envoi",
  settingsGoogleNote:
    "À chaque importation, les photos sont aussi placées dans le dossier Envoi (sans occuper d’espace disque supplémentaire). Pour les paires RAW+JPEG, seul le JPEG y est placé. Si vous utilisez Google Photos : ajoutez ce dossier à la sauvegarde de dossiers de Google Photos.",
  settingsGoogleFolder: "Dossier",
  settingsGoogleOpenFolder: "Ouvrir le dossier",
  settingsGoogleCopyPath: "Copier le chemin",
  settingsGoogleCopied: "Copié",
  settingsGoogleCopyFailed: "Impossible de copier. Sélectionnez le chemin et copiez-le.",
  settingsGoogleFolderNote:
    "Il se trouve sur le même lecteur que la destination de l’importation. Choisissez un emplacement en dehors des dossiers surveillés par une autre sauvegarde, comme un NAS ou Amazon Photos.",
  settingsGoogleFoldersNote:
    "Dossiers Envoi dans lesquels pictkura a déjà placé des photos (un par lecteur). Si vous utilisez Google Photos : ajoutez chacun de ceux que vous utilisez à la sauvegarde de dossiers.",
  settingsGoogleVideo: "Placer aussi les vidéos",
  settingsGoogleRawOnly: "Prises de vue en RAW seul",
  googleRawOnlyNone: "Ne pas placer",
  googleRawOnlyEmbedded: "Placer le JPEG intégré",
  settingsGoogleRawOnlyNote:
    "Ce qu’il faut faire des RAW pris sans JPEG. « JPEG intégré » extrait l’image d’aperçu que l’appareil a enregistrée dans le RAW et la place avec la date de prise de vue. Le JPEG extrait est un fichier distinct ; il occupe donc de l’espace disque.",
  settingsGoogleOneDrive: "Placer aussi les photos qui sont dans OneDrive",
  settingsGoogleOneDriveNote:
    "Dans ce cas, OneDrive ne pourra plus rendre ces photos « disponibles en ligne uniquement ».",
  settingsGoogleAfterNote:
    "Lorsque vous mettez une photo à la Corbeille dans pictkura, elle est aussi retirée du dossier Envoi. Si vous utilisez Google Photos : elle reste dans Google Photos. L’importation dans Google Photos est effectuée par sa page web (photos.google.com) ; elle n’avance donc que lorsque cette page est ouverte dans votre navigateur.",
  pickGoogleFolder: "Choisir le dossier Envoi",
  importGoogle: (n: number) => ` · ${num(n)} ${un(n, "placé", "placés")} dans Envoi`,
  importGoogleFailed: (n: number) =>
    `${num(n)} ${un(n, "élément n’a pas pu être placé", "éléments n’ont pas pu être placés")} dans Envoi (voir le journal)`,
  importGoogleError: (why: string) => `Rien n’a été placé dans Envoi : ${why}`,
  bulkSendGoogle: "Placer dans Envoi",
  bulkRemoveGoogle: "Retirer d’Envoi",
  calendarSendMonth: "Placer ce mois dans Envoi",
  /** 年は数ではなく名前（2,010年にしない）——文字列で受ける */
  calendarSendYear: (year: string) => `Placer toute l’année ${year} dans Envoi`,
  googleSendSummary: (photos: number, videos: number, rawOnly: number, size: string) => {
    const parts: string[] = [];
    if (photos > 0) parts.push(`${num(photos)} ${un(photos, "photo", "photos")}`);
    if (videos > 0) parts.push(`${num(videos)} ${un(videos, "vidéo", "vidéos")}`);
    const items: string[] = [];
    if (parts.length > 0) items.push(`${parts.join(", ")}${size ? ` (${size})` : ""}`);
    if (rawOnly > 0)
      items.push(`${num(rawOnly)} JPEG ${un(rawOnly, "extrait", "extraits")} de prises de vue en RAW seul`);
    return `À placer dans le dossier Envoi : ${items.join(" ; ")}.`;
  },
  googleSendLeftOut: (n: number) =>
    `${num(n)} des éléments choisis ${un(n, "n’est pas placé", "ne sont pas placés")} (le RAW d’une paire RAW+JPEG, les fichiers dans OneDrive, etc.).`,
  googleSendFolders: (list: string) => `Emplacement : ${list}`,
  googleSendNewFolder: (path: string) => `${path} sera créé.`,
  googleSendGoogleNote:
    "Si vous utilisez Google Photos : l’importation dans Google Photos est effectuée par sa page web (photos.google.com) ; elle avance tant que cette page est ouverte dans votre navigateur.",
  googleSendGoogleNoteNew:
    "Si vous utilisez Google Photos : ajoutez aussi chaque nouveau dossier à la sauvegarde de dossiers de Google Photos. L’importation dans Google Photos est effectuée par sa page web (photos.google.com) ; elle avance tant que cette page est ouverte dans votre navigateur.",
  googleFolderCreated: (path: string) =>
    `${path} a été créé. Si vous utilisez Google Photos : ajoutez aussi ce dossier à la sauvegarde de dossiers.`,
  googleSendNothing: "Rien ici ne peut être placé.",
  googleSendConfirmOk: "Placer",
  googleSent: (total: number, already: number) =>
    `${num(total)} ${un(total, "placé", "placés")} dans Envoi${already > 0 ? ` (dont ${num(already)} déjà ${un(already, "présent", "présents")})` : ""}`,
  googleSentCloudOnly: (n: number) =>
    `${num(n)} ${un(n, "élément n’est que dans le cloud et n’a pas pu être placé", "éléments ne sont que dans le cloud et n’ont pas pu être placés")}. ${un(n, "Téléchargez-le", "Téléchargez-les")} d’abord, puis réessayez.`,
  googleRemoved: (n: number) => `${num(n)} ${un(n, "retiré", "retirés")} d’Envoi`,
  googleRemoveFailed: (n: number) =>
    `${num(n)} ${un(n, "élément n’a pas pu être retiré", "éléments n’ont pas pu être retirés")} pour l’instant (le lecteur n’est peut-être pas connecté). pictkura réessaiera au prochain démarrage.`,
  googleSentLeftOut: (n: number) =>
    `${num(n)} ${un(n, "élément n’a pas été placé", "éléments n’ont pas été placés")} (le RAW d’une paire RAW+JPEG, les fichiers dans OneDrive, etc.).`,
  burstGapOption: (seconds: number) => `${num(seconds)} s`,
  burstChip: (frames: number) => `▤ Rafale ${num(frames)}`,
  burstChipShort: (frames: number) => `▤ ${num(frames)}`,
  spreadStack: "Déplier pour voir chaque photo",
  foldStack: "Replier",
  burstTitle: (frames: number, spanMs: number, files: number) =>
    `Rafale de ${num(frames)} ${un(frames, "vue", "vues")} en ${num(Math.round(spanMs / 100) / 10)} s (${num(files)} ${un(files, "fichier", "fichiers")})`,
  burstTitleRawJpeg: (frames: number, spanMs: number, files: number) =>
    `Rafale de ${num(frames)} ${un(frames, "vue", "vues")} en ${num(Math.round(spanMs / 100) / 10)} s, RAW+JPEG (${num(files)} ${un(files, "fichier", "fichiers")})`,
  settingsViewer: "Lorsque vous affichez une photo en grand",
  settingsAutoAdvanceToggle: "Passer à la photo suivante après P / U",
  settingsAutoAdvanceNote:
    "En affichage en grand, P marque la photo d’un ⚑ (une catégorie distincte des ★ favoris) et U retire le marqueur. Avec cette option, la photo suivante s’affiche aussitôt : le tri se fait en une touche par photo. Sans elle, vous restez sur la même photo.",
  settingsPairView: "Affichage des paires RAW+JPEG",
  pairViewJpeg: "JPEG seul",
  pairViewRaw: "RAW seul",
  pairViewBoth: "Les deux (RAW, puis JPEG)",
  settingsPairViewNote:
    "Comment l’affichage en grand parcourt une paire regroupée en une seule vignette dans la grille. Si un seul des deux est montré, ★, ⚑, ✕ et la suppression en grand s’appliquent aux deux fichiers de la paire. Si les deux sont montrés, ils ne s’appliquent qu’au fichier affiché. Les boutons RAW / JPEG de l’affichage en grand permettent aussi de changer. Si les paires ne sont pas regroupées dans la grille, les deux fichiers apparaissent.",
  viewerPairSide: "Quel fichier de la paire afficher (également enregistré dans les paramètres)",
  settingsTransition: "Passage à la photo suivante",
  settingsSlideshowTransition: "Transition du diaporama",
  transitionNone: "Aucune",
  transitionSlide: "Glissement",
  transitionFade: "Fondu",
  settingsTransitionNote:
    "L’effet utilisé quand l’affichage en grand passe à la photo suivante (touches fléchées, boutons, bandeau du bas ou passage automatique après un marquage) et quand un diaporama avance. Le glissement suit le sens dans lequel vous avancez. Si le système est réglé pour réduire les animations, aucun mouvement n’a lieu, quel que soit votre choix.",
  settingsAutoplay: "Lorsque vous insérez une clé USB ou une carte SD",
  settingsAutoplayNote:
    "Si pictkura a été installé avec le programme d’installation ou depuis le Microsoft Store, « pictkura で写真を取り込む » (importer des photos avec pictkura) apparaît parmi les choix lorsque vous branchez un lecteur ; la version portable ne le propose pas. pictkura ne démarre pas tant que vous ne l’avez pas choisi. Une fois ce choix fait, Windows le retient comme action par défaut pour ce type de support (les cartes mémoire et les clés USB sont réglées séparément) et ouvre la fenêtre d’importation chaque fois que vous insérez un support du même type. L’action par défaut se modifie dans les paramètres d’exécution automatique de Windows. L’entrée elle-même est libellée en japonais, sauf avec le MSI anglais, où elle s’intitule « Import photos with pictkura ».",
  settingsAutoplayOpenWindows: "Ouvrir les paramètres d’exécution automatique de Windows",
  settingsImportWaitingFrom: (path: string) =>
    `Une importation depuis ${path} est en attente. Elle s’ouvrira à la fermeture des paramètres.`,
  settingsImportWaiting: "Une importation est en attente. Elle s’ouvrira à la fermeture des paramètres.",
  settingsImportWaitingOpen: "Fermer et importer",
  settingsAbout: "À propos",
  settingsAboutLicense: "Distribué sous licence MIT.",
  settingsManual: "Mode d’emploi",
  settingsOssLicenses: "Logiciels open source utilisés",
  settingsDocNotBundled: "(non inclus dans une version de développement)",
  settingsLog: "Ouvrir le journal",
  settingsLogNone: "(rien n’a encore été enregistré)",
  settingsOpenFailed: "Impossible de l’ouvrir.",
  settingsLogNote:
    "Une ligne n’est écrite qu’en cas d’échec, et elle reste sur cet ordinateur. Rien n’est envoyé.",
  settingsLanguage: "Langue",
  settingsLanguageSystem: "Comme le système",
  settingsLanguageNote:
    "Changer de langue recharge la fenêtre. La copie continue en arrière-plan, mais l’assistant d’importation se ferme et vous ne voyez plus sa progression ; attendez donc la fin d’une importation avant de changer.",
  settingsLanguageMacNote:
    "Les fenêtres de macOS, comme le panneau d’enregistrement et les demandes de confirmation, passeront à cette langue au prochain démarrage de pictkura.",
  settingsTheme: "Thème",
  themeSystem: "Comme le système",
  themeLight: "Clair",
  themeDark: "Sombre",
  settingsEditors: "Applications de retouche",
  settingsEditorsNote:
    "Applications que vous avez choisies dans « Ouvrir avec une autre application… ».",
  settingsForgetEditor: "Retirer de la liste",
  calendarEmpty: "Aucune photo",
  filterEmptyTitle: (conds: string) => `Aucune photo ne correspond à ces filtres : ${conds}`,
  filterEmptyMessage:
    "Effacez la recherche ou le filtre choisi à gauche pour voir les autres photos.",
  filterEmptyStillIndexing:
    "L’importation ou l’indexation est encore en cours ; d’autres photos peuvent apparaître.",
  filterCondQuery: (q: string) => `la recherche « ${q} »`,
  filterCondCamera: (name: string) => `prises avec ${name}`,
  filterEmptyClearSearch: "Effacer la recherche",
  errGoogleOverlapsRoot:
    "Vous ne pouvez pas choisir un dossier de la bibliothèque, un dossier qu’il contient, ni un dossier qui le contient.",
  errGoogleInsideSync:
    "Vous ne pouvez pas choisir un emplacement situé dans un dossier synchronisé (OneDrive, Google Drive, Dropbox, iCloud Drive, etc.).",
  errGoogleNoHardLinks:
    "Impossible sur ce lecteur (exFAT, FAT et systèmes similaires ne peuvent pas créer de liens physiques).",
  errGoogleWholeVolume:
    "La destination de l’importation est un lecteur entier ; il n’y a donc aucun endroit où placer le dossier Envoi. Changez la destination de l’importation.",
  errGoogleNoName: "Vous ne pouvez pas choisir un lecteur lui-même. Choisissez un dossier qu’il contient.",
  errGoogleLinkInTheWay: "Le dossier Envoi est un lien symbolique. Choisissez un autre emplacement.",
  errGoogleIo: "Impossible de lire ou d’écrire dans le dossier Envoi.",
  errGoogleOtherDrive: "Choisissez un dossier sur le même lecteur que la destination de l’importation.",
  errNotFound: "Cette photo ne figure plus dans l’index.",
  errRotateNoOrientation:
    "Cette photo ne contient aucune information d’orientation ; impossible de la faire pivoter.",
  errRotateXmp:
    "Cela contredirait l’orientation que les logiciels de développement lisent dans le XMP ; impossible de la faire pivoter.",
  errRotateCloudOnly:
    "Cette photo n’est que dans le cloud ; impossible de la faire pivoter. Téléchargez-la, puis réessayez.",
  errRotateCameraCard:
    "Cette photo se trouve sur une carte d’appareil photo, protégée car elle sert de source d’importation ; impossible de la faire pivoter. Importez-la, puis faites-la pivoter.",
  errRotateNotReady: "Lecture des informations de la photo en cours. Patientez un instant, puis réessayez.",
  errRotateChanged:
    "Le fichier a été modifié ailleurs pendant la rotation. Réessayez.",
  errRotateBusy: "Vérification des dossiers en cours. Patientez un instant, puis réessayez.",
  errRotateWrite:
    "Impossible d’écrire dans le fichier ; la photo n’a pas pivoté. Vérifiez qu’aucune autre application ne l’a ouvert et qu’il n’est pas en lecture seule.",
  errRotateLeftChanged:
    "Un problème est survenu après la réécriture du fichier, qui n’a pas pu être rétabli. La photo a peut-être pivoté malgré tout. Consultez le journal pour en savoir plus.",
  errRotateCannotRestore:
    "Cette photo ne peut plus retrouver l’orientation qu’elle avait avant que pictkura ne la fasse pivoter, par exemple parce qu’une autre application l’a modifiée.",
  errDb: "Impossible de lire ou d’écrire l’index.",
  errConfigIo: "Impossible de lire ou d’écrire le fichier de paramètres.",
  errConfigFormat: "Impossible de comprendre le contenu du fichier de paramètres.",
  errNoDestination: "Aucun dossier de destination n’a encore été choisi.",
  errRootManaged:
    "Une photothèque gérée par une application ne peut pas être ajoutée comme dossier (son contenu est constitué de fichiers internes).",
  errDestManaged:
    "Une photothèque gérée par une application ne peut pas servir de destination (son contenu est constitué de fichiers internes).",
  errNoImageToExtract: "Cette photo ne contient aucune image à extraire.",
  errSourceUnreadable: "Impossible de lire le dossier source.",
  errSourceManaged:
    "Impossible d’importer depuis l’intérieur d’une photothèque gérée par une application. Retirez l’extension (.photoslibrary, etc.) du nom du dossier, puis choisissez-le de nouveau.",
  errExportDest: "Impossible de créer le dossier d’exportation.",
  errExportManaged: "Impossible d’exporter à cet endroit (c’est un paquet géré par une application).",
  errFolderMissing: "Dossier introuvable.",
  fileOpBusy: "Attendez la fin de l’opération en cours, puis réessayez.",
  errFolderNotAbsolute: "Saisissez le chemin complet du dossier.",
  errTrashFailed: "Impossible de déplacer vers la Corbeille.",
  errTrashUnreachable:
    "Le lecteur ou le dossier est introuvable ; rien n’a été déplacé vers la Corbeille.",
  errTrashPartly: (n: number) =>
    `${num(n)} ${un(n, "élément déplacé", "éléments déplacés")} vers la Corbeille ; le reste n’a pas pu l’être.`,
  errNoLogYet: "Rien n’a encore été enregistré.",
  errOutgoingFolderMissing:
    "Le dossier Envoi est introuvable. Il est créé lorsque vous activez Envoi (s’il est déjà activé, désactivez-le puis réactivez-le).",
  errOpenOutgoingFailed:
    "Impossible d’ouvrir le dossier Envoi. Utilisez « Copier le chemin » et ouvrez-le depuis là.",
  errNotBundled: "Non inclus dans cette version.",
  errNoStoreLink: "Aucune source d’installation à proposer pour ce système.",
  errBadKind: "Impossible de déterminer comment l’ouvrir.",
  errTooManyIds: (n: number) => `${num(n)} au maximum à la fois.`,
};
