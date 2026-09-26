/** 英語辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。 */
import { folderExample } from "./folderExample.ts";
import { num, one } from "./plural.ts";
import type { Dict } from "./ja.ts";

export const en: Dict = {
  appName: "pictkura",
  viewThumbnails: "Photos",
  viewCalendar: "Calendar",
  searchPlaceholder: "Search files, folders, cameras, 2019-08, or year:2019",
  searchClear: "Clear search (Esc)",
  commandPalette: "Command palette",
  importFromUsb: "Import from USB",
  rescan: "Rescan",
  size: "Size",
  /**
   * 一覧の件数。**数と単位を1つのキーにする**（2026-09-02、ゲート2の指摘）。
   * 呼ぶ側で `${formatNumber(n)} {t.itemsSuffix}` と組んでいたので、
   * **1枚に絞ると「1 items」「1 Objekte」**と出ていた——このPRが潰しに来た
   * `full scan (1 files)` と同じ壊れ方が、画面で一番目立つ数字に残っていた。
   * 単位だけのキーでは、どの言語も単数形を書けない。
   */
  itemsCount: (n: number) => `${num(n)} ${one(n, "item", "items")}`,
  navPlaces: "Places",
  navAllPhotos: "All photos",
  navFavorites: "★ Favorites",
  navPicked: "⚑ Picked",
  navKinds: "Kind",
  kindPhoto: "Photos",
  kindRaw: "RAW",
  kindVideo: "Videos",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "Keyboard shortcuts (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "Show keyboard shortcuts",
  shortcutGroups: [
    {
      title: "Grid",
      keys: [
        ["Ctrl+K / ⌘K", "Command palette (jump to a date or camera, search, import)"],
        ["Ctrl+, / ⌘,", "Open Settings (the same panel the toolbar gear opens)"],
        ["Ctrl+A / ⌘A", "Select everything the current search and filter match"],
        ["Shift + click", "Select everything between the last tile you clicked and this one"],
        ["Ctrl + click", "Add or remove one photo (⌘ + click on macOS)"],
        ["Click a date heading", "Select that whole day (click again to clear)"],
        ["Esc", "Stop selecting"],
      ],
    },
    {
      title: "Viewer",
      keys: [
        ["← / →", "Previous / next photo"],
        ["P", "Pick it (⚑). By default this moves on to the next photo"],
        ["X", "Reject it (✕). Rejected photos go to the trash when you close the viewer"],
        ["U", "Undo the judgement on this photo (clear ⚑ and ✕)"],
        ["Ctrl+C / ⌘C", "Copy the picture on screen to the clipboard"],
        ["Ctrl+S / ⌘S", "Save the picture on screen to a file"],
        ["F", "Toggle favorite (★)"],
        ["I", "Capture details (camera, lens, aperture, ISO, GPS)"],
        ["Space", "Slideshow. On a video, play / pause"],
        ["1 / 0", "1 toggles actual size 100% ⇔ fit to screen; 0 always fits"],
        ["F11", "Full screen"],
        ["Esc", "Close"],
      ],
    },
    {
      title: "Mouse (in the viewer)",
      keys: [
        ["Double-click", "Actual size 100% ⇔ fit to screen"],
        ["Wheel", "Zoom in / out"],
        ["Drag", "Move around while zoomed in"],
        ["Right-click", "Open / Open with another app… / Show in folder / Add to favorites / Pick / Delete (move to trash)"],
        ["Click the strip", "Jump to that photo"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "Cameras & media",
  navLibraryFolders: "Library folders",
  navDrives: "Drives",
  navAddFolder: "Add a folder",
  add: "Add",
  browse: "Browse…",
  addFolderPlaceholder: folderExample("e.g. ", "you"),
  pickLibraryFolder: "Choose a folder to add to the library",
  showMore: (n: number) => `${num(n)} more`,
  collapse: "Show less",
  photosCount: (n: number) => `${num(n)}`,
  memoriesTitle: (years: number) =>
    years === 1 ? "1 year ago today" : `${num(years)} years ago today`,
  viewerFavorite: "Favorite (F)",
  viewerPick: "Pick (P)",
  viewerUnpick: "Unpick (U)",
  viewerPicked: "Picked photo",
  judgeFav: "Favorite",
  judgeUnfav: "Favorite removed",
  judgePick: "Picked",
  judgeUnflag: "Judgement cleared",
  viewerReject: "Reject (X)",
  viewerRejected: "Rejected",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "Review the rejected photos",
  rejectGateTitle: (n: number) =>
    n === 1 ? "Move 1 photo to the trash" : `Move ${num(n)} photos to the trash`,
  rejectGateNote: "You can restore them from the trash (they come back to the library, too).",
  rejectGateRestore: "Keep",
  rejectGateBack: "Back",
  rejectGateDiscard: "Close without deleting",
  rejectGateConfirm: (n: number) => (n === 1 ? "Move 1 to trash" : `Move ${num(n)} to trash`),
  rejectGateTrashing: (done: number, total: number) =>
    `Moving… (${num(done)} / ${num(total)})`,
  updateFound: (v: string) => `Version ${v} is available`,
  updateOpenPage: "Open the download page",
  updateLater: "Later",
  updateCheckNow: "Check for updates",
  updateChecking: "Checking…",
  updateUpToDate: "You are up to date",
  updateFailed: "Could not check",
  updateOnStart: "Check for updates at startup",
  updateOnStartNote:
    "Asks GitHub for the latest version name (once a day). No photos or file names are sent. Turn it off and nothing leaves this machine except when you press “Check for updates”.",
  viewerSlideshow: "Slideshow (Space)",
  // 抽出（Issue #13）
  extractSave: (key: string) => `Save this picture to a file (${key})`,
  extractCopy: (key: string) => `Copy this picture to the clipboard (${key})`,
  extractSaveTitle: "Save picture as",
  extractFilter: "Image",
  extractSaved: "Saved",
  extractCopied: "Copied",
  extractFailed: "Could not extract this picture",
  extractSameFile: "Cannot save over the original file",
  viewerExif: "Photo info (I)",
  viewerFullscreen: "Full screen (F11)",
  viewerClose: "Close (Esc)",
  viewerPrev: "Previous (←)",
  viewerNext: "Next (→)",
  viewerFitToScreen: "Fit to screen (0)",
  viewerActualSize: "Actual size, 100% (1) — or double-click",
  actualSizeBadge: "1:1",
  // 動画（第9部）
  videoUnsupported: "This format cannot be played in the app",
  fileMissing: "This file is missing (a USB stick or SD card may be unplugged, or the file may have been moved or deleted)",
  fileUnreachable: "This file cannot be opened (the drive is not ready, or access is not allowed)",
  fileNotShown: "This file cannot be displayed (the file is damaged, or pictkura cannot read this format)",
  fileNotDownloaded: "This file has not been downloaded yet (open it again once you are online)",
  videoCloudOnly: "This video lives in the cloud",
  videoCloudOnlyNote:
    "Playing it here starts a download and shows nothing until it finishes. Opening it in the default app lets you watch the download progress.",
  videoFailed: "Could not play this video",
  videoOpenExternal: "Open in default app",
  videoCodecNote:
    "Videos from iPhones and similar cameras use HEVC (H.265). Playback needs an OS decoder; on Windows that is a paid extension from the Microsoft Store (a few dollars).",
  videoCodecNoteMac:
    "macOS decodes HEVC out of the box, so this is most likely a recording format it does not handle.",
  videoCodecNoteOther:
    "Your system may not have a decoder for this recording format.",
  videoCodecHelp: "Get the HEVC Video Extensions (paid)",
  loading: "Loading…",
  exifTitle: "Photo info",
  exifCamera: "Camera",
  exifLens: "Lens",
  exifAperture: "Aperture",
  exifShutter: "Shutter",
  exifIso: "ISO",
  exifFocal: "Focal length",
  exifLocation: "Location",
  exifNone: "No EXIF data",
  paletteInput: "Date, camera, keyword, or command…",
  paletteNoResults: "No results",
  paletteGroupJumpDate: "Jump to date",
  paletteGroupRecentDays: "Recent days",
  paletteGroupCameras: "Filter by camera",
  paletteGroupSearch: "Search",
  paletteGroupActions: "Actions",
  paletteSearchFor: (q: string) => `Search for “${q}”`,
  paletteSearchHint: "File name, folder, camera",
  paletteSelect: "Select",
  paletteRun: "Run",
  paletteCloseHint: "Close",
  actionShowFavorites: "Show favorites only",
  actionShowPicked: "Show picked only",
  actionShowAll: "Show all photos",
  actionCalendar: "Calendar view",
  actionThumbnails: "Photo grid",
  indexBuilding: "🔍 Building the search index… ",
  cameraScanning: "📷 Reading camera info… ",
  indexIncompleteWarning:
    "⚠ Search indexing was interrupted — results may be incomplete (it resumes on next launch)",
  indexProgressSuffix: "% — results may be incomplete until this finishes",
  removeRoot: (path: string) => `Remove ${path} from the library`,
  rootMissingNotice: (name: string, count: number) =>
    `The folder “${name}” is not there${count > 0 ? ` (${num(count)} ${one(count, "photo", "photos")} in it cannot be opened)` : ""}. If it is on a USB stick or SD card, plug it in and press Rescan.`,
  rootsMissingNotice: (names: string, count: number) =>
    `Some folders are not there: ${names}${count > 0 ? ` (${num(count)} ${one(count, "photo", "photos")} in them cannot be opened)` : ""}. If they are on a USB stick or SD card, plug it back in and press Rescan. To stop using a folder, remove it from the library with the ✕ in the list on the left (the photo files are not deleted).`,
  rootRemoveFromLibrary: "Remove from library",
  rootRemoveKeepsFiles: "Only removes the folder from the library. The photo files are not deleted",
  rootRemoveConfirm: (name: string) =>
    `Remove “${name}” from the library?\nThe photo files are not deleted, but the ★ and ⚑ marks on its photos are.`,
  rootTempConfirm: (path: string) =>
    `“${path}” is inside a temporary folder.\nThe system or other apps may delete files there without asking. Add it to the library anyway?`,
  rootTempConfirmOk: "Add anyway",
  destTempConfirm: (path: string) =>
    `“${path}” is inside a temporary folder.\nPhotos imported here may be deleted by the system or other apps without asking. Once the card is erased, they will not exist anywhere else. Use it as the destination anyway?`,
  destTempConfirmOk: "Use as destination",
  destTempWarning:
    "⚠ The destination is inside a temporary folder. Imported photos may disappear without warning.",
  rootTempTip: (path: string) =>
    `${path} — inside a temporary folder. The system or other apps may delete files there without asking`,
  rootMissingTempTip: (path: string) =>
    `${path} — not there. It was inside a temporary folder, so the system or another app may have deleted it`,
  rootMissingTip: (path: string) =>
    `${path} — not there. If it is on a USB stick or SD card, plug it in and press Rescan`,
  importFrom: (path: string) => `Import from ${path}`,
  filterByCamera: (name: string) => `Show only photos taken with ${name}`,
  jumpToYear: (year: number) => `Jump to ${year}`,
  importing: (done: number, total: number) => `Importing… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `Import finished: ${num(copied)} copied, ${num(skipped)} skipped`,
  importFailed: (n: number) => `, ${num(n)} failed`,
  importIncomplete: " ⚠ Some folders could not be read — do not erase the card yet",
  syncDone: (added: number, changed: number, removed: number) =>
    `${num(added)} added, ${num(changed)} changed, ${num(removed)} removed`,
  drivesReturnedScanned: (added: number, changed: number, removed: number) =>
    `Read the folders on the inserted drive again (${num(added)} added, ${num(changed)} changed, ${num(removed)} removed)`,
  pickSource: "Choose the folder to import from (USB / DCIM)",
  pickDestination: "Choose the destination folder",
  wizardTitle: "Import",
  wizardSources: "Source",
  wizardOtherFolder: "Other folder…",
  wizardRefresh: "Rescan drives",
  wizardRemovable: "Removable",
  wizardNoDrives: "No drives found",
  emptyTitle: "No photos yet",
  emptyTitleFailed: "The list could not be shown",
  emptyTitleChecking: "Checking",
  emptyTitleStartupFailed: "The startup sync did not finish",
  emptyStartupFailed:
    "The sync that runs at startup did not finish. There may be photos that have not been taken in yet. Press Rescan; if that does not help, reopen the app.",
  emptyTitleMissing: "Some places are not there",
  emptyTitleUnreadable: "Some places could not be opened",
  emptyNoRoots:
    "No library folder has been set up yet. Import from a card, or pick a folder that has photos in it.",
  emptyMissing: (names: string) =>
    `These places are not there: ${names}. If they are on a USB stick or SD card, plug it back in and press Rescan.`,
  emptyUnreadableMac: (names: string) =>
    `These places could not be opened: ${names}. Grant pictkura access to that folder (Desktop, Documents, an external drive) in System Settings → Privacy & Security. If it is on a network, make sure it is connected and press Rescan.`,
  emptyUnreadableWin: (names: string) =>
    `These places could not be opened: ${names}. Check the folder's permissions. If it is a network drive, make sure it is connected and press Rescan.`,
  emptyUnreadableOther: (names: string) =>
    `These places could not be opened: ${names}. Check that you have permission to read them, then press Rescan.`,
  listSeparator: ", ",
  andMore: (n: number) => `and ${num(n)} more`,
  emptyRootIsPackage:
    "One of the library folders is a Photos app library itself. pictkura deliberately does not read inside one, so nothing will ever come from it. Pick a folder that has photos in it, or import from a card.",
  emptyPhotoLibrary:
    "Nothing was found but the Photos app library. pictkura does not read inside Photos by default — most of the originals live in iCloud, not on this Mac. Import from a card, or pick a folder that has photos in it.",
  emptyManagedLibrary:
    "Nothing was found but photo-manager libraries (Photos, iPhoto or Aperture). pictkura deliberately does not read inside one — it would index the contents once and never see them change again. Import from a card, or pick a folder that has photos in it.",
  emptyRootIsManagedLibrary:
    "One of the library folders is a photo-manager library itself (a Photos, iPhoto or Aperture one). pictkura deliberately does not read inside one, so nothing will ever come from it. Pick a folder that has photos in it, or import from a card.",
  emptyAllExcluded: (names: string) =>
    `Everything found is skipped by the exclude patterns (for example ${names}). You can change them in pictkura.toml in the settings folder.`,
  emptyNothingHere:
    "No photos that pictkura can read have turned up yet. Import from a card, or pick a folder that has photos in it.",
  calendarChecking: "Checking…",
  emptyTitleStalled: "Some places are not answering",
  emptyStalled: (names: string) =>
    `These places are not answering: ${names}. If one of them is on a network, check that it is still connected and press Rescan. If it is gone for good, remove it from the library folders and the rest will report.`,
  emptyChecking:
    "Still looking through the folders. If one of them is on a network, make sure it is connected and press Rescan.",
  emptyLoadFailed:
    "The list could not be loaded. Press Rescan, or reopen the app.",
  wizardPickFolderHint: "Pick a folder on the left to see the photos in it",
  wizardNoImages: "No photos in this folder",
  wizardUnreadable: "Could not read this folder (it may have been removed)",
  wizardCounting: "Loading…",
  wizardSelectAll: "Select all",
  wizardSelectNew: "Select new only",
  wizardClearSelection: "Clear selection",
  wizardSelected: (n: number) => `${num(n)} selected`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "Already imported (the same file exists in the destination)",
  wizardUnsureBadge: "?",
  wizardUnsureTitle:
    "This name is on the card more than once. Whether the file in the destination is this photo is not known until the contents are read — importing reads them, and does not make a second copy of a photo that is already there",
  wizardDestination: "Destination",
  wizardChangeDestination: "Change",
  wizardStructure: "Filing",
  wizardImportButton: (n: number) => `Import ${num(n)}`,
  wizardImportAll: "Import this whole folder (including subfolders)",
  wizardImportAllShort: "Whole folder",
  wizardDeep: "Include subfolders",
  wizardDeepHint: "Sweeps the whole media so you do not have to know where the photos are",
  wizardScanning: "Looking through the media…",
  wizardTruncated: (n: number) =>
    `Showing the first ${num(n)} only. Use "Whole folder" to import everything`,
  wizardScanIncomplete: "⚠ Some folders could not be read (photos may be missing)",
  decoderHeifNotice: (n: number) =>
    `⚠ ${num(n)} HEIC/HEIF ${one(n, "photo has", "photos have")} no thumbnail here, and will not open either. ${one(n, "It needs", "They need")} the free HEIF Image Extensions plus the paid HEVC Video Extensions (a few dollars) that decode the pixels`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ ${num(n)} HEIC/HEIF ${one(n, "photo has", "photos have")} no thumbnail here, and will not open either`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ ${num(n)} HEIC/HEIF ${one(n, "photo has", "photos have")} no thumbnail here, and will not open either. Your system may not have a decoder for HEIC/HEVC`,
  decoderHeifHow: "HEIF Image Extensions (free)",
  decoderHevcHow: "HEVC Video Extensions (paid)",
  decoderNoticeDismiss: "Don't show again",
  wizardOfflineTitle:
    "This file lives in the cloud (no preview here; importing will download it)",
  wizardHideImported: "Hide already imported",
  wizardAllImported: "Nothing new here (everything in this folder is already imported)",
  wizardHiddenCount: (n: number) => `${num(n)} already-imported hidden`,
  wizardEtaSeconds: (n: number) => `about ${num(n)}s left`,
  wizardEtaMinutes: (n: number) => `about ${num(n)} min left`,
  wizardEtaCalculating: "estimating time left…",
  wizardCapped: (n: number) => `${num(n)}+`,
  wizardMoreFiles: (n: number) => `${num(n)} more (scroll to load)`,
  menuOpen: "Open",
  menuOpenWith: (name: string) => `Open with ${name}`,
  menuOpenWithOther: "Open with another app…",
  menuReveal: "Show in folder",
  menuDelete: "Delete (move to trash)",
  menuFavoriteOn: "Add to favorites",
  menuFavoriteOff: "Remove from favorites",
  pickEditor: "Choose an app to edit with",
  deleteConfirmFiles: (photos: number, files: number) =>
    photos === 1
      ? `Move this photo (${num(files)} files) to the trash?`
      : `Move ${num(photos)} photos (${num(files)} files) to the trash?`,
  deleteConfirm: (n: number) =>
    n === 1
      ? "Move this photo to the trash?"
      : `Move ${num(n)} photos to the trash?`,
  deleted: (n: number) => `Moved ${num(n)} to the trash`,
  deletedSomeLeft: (n: number, left: number) =>
    `Moved ${num(n)} to the trash (${num(left)} ${one(left, "was", "were")} not found and ${one(left, "was", "were")} left alone)`,
  // 複数選択と一括操作
  selectItem: "Select",
  selectedCount: (n: number) => (n === 1 ? "1 selected" : `${num(n)} selected`),
  selectAll: "Select all",
  clearSelection: "Clear selection (Esc)",
  selectDay: "Select this whole day",
  bulkFavoriteOn: "Add to favorites",
  bulkFavoriteOff: "Remove from favorites",
  bulkDelete: "Move to trash",
  bulkCopy: "Copy to folder",
  bulkMove: "Move to folder",
  bulkViewer: "View the selection",
  pickExportFolder: "Choose a folder to export to",
  pickMoveFolder: "Choose a folder to move to",
  moveConfirm: (n: number) =>
    n === 1
      ? "Move this photo to a folder you pick next?\nIt leaves its current place and leaves the library.\n★ and ⚑ marks are not carried over."
      : `Move ${num(n)} photos to a folder you pick next?\nThey leave their current place and leave the library.\n★ and ⚑ marks are not carried over.`,
  confirmCancel: "Cancel",
  deleteConfirmOk: "Move to trash",
  moveConfirmOk: "Choose where to move",
  rootRemoveConfirmOk: "Remove",
  exporting: (done: number, total: number, name: string) =>
    `Exporting… ${num(done)}/${num(total)} ${name}`,
  exportDone: (done: number, skipped: number, failed: number) => {
    const parts = [done === 1 ? "Exported 1 photo" : `Exported ${num(done)} photos`];
    if (skipped > 0) parts.push(`${num(skipped)} already there`);
    if (failed > 0) parts.push(`${num(failed)} failed`);
    return parts.join(". ") + ".";
  },
  moving: (done: number, total: number, name: string) =>
    `Moving… ${num(done)}/${num(total)} ${name}`,
  moveDone: (moved: number, skipped: number, failed: number, leftBehind: number) => {
    const parts: string[] = [];
    if (moved > 0 || skipped + failed + leftBehind === 0) parts.push(moved === 1 ? "Moved 1 photo" : `Moved ${num(moved)} photos`);
    if (skipped > 0) parts.push(`${num(skipped)} already there, left in place`);
    if (failed > 0) parts.push(`${num(failed)} could not be moved and stayed in place`);
    if (leftBehind > 0) parts.push(`${num(leftBehind)} copied, but could not be removed from the original place`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "Pick",
  bulkPickOff: "Unpick",
  bulkPickDone: (n: number) =>
    n === 1 ? "1 photo picked" : `${num(n)} photos picked`,
  bulkUnpickDone: (n: number) =>
    n === 1 ? "1 photo unpicked" : `${num(n)} photos unpicked`,
  bulkFavoriteDone: (n: number) =>
    n === 1 ? "1 photo added to favorites" : `${num(n)} photos added to favorites`,
  bulkUnfavoriteDone: (n: number) =>
    n === 1
      ? "1 photo removed from favorites"
      : `${num(n)} photos removed from favorites`,
  settings: "Settings",
  close: "Close",
  settingsTitle: "Settings",
  settingsImportStructure: "Import folder structure",
  settingsImportStructureNote:
    "How imported photos are filed by capture date. This affects thousands of files, and changing it later takes considerable work. Where a folder name carries a date, it is written year-first in every language, so that sorting by name sorts by time.",
  settingsDestination: "Destination",
  settingsDestinationUnset: "(not set — you'll choose it on the first import)",
  settingsFlatExample: "IMG_0001.JPG (no subfolders)",
  settingsCustomPattern: "Custom",
  settingsCustomPatternNote:
    "{year} {month} {day} are replaced with the date. Use / for nesting. Unusable characters and moves to a parent folder (..) are dropped automatically.",
  settingsCustomPatternResult: "Resulting folder",
  settingsGrid: "Photo grid",
  settingsStackRawJpegToggle: "Stack RAW+JPEG pairs into one tile",
  settingsStackRawJpegNote:
    "A RAW and a JPEG with the same name in the same folder show as one tile, the JPEG. In the grid, ★, ⚑, delete and selection apply to both. Full screen steps through the files one by one, and ★, ⚑ and delete there apply only to the file you are looking at.",
  stackRawChipTitle: (files: number) =>
    `RAW and JPEG stacked into one tile (${num(files)} files)`,
  settingsStackBurstsToggle: "Stack bursts into one tile",
  settingsStackBurstsNote:
    "Photos taken in quick succession with the same camera show as one tile. Only photos whose capture time is recorded to a fraction of a second are stacked (some cameras do not record it). In the grid, ★ and ⚑ from the right-click menu apply to the cover photo only; delete and selection apply to the whole burst. Full screen steps through the files one by one.",
  settingsBurstGap: "Longest gap within a burst",
  burstGapOption: (seconds: number) => `${num(seconds)} s`,
  burstChip: (frames: number) => `▤ Burst ${num(frames)}`,
  burstChipShort: (frames: number) => `▤ ${num(frames)}`,
  burstTitle: (frames: number, spanMs: number, files: number) =>
    `Burst of ${num(frames)} shots over ${num(Math.round(spanMs / 100) / 10)} s (${num(files)} files)`,
  burstTitleRawJpeg: (frames: number, spanMs: number, files: number) =>
    `Burst of ${num(frames)} shots over ${num(Math.round(spanMs / 100) / 10)} s, RAW+JPEG (${num(files)} files)`,
  settingsViewer: "When you view a photo full screen",
  settingsAutoAdvanceToggle: "Move to the next photo after P / U",
  settingsAutoAdvanceNote:
    "In full screen, P flags the photo with ⚑ (a separate shelf from ★ favorites) and U clears it. With this on, the next photo follows right away, so picking takes one key per photo. With it off, you stay on the same photo.",
  settingsAutoplay: "When you insert a USB drive or SD card",
  settingsAutoplayToggle: "Offer pictkura in the AutoPlay choices",
  settingsAutoplayNote:
    "Adds pictkura to the Windows AutoPlay choices. It never starts on its own. Note that the entry itself is worded in Japanese. Uninstalling with the installer removes it, but the portable build — and other users’ copies on a shared PC — are not covered; turn this off before removing pictkura in those cases.",
  settingsAbout: "About",
  settingsAboutLicense: "Distributed under the MIT license.",
  settingsManual: "Manual",
  settingsOssLicenses: "Open source we use",
  settingsDocNotBundled: "(not bundled in a development build)",
  settingsLog: "Open the log",
  settingsLogNone: "(nothing recorded yet)",
  settingsOpenFailed: "It could not be opened.",
  settingsLogNote:
    "A line is written only when something fails, and it stays on this machine. Nothing is sent.",
  settingsLanguage: "Language",
  settingsLanguageSystem: "Match system",
  settingsLanguageNote:
    "Switching reloads the window. Copying itself keeps running in the background, but the import wizard closes and you lose sight of its progress — so wait until an import finishes before switching.",
  settingsTheme: "Theme",
  themeSystem: "Match system",
  themeLight: "Light",
  themeDark: "Dark",
  settingsEditors: "Editing apps",
  settingsEditorsNote: "Apps you picked in “Open with another app…”.",
  settingsForgetEditor: "Remove from the list",
  calendarEmpty: "No photos",
  speedPrefix: (sec: string) => `⚡ Startup check in ${sec}s — `,
  speedUsn: "USN journal delta: ",
  speedUsnNoChange: "no changes, no folders walked",
  speedUsnDirty: (records: number, dirs: number) =>
    `${num(records)} journal ${one(records, "record", "records")} → rescanned only ${num(dirs)} ${one(dirs, "folder", "folders")}`,
  speedPruned: (skipped: number) =>
    `pruned scan: skipped ${num(skipped)} ${one(skipped, "folder", "folders")}`,
  speedFull: (total: number) =>
    `full scan (${num(total)} ${one(total, "file", "files")})`,
  speedNoDiff: " — no changes",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` — ${num(added)} added, ${num(changed)} changed, ${num(removed)} removed`,
  errNotFound: "That photo is no longer in the index.",
  errDb: "The index could not be read or written.",
  errConfigIo: "The settings file could not be read or written.",
  errConfigFormat: "The settings file could not be understood.",
  errNoDestination: "No destination folder has been chosen yet.",
  errRootManaged:
    "A managed library cannot be added as a folder — what is inside belongs to that app.",
  errDestManaged:
    "A managed library cannot be the destination — what is inside belongs to that app.",
  errNoImageToExtract: "There is no picture in this file to take out.",
  errSourceUnreadable: "That folder could not be read.",
  errSourceManaged:
    "The inside of a managed library cannot be imported. Drop the extension (.photoslibrary and the like) from the folder name and choose it again.",
  errExportDest: "That destination folder could not be created.",
  errExportManaged: "Nothing can be written there — another app manages that package.",
  errFolderMissing: "That folder is not there.",
  errTrashFailed: "It could not be moved to the trash.",
  errTrashPartly: (n: number) =>
    `${num(n)} moved to the trash; the rest could not be.`,
  errAutoplayRollback:
    "The setting could not be saved. AutoPlay is left as it was just changed, and the next launch puts it back in line with the setting.",
  errNoLogYet: "Nothing has been recorded yet.",
  errNotBundled: "It is not bundled in this build.",
  errNoStoreLink: "This OS has no page to point you to.",
  errBadKind: "There was nothing to open.",
  errTooManyIds: (n: number) => `No more than ${num(n)} at a time.`,
};
