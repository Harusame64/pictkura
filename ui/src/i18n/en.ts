/** 英語辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。 */
import { folderExample } from "./folderExample";
import type { Dict } from "./ja";

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
  itemsSuffix: "items",
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
        ["1 / 0", "Actual size 100% / fit to screen"],
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
        ["Right-click", "Open / open with / show in folder / move to trash"],
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
  showMore: (n: number) => `${n} more`,
  collapse: "Show less",
  photosCount: (n: number) => `${n}`,
  memoriesTitle: (years: number) =>
    years === 1 ? "1 year ago today" : `${years} years ago today`,
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
  rejectChip: (n: number) => `✕ ${n}`,
  rejectChipTitle: "Review the rejected photos",
  rejectGateTitle: (n: number) =>
    n === 1 ? "Move 1 photo to the trash" : `Move ${n} photos to the trash`,
  rejectGateNote: "You can restore them from the trash (they come back to the library, too).",
  rejectGateRestore: "Keep",
  rejectGateBack: "Back",
  rejectGateDiscard: "Close without deleting",
  rejectGateConfirm: (n: number) => (n === 1 ? "Move 1 to trash" : `Move ${n} to trash`),
  rejectGateTrashing: (done: number, total: number) =>
    `Moving… (${done} / ${total})`,
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
  videoMissing: "This file is missing (it looks moved or deleted)",
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
  importFrom: (path: string) => `Import from ${path}`,
  filterByCamera: (name: string) => `Show only photos taken with ${name}`,
  jumpToYear: (year: number) => `Jump to ${year}`,
  importing: (done: number, total: number) => `Importing… ${done}/${total}`,
  importDone: (copied: number, skipped: number) =>
    `Import finished: ${copied} copied, ${skipped} skipped`,
  importFailed: (n: number) => `, ${n} failed`,
  importIncomplete: " ⚠ Some folders could not be read — do not erase the card yet",
  syncDone: (added: number, changed: number, removed: number) =>
    `${added} added, ${changed} changed, ${removed} removed`,
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
    `These places are not there: ${names}. If that is an external drive, connect it and press Rescan.`,
  emptyUnreadableMac: (names: string) =>
    `These places could not be opened: ${names}. Grant pictkura access to that folder (Desktop, Documents, an external drive) in System Settings → Privacy & Security. If it is on a network, make sure it is connected and press Rescan.`,
  emptyUnreadableWin: (names: string) =>
    `These places could not be opened: ${names}. Check the folder's permissions. If it is a network drive, make sure it is connected and press Rescan.`,
  emptyUnreadableOther: (names: string) =>
    `These places could not be opened: ${names}. Check that you have permission to read them, then press Rescan.`,
  listSeparator: ", ",
  andMore: (n: number) => `and ${n} more`,
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
    "The list could not be loaded. The reason is in the bar above. Press Rescan, or reopen the app.",
  wizardPickFolderHint: "Pick a folder on the left to see the photos in it",
  wizardNoImages: "No photos in this folder",
  wizardUnreadable: "Could not read this folder (it may have been removed)",
  wizardCounting: "Loading…",
  wizardSelectAll: "Select all",
  wizardSelectNew: "Select new only",
  wizardClearSelection: "Clear selection",
  wizardSelected: (n: number) => `${n} selected`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "Already imported (the same file exists in the destination)",
  wizardDestination: "Destination",
  wizardChangeDestination: "Change",
  wizardStructure: "Filing",
  wizardImportButton: (n: number) => `Import ${n}`,
  wizardImportAll: "Import this whole folder (including subfolders)",
  wizardImportAllShort: "Whole folder",
  wizardDeep: "Include subfolders",
  wizardDeepHint: "Sweeps the whole media so you do not have to know where the photos are",
  wizardScanning: "Looking through the media…",
  wizardTruncated: (n: number) =>
    `Showing the first ${n} only. Use "Whole folder" to import everything`,
  wizardScanIncomplete: "⚠ Some folders could not be read (photos may be missing)",
  decoderHeifNotice: (n: string) =>
    `⚠ ${n} HEIC/HEIF photos have no thumbnail here, and will not open either. They need the free HEIF Image Extensions plus the paid HEVC Video Extensions (a few dollars) that decode the pixels`,
  decoderHeifNoticeMac: (n: string) =>
    `⚠ ${n} HEIC/HEIF photos have no thumbnail here, and will not open either`,
  decoderHeifNoticeOther: (n: string) =>
    `⚠ ${n} HEIC/HEIF photos have no thumbnail here, and will not open either. Your system may not have a decoder for HEIC/HEVC`,
  decoderHeifHow: "HEIF Image Extensions (free)",
  decoderHevcHow: "HEVC Video Extensions (paid)",
  decoderNoticeDismiss: "Don't show again",
  wizardOfflineTitle:
    "This file lives in the cloud (no preview here; importing will download it)",
  wizardHideImported: "Hide already imported",
  wizardAllImported: "Nothing new here (everything in this folder is already imported)",
  wizardHiddenCount: (n: number) => `${n} already-imported hidden`,
  wizardCopying: "Importing",
  wizardEtaSeconds: (n: number) => `about ${n}s left`,
  wizardEtaMinutes: (n: number) => `about ${n} min left`,
  wizardEtaCalculating: "estimating time left…",
  wizardCapped: (n: number) => `${n}+`,
  wizardMoreFiles: (n: number) => `${n} more (scroll to load)`,
  menuOpen: "Open",
  menuOpenWith: (name: string) => `Open with ${name}`,
  menuOpenWithOther: "Open with another app…",
  menuReveal: "Show in folder",
  menuDelete: "Delete (move to trash)",
  menuFavoriteOn: "Add to favorites",
  menuFavoriteOff: "Remove from favorites",
  pickEditor: "Choose an app to edit with",
  deleteConfirm: (n: number) =>
    n === 1
      ? "Move this photo to the trash?"
      : `Move ${n} photos to the trash?`,
  deleted: (n: number) => `Moved ${n} to the trash`,
  deletedSomeLeft: (n: number, left: number) =>
    `Moved ${n} to the trash (${left} could not be found and were left alone)`,
  // 複数選択と一括操作
  selectItem: "Select",
  selectedCount: (n: number) => (n === 1 ? "1 selected" : `${n} selected`),
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
  moveConfirm: (n: number) =>
    n === 1
      ? "Move this photo to a folder you pick next? It leaves its current place and leaves the library (★ and ⚑ marks are not carried over)."
      : `Move ${n} photos to a folder you pick next? They leave their current place and leave the library (★ and ⚑ marks are not carried over).`,
  exporting: (done: number, total: number, name: string) =>
    `Exporting… ${done}/${total} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, leftBehind: number) => {
    const parts = [done === 1 ? "Exported 1 photo" : `Exported ${done} photos`];
    if (skipped > 0) parts.push(`${skipped} already there`);
    if (failed > 0) parts.push(`${failed} failed`);
    if (leftBehind > 0) parts.push(`${leftBehind} could not be removed from the original place`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "Pick",
  bulkPickOff: "Unpick",
  bulkPickDone: (n: number) =>
    n === 1 ? "1 photo picked" : `${n} photos picked`,
  bulkUnpickDone: (n: number) =>
    n === 1 ? "1 photo unpicked" : `${n} photos unpicked`,
  bulkFavoriteDone: (n: number) =>
    n === 1 ? "1 photo added to favorites" : `${n} photos added to favorites`,
  bulkUnfavoriteDone: (n: number) =>
    n === 1
      ? "1 photo removed from favorites"
      : `${n} photos removed from favorites`,
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
    `${records} journal records → rescanned only ${dirs} folders`,
  speedPruned: (skipped: number) => `pruned scan: skipped ${skipped} folders`,
  speedFull: (total: number) => `full scan (${total} files)`,
  speedNoDiff: " — no changes",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` — ${added} added, ${changed} changed, ${removed} removed`,
};
