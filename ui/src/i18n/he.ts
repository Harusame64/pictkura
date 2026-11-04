/**
 * ヘブライ語の辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **Apple・Microsoft のヘブライ語版と同じ書き方にする。** 語は発明せず、既にヘブライ語圏で使われて
 * いるものへ合わせる（ほかの言語と同じ方針）:
 *
 * - **右から左へ書く言語。** 画面の鏡は `<html dir>` と CSS が受け持つ（`direction.ts`）。辞書が
 *   受け持つのは矢印だけで、アラビア語と同じ——`viewerPrev` は「→」、`viewerNext` は「←」。
 *   ショートカット一覧の「← / →」は文字列のまま置き、説明を「次 / 前」の順にする
 * - **呼びかけで性を割らない。** ヘブライ語の命令形と二人称は男女で形が違う。
 *   - 利用者へ頼む文は**複数の命令形**（`לחצו`・`בחרו`・`חברו`）。性を問わない書き方として広く使われる
 *   - 確かめる問いは**不定詞**（`להעביר … לפח האשפה?`）。Apple の確認画面と同じ
 *   - ボタンとメニューは**動作名詞**（`מחיקה`・`העתקה`・`ביטול`）。Apple のヘブライ語と同じ
 *   - 進み具合の現在分詞（`טוען…`・`מעביר…`）は男性形。主語はアプリで、Apple の `טוען…` と同じ
 * - **⚑ / ✕ は `סימון`（印を付ける）/ `דחייה`（退ける）**。⚑ の印の付いた写真は `מסומנת`、✕ は `נדחתה`。
 *   **⚑ に `בחירה` を使わない**——複数選択が `בחירה`（Apple の Select All は `בחירת הכול`）なので、
 *   アラビア語の `انتقاء` と同じ理由で割る。⚑ と ✕ の両方を覆う文字列は中立の `הערכה`（日本語の「判定」）
 * - **アプリの蔵書は `הספרייה`、写真アプリの蔵書は `הספרייה של אפליקציית התמונות`**。macOS の写真アプリの
 *   名前は `תמונות`（写真）で、そのまま書くと「写真」と見分けが付かないので、アプリと言い添える
 * - **送り出しは `תיקיית השליחה`、書き出しは `ייצוא`**——ja が「送り出し」と「書き出し」を分けるのと同じく割る
 * - **ゴミ箱は Apple の `פח האשפה`**。Windows の he は `סל המיחזור` だが、どちらの OS の人にも通じる
 *   一般の語の方を1つに決める（英語の `trash`・アラビア語と同じ）
 * - **OSの用語は引いてくる**。Apple の he（loctable で確認）: `ביטול`・`מחיקה`・`העתקה`・`שמירה`・
 *   `בחירת הכול`・`הדפסה…`・`הגדרות…`・`פתיחה באמצעות`・`פח אשפה`、macOS の
 *   `הגדרות המערכת ← פרטיות ואבטחה`。Windows の he: `הפעלה אוטומטית`（自動再生）・
 *   `הצג אפשרויות נוספות`（Windows 11 のエクスプローラー）・`סייר הקבצים`・`קישורים קשיחים`（ハードリンク）
 * - **金額はシェケルで言う**（`בתשלום של כמה שקלים`）。ヘブライ語の利用者はほぼイスラエルに居る
 * - **確認画面の取消は `ביטול`**（AppKit の Cancel。違うと macOS で Esc が効かない）
 * - **自動再生の項目名は日本語のまま引く**（Windows に登録してあるのは日本語だけ。ほかの言語と同じ）
 * - キーの文字（P・X・U・F・I…）と製品名はラテン文字のまま。ラテン文字の前の接頭辞は `-` でつなぐ
 *   （`ב-Windows`・`מ-USB`）。文に差し込むパス・名前・検索語・理由は `isolate()` で向きを閉じる
 * - **引用符は Apple と同じ `״`（ゲルシャイム）**。メニューの `מידע על ״%@״` と揃う。字種は右から左なので、
 *   曲がった “ ” のように右から左の行で向きが逆に見えることも、差し込んだ名前の側へ寄ることもない
 *
 * **数はほぼ1か、それ以外か**（`one()`）。CLDR の he には `two` もあるが、名詞の双数が残るのは時の語
 * （`שנתיים`＝2年）くらいで、写真やファイルは `2 תמונות` と数字で言う。2 の形は `memoriesTitle` だけ:
 * - **1**: 数字を付けずに `תמונה אחת`（名詞＋「1つの」。数詞は名詞の後ろで、名詞の性に合わせる）
 * - **それ以外**: `3 תמונות`（数字＋複数）
 *
 * **動詞の一致を避ける書き方にする。** 過去と受け身の動詞は単数で性が割れる（`הועברה`＝女性、
 * `הועבר`＝男性）。写真（`תמונה`）は女性、ファイル（`קובץ`）・動画（`סרטון`）・項目（`פריט`）は男性。
 * 件数の報告は「ラベル: N」（`נוספו: 3`）か動作名詞（`העברת 3 תמונות`）で書き、形の組み合わせを減らす。
 * 写真と動画・ファイルが混ざる物を指すときは、ヘブライ語の既定どおり男性の複数（`במקומם`・`אותם`）
 */
import { isolate } from "../direction.ts";
import { folderExample } from "./folderExample.ts";
import { num, one, year } from "./plural.ts";
import type { Dict } from "./ja.ts";

/**
 * パス・名前・検索語・失敗の理由は、向きを閉じて差し込む（`direction.ts` の `isolate`）。
 * **名前の一覧（`names`・`list`）は閉じない**——呼ぶ側が名前ごとに閉じて渡す（`isolateEach`）。
 * 丸ごと閉じると、一覧の並びが先頭の名前の字の向きで決まってしまう（#284 の win の実機）
 */
const iso = isolate;

/** 写真（女性） */
const photos = (n: number) => one(n, "תמונה אחת", `${num(n)} תמונות`);
/** ファイル（男性） */
const files = (n: number) => one(n, "קובץ אחד", `${num(n)} קבצים`);
/** 動画（男性） */
const videos = (n: number) => one(n, "סרטון אחד", `${num(n)} סרטונים`);
/** 項目（男性。写真と動画が混ざる物） */
const items = (n: number) => one(n, "פריט אחד", `${num(n)} פריטים`);
/** 連写の1枚（男性。`צילום`＝撮影したもの） */
const shots = (n: number) => one(n, "צילום אחד", `${num(n)} צילומים`);

export const he: Dict = {
  appName: "pictkura",
  viewThumbnails: "תמונות",
  viewCalendar: "לוח שנה",
  searchPlaceholder: "חיפוש קובץ, תיקייה, מצלמה, 2019-08 או year:2019",
  searchClear: "ניקוי החיפוש (Esc)",
  commandPalette: "לוח הפקודות",
  // 「מ-USB」は折らない（‑ は改行しないハイフン）——アラビア語と同じく狭い窓で上の帯からはみ出さないように
  importFromUsb: "ייבוא מ‑USB",
  rescan: "סריקה מחדש",
  size: "גודל",
  itemsCount: (n: number) => items(n),
  navPlaces: "מקומות",
  navAllPhotos: "כל התמונות",
  navFavorites: "★ מועדפים",
  navPicked: "⚑ מסומנות",
  navOutgoing: "תיקיית השליחה",
  outgoingGuide:
    "בחרו את תיקיית השליחה הזו כתיקייה לגיבוי (סנכרון) באפליקציה של שירות התמונות שלכם. מעכשיו, כל ייבוא יסדר כאן את התמונות, והשירות יאסוף אותן. אם אתם משתמשים ב-Google Photos: הוסיפו את התיקייה הזו לגיבוי התיקיות ב-Google Photos.",
  navKinds: "סוג",
  kindPhoto: "תמונות",
  kindRaw: "RAW",
  kindVideo: "סרטונים",
  shortcutsTitle: "קיצורי מקלדת (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "הצגת קיצורי המקלדת",
  shortcutGroups: [
    {
      title: "רשת",
      keys: [
        ["Ctrl+K / ⌘K", "לוח הפקודות (מעבר לתאריך או למצלמה, חיפוש, ייבוא)"],
        ["Ctrl+, / ⌘,", "פתיחת ההגדרות (אותו חלון שנפתח מגלגל השיניים שבסרגל)"],
        ["Ctrl+A / ⌘A", "בחירת כל מה שתואם לחיפוש ולסינון הנוכחיים"],
        ["Shift + לחיצה", "בחירת כל מה שבין המשבצת האחרונה שלחצתם עליה לבין זו"],
        ["Ctrl + לחיצה", "הוספה או הסרה של תמונה אחת (⌘ + לחיצה ב-macOS)"],
        ["לחיצה על כותרת תאריך", "בחירת היום כולו (לחיצה נוספת מנקה)"],
        ["Esc", "סיום הבחירה"],
      ],
    },
    {
      title: "מציג",
      keys: [
        // 右→左の行では「← / →」が右から ←・→ の順に描かれる。先に読む ← が「次」
        ["← / →", "התמונה הבאה / הקודמת"],
        ["P", "סימון התמונה (⚑). כברירת מחדל עוברים לתמונה הבאה"],
        ["X", "דחיית התמונה (✕). תמונות שנדחו עוברות לפח האשפה כשסוגרים את המציג"],
        ["U", "ביטול ההערכה של התמונה הזו (הסרת ⚑ ו-✕)"],
        ["Ctrl+C / ⌘C", "העתקת התמונה שעל המסך ללוח"],
        ["Ctrl+S / ⌘S", "שמירת התמונה שעל המסך לקובץ"],
        ["Ctrl+P / ⌘P", "הדפסת התמונה שעל המסך"],
        ["F", "הוספה למועדפים או הסרה מהם (★)"],
        ["I", "פרטי הקובץ והצילום (מצלמה, עדשה, צמצם, תריס, ISO, אורך מוקד, GPS)"],
        ["רווח", "מצגת. בסרטון: הפעלה / השהיה"],
        ["1 / 0", "1 עובר בין גודל אמיתי 100% להתאמה למסך; 0 תמיד מתאים למסך"],
        ["F11", "מסך מלא"],
        ["Esc", "סגירה"],
      ],
    },
    {
      title: "עכבר (במציג)",
      keys: [
        ["לחיצה כפולה", "גודל אמיתי 100% ⇔ התאמה למסך"],
        ["גלגלת", "הגדלה / הקטנה"],
        ["גרירה", "הזזה בזמן הגדלה"],
        ["לחיצה ימנית", "תפריט (פתיחה, העתקה, סיבוב, הדפסה, מידע, מחיקה ועוד)"],
        ["לחיצה על הרצועה", "מעבר לאותה תמונה"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "מצלמות ומדיה",
  navLibraryFolders: "תיקיות הספרייה",
  navDrives: "כוננים",
  navAddFolder: "הוספת תיקייה",
  add: "הוספה",
  browse: "עיון…",
  addFolderPlaceholder: folderExample("לדוגמה: ", "user"),
  pickLibraryFolder: "בחרו תיקייה להוספה לספרייה",
  showMore: (n: number) => `עוד ${num(n)}`,
  collapse: "הצגת פחות",
  photosCount: (n: number) => `${num(n)}`,
  // 年だけは双数が残る（`שנתיים`＝2年）
  memoriesTitle: (years: number) =>
    years === 1
      ? "היום לפני שנה"
      : years === 2
        ? "היום לפני שנתיים"
        : `היום לפני ${num(years)} שנים`,
  viewerFavorite: "מועדף (F)",
  viewerPick: "סימון (P)",
  viewerUnpick: "ביטול סימון (U)",
  viewerPicked: "תמונה מסומנת",
  judgeFav: "מועדפת",
  judgeUnfav: "הוסרה מהמועדפים",
  judgePick: "מסומנת",
  judgeUnflag: "ההערכה בוטלה",
  viewerReject: "דחייה (X)",
  viewerRejected: "נדחתה",
  favoriteMark: "מועדפת",
  viewerMutedHint: "מושתק — לחצו כדי להפעיל את הצליל",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "סקירת התמונות שנדחו",
  rejectGateTitle: (n: number) => `העברת ${photos(n)} לפח האשפה`,
  rejectGateNote: "אפשר לשחזר אותן מפח האשפה (והן יחזרו גם לספרייה).",
  rejectGateRestore: "השארה",
  rejectGateBack: "חזרה",
  rejectGateDiscard: "סגירה בלי למחוק",
  rejectGateConfirm: (n: number) => `העברת ${photos(n)} לפח האשפה`,
  rejectGateTrashing: (done: number, total: number) =>
    `מעביר… (${num(done)} / ${num(total)})`,
  updateFound: (v: string) => `גרסה ${iso(v)} זמינה`,
  updateOpenPage: "פתיחת דף ההורדה",
  updateLater: "מאוחר יותר",
  updateCheckNow: "בדיקת עדכונים",
  updateChecking: "בודק…",
  updateUpToDate: "הגרסה שלכם עדכנית",
  updateFailed: "לא ניתן היה לבדוק",
  updateOnStart: "בדיקת עדכונים בהפעלה",
  updateOnStartNote:
    `שואל את GitHub מהו שם הגרסה האחרונה (פעם ביום). לא נשלחות תמונות ולא שמות קבצים. אם תכבו את האפשרות, שום דבר לא ייצא מהמחשב הזה, מלבד כשלוחצים על ״בדיקת עדכונים״.`,
  viewerSlideshow: "מצגת (רווח)",
  extractSave: (key: string) => `שמירת התמונה הזו לקובץ (${key})`,
  extractCopy: (key: string) => `העתקת התמונה הזו ללוח (${key})`,
  extractSaveTitle: "שמירת התמונה בשם",
  extractFilter: "תמונה",
  extractSaved: "נשמרה",
  extractCopied: "הועתקה",
  extractCopying: "מעתיק…",
  extractFailed: "לא ניתן היה לחלץ את התמונה הזו",
  extractSameFile: "לא ניתן לדרוס את הקובץ המקורי",
  viewerExif: "מידע על התמונה (I)",
  viewerFullscreen: "מסך מלא (F11)",
  viewerClose: "סגירה (Esc)",
  // 右→左の画面では → が「前」、← が「次」（direction.ts の stepOfArrow）
  viewerPrev: "הקודמת (→)",
  viewerNext: "הבאה (←)",
  viewerFitToScreen: "התאמה למסך (0)",
  viewerActualSize: "גודל אמיתי, 100% (1) — או לחיצה כפולה",
  actualSizeBadge: "1:1",
  videoUnsupported: "לא ניתן להפעיל את הפורמט הזה באפליקציה",
  fileMissing:
    "הקובץ הזה לא נמצא (ייתכן שזיכרון USB או כרטיס SD נותקו, או שהקובץ הועבר או נמחק)",
  fileUnreachable: "לא ניתן לפתוח את הקובץ הזה (הכונן אינו מוכן, או שהגישה אינה מותרת)",
  fileNotShown:
    "לא ניתן להציג את הקובץ הזה (הקובץ פגום, או ש-pictkura אינו יכול לקרוא את הפורמט הזה)",
  fileNotDownloaded: "הקובץ הזה עדיין לא הורד (פתחו אותו שוב כשתהיו מחוברים לאינטרנט)",
  videoCloudOnly: "הסרטון הזה נמצא בענן",
  videoCloudOnlyNote:
    "הפעלה כאן מתחילה הורדה ולא מציגה דבר עד שהיא מסתיימת. פתיחה באפליקציית ברירת המחדל מאפשרת לעקוב אחר התקדמות ההורדה.",
  videoFailed: "לא ניתן היה להפעיל את הסרטון הזה",
  videoOpenExternal: "פתיחה באפליקציית ברירת המחדל",
  // 「HEVC (H.265)」の括弧は右から左の文の中で向きが崩れるので、括弧を使わずに言う
  videoCodecNote:
    "סרטונים מ-iPhone וממצלמות דומות מוקלטים בפורמט HEVC, הידוע גם כ-H.265. כדי להפעיל אותם נדרש מפענח במערכת; ב-Windows זו הרחבה בתשלום של כמה שקלים מ-Microsoft Store.",
  videoCodecNoteMac:
    "מערכת macOS מפענחת HEVC כבר מההתחלה, ולכן סביר שזהו פורמט הקלטה שאינה תומכת בו.",
  videoCodecNoteOther: "ייתכן שאין במערכת שלכם מפענח לפורמט ההקלטה הזה.",
  videoCodecHelp: "השגת הרחבות הווידאו של HEVC (בתשלום)",
  loading: "טוען…",
  exifTitle: "מידע על התמונה",
  exifCamera: "מצלמה",
  exifLens: "עדשה",
  exifAperture: "צמצם",
  exifShutter: "תריס",
  exifIso: "ISO",
  exifFocal: "אורך מוקד",
  exifLocation: "מיקום",
  exifNone: "אין נתוני EXIF",
  paletteInput: "תאריך, מצלמה, מילת מפתח או פקודה…",
  paletteNoResults: "אין תוצאות",
  paletteGroupJumpDate: "מעבר לתאריך",
  paletteGroupRecentDays: "ימים אחרונים",
  paletteGroupCameras: "סינון לפי מצלמה",
  paletteGroupSearch: "חיפוש",
  paletteGroupActions: "פעולות",
  paletteSearchFor: (q: string) => `חיפוש ״${iso(q)}״`,
  paletteSearchHint: "שם קובץ, תיקייה, מצלמה",
  paletteSelect: "בחירה",
  paletteRun: "הפעלה",
  paletteCloseHint: "סגירה",
  actionShowFavorites: "הצגת המועדפים בלבד",
  actionShowPicked: "הצגת המסומנות בלבד",
  actionShowAll: "הצגת כל התמונות",
  actionCalendar: "תצוגת לוח שנה",
  actionThumbnails: "רשת התמונות",
  indexBuilding: "🔍 בונה את אינדקס החיפוש… ",
  bgScanning: "מעדכן את הספרייה…",
  bgThumbs: (n: number) => `קורא תמונות (נותרו ${num(n)})`,
  bgScanningShort: "מעדכן",
  bgThumbsShort: (n: number) => `נותרו ${num(n)}`,
  cameraScanning: "📷 קורא את פרטי המצלמה… ",
  indexIncompleteWarning:
    "⚠ בניית אינדקס החיפוש נקטעה — ייתכן שהתוצאות חלקיות (היא תימשך בהפעלה הבאה)",
  indexProgressSuffix: "% — ייתכן שהתוצאות חלקיות עד שזה יסתיים",
  removeRoot: (path: string) => `הסרת ${iso(path)} מהספרייה`,
  rootMissingNotice: (name: string, count: number) =>
    `התיקייה ״${iso(name)}״ לא נמצאת${count > 0 ? ` (לא ניתן לפתוח ${photos(count)} שבה)` : ""}. אם היא בזיכרון USB או בכרטיס SD, חברו אותו ולחצו על ״סריקה מחדש״.`,
  rootsMissingNotice: (names: string, count: number) =>
    `חלק מהתיקיות לא נמצאות: ${names}${count > 0 ? ` (לא ניתן לפתוח ${photos(count)} שבהן)` : ""}. אם הן בזיכרון USB או בכרטיס SD, חברו אותו שוב ולחצו על ״סריקה מחדש״. כדי להפסיק להשתמש בתיקייה, הסירו אותה מהספרייה בלחצן ✕ שברשימה שבצד (קובצי התמונות לא נמחקים).`,
  rootRemoveFromLibrary: "הסרה מהספרייה",
  rootRemoveKeepsFiles: "מסיר את התיקייה מהספרייה בלבד. קובצי התמונות לא נמחקים",
  rootRemoveConfirm: (name: string) =>
    `להסיר את ״${iso(name)}״ מהספרייה?\nקובצי התמונות לא יימחקו, אבל סימוני ★ ו-⚑ שעל התמונות שבה יימחקו.`,
  rootTempConfirm: (path: string) =>
    `״${iso(path)}״ נמצאת בתוך תיקייה זמנית.\nהמערכת או אפליקציות אחרות עלולות למחוק בה קבצים בלי לשאול. להוסיף אותה לספרייה בכל זאת?`,
  rootTempConfirmOk: "הוספה בכל זאת",
  destTempConfirm: (path: string) =>
    `״${iso(path)}״ נמצאת בתוך תיקייה זמנית.\nהמערכת או אפליקציות אחרות עלולות למחוק תמונות שיובאו אליה בלי לשאול. אחרי מחיקת הכרטיס, הן לא יהיו בשום מקום אחר. להשתמש בה כיעד בכל זאת?`,
  destTempConfirmOk: "שימוש כיעד",
  destTempWarning: "⚠ היעד נמצא בתוך תיקייה זמנית. תמונות מיובאות עלולות להיעלם בלי אזהרה.",
  rootTempTip: (path: string) =>
    `${iso(path)} — בתוך תיקייה זמנית. המערכת או אפליקציות אחרות עלולות למחוק בה קבצים בלי לשאול`,
  rootMissingTempTip: (path: string) =>
    `${iso(path)} — לא נמצאת. היא הייתה בתוך תיקייה זמנית, ולכן ייתכן שהמערכת או אפליקציה אחרת מחקו אותה`,
  rootMissingTip: (path: string) =>
    `${iso(path)} — לא נמצאת. אם היא בזיכרון USB או בכרטיס SD, חברו אותו ולחצו על ״סריקה מחדש״`,
  importFrom: (path: string) => `ייבוא מ-${iso(path)}`,
  filterByCamera: (name: string) => `הצגת התמונות שצולמו ב-${iso(name)} בלבד`,
  jumpToYear: (y: number) => `מעבר ל-${year(y)}`,
  importing: (done: number, total: number) => `מייבא… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `הייבוא הסתיים: הועתקו ${num(copied)}, דולגו ${num(skipped)}`,
  importFailed: (n: number) => `, נכשלו ${num(n)}`,
  importIncomplete: " ⚠ לא ניתן היה לקרוא חלק מהתיקיות — אל תמחקו עדיין את הכרטיס",
  syncDone: (added: number, changed: number, removed: number) =>
    `נוספו: ${num(added)}, השתנו: ${num(changed)}, הוסרו: ${num(removed)}`,
  drivesReturnedScanned: (added: number, changed: number) =>
    `התיקיות שבכונן שחובר נקראו מחדש (נוספו: ${num(added)}, השתנו: ${num(changed)})`,
  pickSource: "בחרו את התיקייה לייבא ממנה (USB / DCIM)",
  pickDestination: "בחרו את תיקיית היעד",
  wizardTitle: "ייבוא",
  wizardSources: "מקור",
  wizardOtherFolder: "תיקייה אחרת…",
  wizardRefresh: "סריקת הכוננים מחדש",
  wizardRemovable: "נשלף",
  wizardNoDrives: "לא נמצאו כוננים",
  emptyTitle: "אין עדיין תמונות",
  emptyTitleFailed: "לא ניתן היה להציג את הרשימה",
  emptyTitleChecking: "בודק",
  emptyTitleStartupFailed: "הסנכרון בהפעלה לא הסתיים",
  emptyStartupFailed:
    `הסנכרון שרץ בהפעלה לא הסתיים. ייתכן שיש תמונות שעדיין לא נקלטו. לחצו על ״סריקה מחדש״; אם זה לא עוזר, פתחו את האפליקציה מחדש.`,
  emptyTitleMissing: "חלק מהמקומות לא נמצאים",
  emptyTitleUnreadable: "לא ניתן היה לפתוח חלק מהמקומות",
  emptyNoRoots: "עדיין לא הוגדרה תיקיית ספרייה. ייבאו מכרטיס, או בחרו תיקייה שיש בה תמונות.",
  emptyMissing: (names: string) =>
    `המקומות האלה לא נמצאים: ${names}. אם הם בזיכרון USB או בכרטיס SD, חברו אותו שוב ולחצו על ״סריקה מחדש״.`,
  emptyUnreadableMac: (names: string) =>
    `לא ניתן היה לפתוח את המקומות האלה: ${names}. תנו ל-pictkura גישה לתיקייה הזו (שולחן העבודה, מסמכים, כונן חיצוני) בהגדרות המערכת ← פרטיות ואבטחה. אם היא ברשת, ודאו שהיא מחוברת ולחצו על ״סריקה מחדש״.`,
  emptyUnreadableWin: (names: string) =>
    `לא ניתן היה לפתוח את המקומות האלה: ${names}. בדקו את ההרשאות של התיקייה. אם זה כונן רשת, ודאו שהוא מחובר ולחצו על ״סריקה מחדש״.`,
  emptyUnreadableOther: (names: string) =>
    `לא ניתן היה לפתוח את המקומות האלה: ${names}. ודאו שיש לכם הרשאה לקרוא אותם, ואז לחצו על ״סריקה מחדש״.`,
  listSeparator: ", ",
  listLastSeparator: " ",
  andMore: (n: number) => `ועוד ${num(n)}`,
  emptyRootIsPackage:
    "אחת מתיקיות הספרייה היא בעצמה הספרייה של אפליקציית התמונות. pictkura אינו קורא בתוכה בכוונה, ולכן שום דבר לא יגיע ממנה. בחרו תיקייה שיש בה תמונות, או ייבאו מכרטיס.",
  emptyPhotoLibrary:
    "לא נמצא דבר מלבד הספרייה של אפליקציית התמונות. כברירת מחדל pictkura אינו קורא בתוך אפליקציית התמונות — רוב הקבצים המקוריים נמצאים ב-iCloud, ולא במחשב הזה. ייבאו מכרטיס, או בחרו תיקייה שיש בה תמונות.",
  emptyManagedLibrary:
    "לא נמצא דבר מלבד ספריות מנוהלות של אפליקציות תמונות (תמונות, iPhoto או Aperture). pictkura אינו קורא בתוכן בכוונה — הוא היה מוסיף את התוכן לאינדקס פעם אחת, ולא היה רואה עוד את השינויים בו. ייבאו מכרטיס, או בחרו תיקייה שיש בה תמונות.",
  emptyRootIsManagedLibrary:
    "אחת מתיקיות הספרייה היא בעצמה ספרייה מנוהלת של אפליקציית תמונות (תמונות, iPhoto או Aperture). pictkura אינו קורא בתוכה בכוונה, ולכן שום דבר לא יגיע ממנה. בחרו תיקייה שיש בה תמונות, או ייבאו מכרטיס.",
  emptyAllExcluded: (names: string) =>
    `כל מה שנמצא מדולג לפי דפוסי ההחרגה (למשל ${names}). אפשר לשנות אותם בקובץ pictkura.toml שבתיקיית ההגדרות.`,
  emptyNothingHere:
    "עדיין לא נמצאו תמונות ש-pictkura יכול לקרוא. ייבאו מכרטיס, או בחרו תיקייה שיש בה תמונות.",
  calendarChecking: "בודק…",
  emptyTitleStalled: "חלק מהמקומות לא מגיבים",
  emptyStalled: (names: string) =>
    `המקומות האלה לא מגיבים: ${names}. אם אחד מהם ברשת, ודאו שהוא עדיין מחובר ולחצו על ״סריקה מחדש״. אם הוא נעלם לצמיתות, הסירו אותו מתיקיות הספרייה, ושאר המקומות יוצגו.`,
  emptyChecking:
    `עדיין מחפש בתיקיות. אם אחת מהן ברשת, ודאו שהיא מחוברת ולחצו על ״סריקה מחדש״.`,
  emptyLoadFailed: `לא ניתן היה לטעון את הרשימה. לחצו על ״סריקה מחדש״, או פתחו את האפליקציה מחדש.`,
  wizardPickFolderHint: "בחרו תיקייה ברשימה כדי לראות את התמונות שבה",
  wizardNoImages: "אין תמונות בתיקייה הזו",
  wizardUnreadable: "לא ניתן היה לקרוא את התיקייה הזו (ייתכן שהיא הוסרה)",
  wizardCounting: "טוען…",
  wizardSelectAll: "בחירת הכול",
  wizardSelectNew: "בחירת החדשות בלבד",
  wizardClearSelection: "ניקוי הבחירה",
  wizardSelected: (n: number) => `נבחרו: ${num(n)}`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "כבר יובאה (אותו קובץ קיים ביעד)",
  wizardUnsureBadge: "?",
  wizardUnsureTitle:
    "השם הזה מופיע בכרטיס יותר מפעם אחת. לא ידוע אם הקובץ שביעד הוא התמונה הזו עד שקוראים את התוכן — הייבוא קורא אותו, ואינו יוצר עותק שני של תמונה שכבר קיימת",
  wizardDestination: "יעד",
  wizardChangeDestination: "שינוי",
  wizardStructure: "סידור",
  wizardImportButton: (n: number) => `ייבוא (${num(n)})`,
  wizardImportAll: "ייבוא התיקייה הזו כולה (כולל תיקיות משנה)",
  wizardImportAllShort: "התיקייה כולה",
  wizardDeep: "כולל תיקיות משנה",
  wizardDeepHint: "סורק את כל המדיה, כך שאין צורך לדעת איפה התמונות",
  wizardScanning: "מחפש במדיה…",
  wizardTruncated: (n: number) =>
    `מוצגות רק ${num(n)} הראשונות. השתמשו ב״התיקייה כולה״ כדי לייבא הכול`,
  wizardScanIncomplete: "⚠ לא ניתן היה לקרוא חלק מהתיקיות (ייתכן שחסרות תמונות)",
  // 「N枚に…が無い」は前置詞が数に付く（`לתמונה אחת` / `ל-3 תמונות`）
  decoderHeifNotice: (n: number) =>
    `⚠ ${one(n, "לתמונה אחת", `ל-${num(n)} תמונות`)} בפורמט HEIC/HEIF אין כאן תמונה ממוזערת, ${one(n, "והיא גם לא תיפתח", "והן גם לא ייפתחו")}. נדרשות ״הרחבות תמונה של HEIF״ החינמיות, ואיתן ״הרחבות וידאו של HEVC״ בתשלום של כמה שקלים, שמפענחות את התמונה`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ ${one(n, "לתמונה אחת", `ל-${num(n)} תמונות`)} בפורמט HEIC/HEIF אין כאן תמונה ממוזערת, ${one(n, "והיא גם לא תיפתח", "והן גם לא ייפתחו")}`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ ${one(n, "לתמונה אחת", `ל-${num(n)} תמונות`)} בפורמט HEIC/HEIF אין כאן תמונה ממוזערת, ${one(n, "והיא גם לא תיפתח", "והן גם לא ייפתחו")}. ייתכן שאין במערכת שלכם מפענח ל-HEIC/HEVC`,
  decoderHeifHow: "הרחבות תמונה של HEIF (חינם)",
  decoderHevcHow: "הרחבות וידאו של HEVC (בתשלום)",
  decoderNoticeDismiss: "לא להציג שוב",
  wizardOfflineTitle: "הקובץ הזה נמצא בענן (אין כאן תצוגה מקדימה; הייבוא יוריד אותו)",
  wizardHideImported: "הסתרת מה שכבר יובא",
  wizardAllImported: "אין כאן משהו חדש (כל מה שבתיקייה הזו כבר יובא)",
  wizardHiddenCount: (n: number) => `הוסתרו כי כבר יובאו: ${num(n)}`,
  wizardEtaSeconds: (n: number) => `נותרו כ-${num(n)} שנ׳`,
  wizardEtaMinutes: (n: number) => `נותרו כ-${num(n)} דק׳`,
  wizardEtaCalculating: "מעריך את הזמן שנותר…",
  wizardCapped: (n: number) => `${num(n)}+`,
  wizardMoreFiles: (n: number) => `עוד ${num(n)} (גללו כדי לטעון)`,
  menuOpen: "פתיחה",
  menuOpenWith: (name: string) => `פתיחה באמצעות ${iso(name)}`,
  menuOpenWithOther: "פתיחה באמצעות אפליקציה אחרת…",
  menuReveal: "הצגה בתיקייה",
  menuMoreOptions: "הצג אפשרויות נוספות",
  menuCopyImage: "העתקת התמונה ללוח",
  menuRotateRight: "סיבוב ימינה",
  menuRotateLeft: "סיבוב שמאלה",
  menuRotateRestore: "שחזור הכיוון המקורי",
  menuPrint: "הדפסה…",
  printTitle: "הדפסה",
  printFitContain: "התאמה לדף",
  printFitContainHint: "התמונה נכנסת כולה. אם הצורה שלה שונה מהנייר, נשארים שוליים.",
  printFitCover: "חיתוך למילוי הדף",
  printFitCoverHint: "בלי שוליים, אבל שולי התמונה שלא נכנסים נחתכים.",
  printFitStretch: "מתיחה למילוי הדף (בלי לשמור על הפרופורציות)",
  printFitStretchHint: "בלי שוליים ובלי חיתוך, אבל התמונה נמתחת.",
  printBorderless: "כדי להדפיס בלי שוליים, בחרו גודל נייר ללא שוליים בחלון ההדפסה הבא.",
  printGo: "הדפסה…",
  printPreparing: "מכין להדפסה…",
  printSheetTitle: (n: number) => `הדפסת ${photos(n)}`,
  printPerPage: "תמונות בכל דף",
  printCaptions: "הוספת שם הקובץ ותאריך הצילום",
  printTooMany: (max: number) =>
    `בגודל הזה אפשר להדפיס עד ${photos(max)} בבת אחת. בחרו יותר תמונות בכל דף, או בחרו פחות תמונות.`,
  printPreparingN: (done: number, total: number) => `מכין להדפסה… ${num(done)}/${num(total)}`,
  printSkipped: (n: number) =>
    `לא ניתן היה להדפיס ${items(n)} (סרטונים וכדומה), ולכן ${one(n, "הוא הושמט", "הם הושמטו")}.`,
  errPrint: "לא ניתן היה להכין את התמונה להדפסה.",
  errShellMenu: "לא ניתן היה להציג את התפריט של סייר הקבצים.",
  menuDelete: "מחיקה (העברה לפח האשפה)",
  menuInfo: "מידע",
  fileInfoTitle: "קובץ",
  fileInfoName: "שם",
  fileInfoFolder: "תיקייה",
  fileInfoSize: "גודל",
  fileInfoTaken: "צולם",
  fileInfoTakenUnknown: "לא ידוע",
  fileInfoModified: "שונה",
  fileInfoDimensions: "ממדים",
  fileInfoDuration: "אורך",
  fileInfoType: "סוג",
  selectionTotal: (size: string) => `${iso(size)} בסך הכול`,
  menuFavoriteOn: "הוספה למועדפים",
  menuFavoriteOff: "הסרה מהמועדפים",
  pickEditor: "בחרו אפליקציה לעריכה",
  deleteConfirm: (n: number) =>
    n === 1 ? "להעביר את התמונה הזו לפח האשפה?" : `להעביר ${photos(n)} לפח האשפה?`,
  deleted: (n: number) => `הועברו לפח האשפה: ${num(n)}`,
  deletedSomeLeft: (n: number, left: number) =>
    `הועברו לפח האשפה: ${num(n)} (לא נמצאו ולכן לא טופלו: ${num(left)})`,
  deletedMissing: (n: number, missing: number) =>
    `הועברו לפח האשפה: ${num(n)} (כבר לא בספרייה ולכן לא הועברו: ${num(missing)})`,
  selectItem: "בחירה",
  selectedCount: (n: number) => `נבחרו: ${num(n)}`,
  selectAll: "בחירת הכול",
  clearSelection: "ניקוי הבחירה (Esc)",
  selectDay: "בחירת היום הזה כולו",
  bulkFavoriteOn: "הוספה למועדפים",
  bulkFavoriteOff: "הסרה מהמועדפים",
  bulkDelete: "העברה לפח האשפה",
  bulkCopy: "העתקה לתיקייה",
  bulkMove: "העברה לתיקייה",
  bulkViewer: "הצגת הבחירה",
  pickExportFolder: "בחרו תיקייה לייצא אליה",
  pickMoveFolder: "בחרו תיקייה להעביר אליה",
  moveConfirm: (n: number) =>
    n === 1
      ? "להעביר את התמונה הזו לתיקייה שתבחרו בשלב הבא?\nהיא תוסר ממקומה הנוכחי ומהספרייה.\nסימוני ★ ו-⚑ לא עוברים איתה."
      : `להעביר ${photos(n)} לתיקייה שתבחרו בשלב הבא?\nהן יוסרו ממקומן הנוכחי ומהספרייה.\nסימוני ★ ו-⚑ לא עוברים איתן.`,
  confirmCancel: "ביטול",
  deleteConfirmOk: "העברה לפח האשפה",
  moveConfirmOk: "בחירת מקום להעברה",
  rootRemoveConfirmOk: "הסרה",
  exporting: (done: number, total: number, name: string) =>
    `מייצא… ${num(done)}/${num(total)} ${iso(name)}`,
  exportDone: (done: number, skipped: number, failed: number, missing: number) => {
    const parts: string[] = [];
    // 書き出すものが無かったときに「0枚を書き出しました」から始めない（pictkura-dev #33）
    if (done > 0 || skipped + failed + missing === 0) parts.push(`יוצאו: ${num(done)}`);
    if (skipped > 0) parts.push(`כבר היו שם: ${num(skipped)}`);
    if (failed > 0) parts.push(`נכשלו: ${num(failed)}`);
    if (missing > 0) parts.push(`כבר לא בספרייה ולכן לא יוצאו: ${num(missing)}`);
    return parts.join(". ") + ".";
  },
  moving: (done: number, total: number, name: string) =>
    `מעביר… ${num(done)}/${num(total)} ${iso(name)}`,
  moveDone: (moved: number, skipped: number, failed: number, leftBehind: number, missing: number) => {
    const parts: string[] = [];
    if (moved > 0 || skipped + failed + leftBehind + missing === 0) parts.push(`הועברו: ${num(moved)}`);
    if (skipped > 0) parts.push(`כבר היו שם ונשארו במקומם: ${num(skipped)}`);
    if (failed > 0) parts.push(`לא ניתן היה להעביר ולכן נשארו במקומם: ${num(failed)}`);
    if (leftBehind > 0) parts.push(`הועתקו, אבל לא ניתן היה להסיר אותם מהמקום המקורי: ${num(leftBehind)}`);
    if (missing > 0) parts.push(`כבר לא בספרייה ולכן לא הועברו: ${num(missing)}`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "סימון",
  bulkPickOff: "ביטול סימון",
  bulkPickDone: (n: number) => `סומנו: ${num(n)}`,
  bulkUnpickDone: (n: number) => `הסימון בוטל: ${num(n)}`,
  bulkFavoriteDone: (n: number) => `נוספו למועדפים: ${num(n)}`,
  bulkUnfavoriteDone: (n: number) => `הוסרו מהמועדפים: ${num(n)}`,
  settings: "הגדרות",
  close: "סגירה",
  settingsTitle: "הגדרות",
  settingsImportStructure: "מבנה תיקיות הייבוא",
  settingsImportStructureNote:
    "איך התמונות המיובאות מסודרות לפי תאריך הצילום. זה נוגע באלפי קבצים, ושינוי מאוחר יותר דורש עבודה רבה. כשבשם תיקייה יש תאריך, הוא נכתב עם השנה בהתחלה בכל השפות, כך שמיון לפי שם ממיין לפי זמן.",
  settingsDestination: "יעד",
  settingsDestinationUnset: "(לא הוגדר — תבחרו אותו בייבוא הראשון)",
  settingsFlatExample: "IMG_0001.JPG (בלי תיקיות משנה)",
  settingsCustomPattern: "מותאם אישית",
  // 「{year}, {month}」は間に字が無いので1つの左から右の塊になり、右から読むと月が先に来る。1つずつ閉じる
  settingsCustomPatternNote:
    `התאריך נכתב במקום ${iso("{year}")}, ${iso("{month}")} ו-${iso("{day}")}. השתמשו ב-/ ליצירת תיקיות בתוך תיקיות. תווים שאינם שמישים ומעבר לתיקייה שמעל (..) מושמטים אוטומטית.`,
  settingsCustomPatternResult: "התיקייה שתתקבל",
  settingsGrid: "רשת התמונות",
  settingsStackRawJpegToggle: "איחוד זוגות RAW+JPEG למשבצת אחת",
  settingsStackRawJpegNote:
    `RAW ו-JPEG עם אותו שם באותה תיקייה מוצגים כמשבצת אחת, של ה-JPEG. ברשת, ★, ⚑, מחיקה ובחירה חלים על שניהם. איזה קובץ מוצג במסך מלא נקבע ב״כשמציגים תמונה במסך מלא״.`,
  stackRawChipTitle: (fileCount: number) =>
    `RAW ו-JPEG מאוחדים למשבצת אחת (${files(fileCount)})`,
  settingsStackBurstsToggle: "איחוד צילומים רציפים למשבצת אחת",
  settingsStackBurstsNote:
    "תמונות שצולמו ברצף מהיר באותה מצלמה מוצגות כמשבצת אחת. מאוחדות רק תמונות ששעת הצילום שלהן נרשמה בדיוק של חלקי שנייה (יש מצלמות שאינן רושמות אותה). ברשת, ★ ו-⚑ מתפריט הלחיצה הימנית חלים על תמונת השער בלבד; מחיקה ובחירה חלות על כל הרצף. המסך המלא אינו מאחד רצפים; הוא עובר על כל צילום.",
  settingsBurstGap: "הפער הארוך ביותר בתוך רצף",
  settingsOutgoing: "תיקיית השליחה",
  settingsGoogleToggle: "להניח את התמונות המיובאות גם בתיקיית השליחה",
  settingsGoogleNote:
    "כל ייבוא מסדר את התמונות גם בתיקיית השליחה (בלי לתפוס מקום נוסף בדיסק). מזוגות RAW+JPEG נכנס רק ה-JPEG. אם אתם משתמשים ב-Google Photos: הוסיפו את התיקייה הזו לגיבוי התיקיות ב-Google Photos.",
  settingsGoogleFolder: "תיקייה",
  settingsGoogleOpenFolder: "פתיחת התיקייה",
  settingsGoogleCopyPath: "העתקת הנתיב",
  settingsGoogleCopied: "הועתק",
  settingsGoogleCopyFailed: "לא ניתן היה להעתיק. סמנו את הנתיב והעתיקו אותו.",
  settingsGoogleFolderNote:
    "היא נמצאת באותו כונן שבו נמצא יעד הייבוא. בחרו מקום מחוץ לכל תיקייה שגיבוי אחר עוקב אחריה, כמו NAS או Amazon Photos.",
  settingsGoogleFoldersNote:
    "תיקיות השליחה ש-pictkura הניח בהן תמונות (נוצרת אחת לכל כונן). אם אתם משתמשים ב-Google Photos: הוסיפו לגיבוי התיקיות כל אחת מהן שאתם משתמשים בה.",
  settingsGoogleVideo: "להניח גם סרטונים",
  settingsGoogleRawOnly: "צילומי RAW בלבד",
  googleRawOnlyNone: "השמטה",
  googleRawOnlyEmbedded: "הנחת ה-JPEG המוטמע",
  settingsGoogleRawOnlyNote:
    `מה לעשות בקובצי RAW שצולמו בלי JPEG. ״הנחת ה-JPEG המוטמע״ מוציאה את תמונת התצוגה המקדימה שהמצלמה שמרה בתוך ה-RAW, ומניחה אותה עם תאריך הצילום. ה-JPEG שחולץ הוא קובץ נפרד, ולכן הוא תופס מקום בדיסק.`,
  settingsGoogleOneDrive: "להניח גם תמונות שבתוך OneDrive",
  settingsGoogleOneDriveNote: "אם תעשו זאת, OneDrive כבר לא יוכל להפוך את התמונות האלה לזמינות באינטרנט בלבד.",
  settingsGoogleAfterNote:
    "כשמעבירים תמונה לפח האשפה ב-pictkura, היא מוסרת גם מתיקיית השליחה. אם אתם משתמשים ב-Google Photos: היא נשארת ב-Google Photos. ההעלאה נעשית בדף האינטרנט של Google Photos‏ (photos.google.com), ולכן היא מתקדמת רק כשהדף הזה פתוח בדפדפן.",
  pickGoogleFolder: "בחירת תיקיית השליחה",
  importGoogle: (n: number) => ` · בתיקיית השליחה: ${num(n)}`,
  importGoogleFailed: (n: number) => `לא ניתן היה להניח בתיקיית השליחה: ${num(n)} (ראו ביומן)`,
  importGoogleError: (why: string) => `שום דבר לא הונח בתיקיית השליחה: ${iso(why)}`,
  bulkSendGoogle: "הוספה לתיקיית השליחה",
  bulkRemoveGoogle: "הסרה מתיקיית השליחה",
  calendarSendMonth: "הוספת החודש הזה לתיקיית השליחה",
  calendarSendYear: (y: string) => `הוספת כל שנת ${year(y)} לתיקיית השליחה`,
  googleSendSummary: (photoCount: number, videoCount: number, rawOnly: number, size: string) => {
    const parts: string[] = [];
    if (photoCount > 0) parts.push(photos(photoCount));
    if (videoCount > 0) parts.push(videos(videoCount));
    const list: string[] = [];
    if (parts.length > 0) list.push(`${parts.join(", ")}${size ? ` (${iso(size)})` : ""}`);
    if (rawOnly > 0) list.push(`קובצי JPEG שהוצאו מצילומי RAW בלבד: ${num(rawOnly)}`);
    return `יונחו בתיקיית השליחה: ${list.join("; ")}.`;
  },
  googleSendLeftOut: (n: number) =>
    `מתוך מה שבחרתם ${one(n, "מושמט פריט אחד", `מושמטים ${num(n)} פריטים`)} (ה-RAW של זוג RAW+JPEG, קבצים שבתוך OneDrive וכדומה).`,
  googleSendFolders: (list: string) => `מקומות ההנחה: ${list}`,
  googleSendNewFolder: (path: string) => `התיקייה ${iso(path)} תיווצר.`,
  googleSendGoogleNote:
    "אם אתם משתמשים ב-Google Photos: ההעלאה נעשית בדף האינטרנט של Google Photos‏ (photos.google.com), ולכן היא מתקדמת כל עוד הדף הזה פתוח בדפדפן.",
  googleSendGoogleNoteNew:
    "אם אתם משתמשים ב-Google Photos: הוסיפו גם כל תיקייה חדשה לגיבוי התיקיות ב-Google Photos. ההעלאה נעשית בדף האינטרנט של Google Photos‏ (photos.google.com), ולכן היא מתקדמת כל עוד הדף הזה פתוח בדפדפן.",
  googleFolderCreated: (path: string) =>
    `התיקייה ${iso(path)} נוצרה. אם אתם משתמשים ב-Google Photos: הוסיפו גם את התיקייה הזו לגיבוי התיקיות.`,
  googleSendNothing: "אין כאן משהו שאפשר להניח.",
  googleSendConfirmOk: "הנחה",
  googleSent: (total: number, already: number) =>
    `הונחו בתיקיית השליחה: ${num(total)}${already > 0 ? ` (כבר היו שם: ${num(already)})` : ""}`,
  googleSentCloudOnly: (n: number) =>
    `נמצאים רק בענן ולכן לא הונחו: ${num(n)}. הורידו אותם קודם, ואז שלחו שוב.`,
  googleRemoved: (n: number) => `הוסרו מתיקיית השליחה: ${num(n)}`,
  googleRemoveFailed: (n: number) =>
    `לא ניתן היה להסיר כרגע: ${num(n)} (ייתכן שהכונן מנותק). pictkura ינסה שוב בהפעלה הבאה.`,
  googleSentLeftOut: (n: number) =>
    `${one(n, "הושמט פריט אחד", `הושמטו ${num(n)} פריטים`)} (ה-RAW של זוג RAW+JPEG, קבצים שבתוך OneDrive וכדומה).`,
  burstGapOption: (seconds: number) => `${num(seconds)} שנ׳`,
  burstChip: (frames: number) => `▤ רצף ${num(frames)}`,
  burstChipShort: (frames: number) => `▤ ${num(frames)}`,
  spreadStack: "פריסה כדי לראות כל אחת",
  foldStack: "קיפול",
  burstTitle: (frames: number, spanMs: number, fileCount: number) =>
    `רצף של ${shots(frames)} במשך ${num(Math.round(spanMs / 100) / 10)} שנ׳ (${files(fileCount)})`,
  burstTitleRawJpeg: (frames: number, spanMs: number, fileCount: number) =>
    `רצף של ${shots(frames)} במשך ${num(Math.round(spanMs / 100) / 10)} שנ׳, RAW+JPEG (${files(fileCount)})`,
  settingsViewer: "כשמציגים תמונה במסך מלא",
  settingsAutoAdvanceToggle: "מעבר לתמונה הבאה אחרי P / U",
  settingsAutoAdvanceNote:
    "במסך מלא, P מסמן את התמונה ב-⚑ (מדף נפרד ממועדפי ★), ו-U מנקה את הסימון. כשהאפשרות פועלת, התמונה הבאה מגיעה מיד, כך שסימון דורש מקש אחד לכל תמונה. כשהיא כבויה, נשארים על אותה תמונה.",
  settingsPairView: "מה להציג מזוג RAW+JPEG",
  pairViewJpeg: "JPEG בלבד",
  pairViewRaw: "RAW בלבד",
  pairViewBoth: "שניהם (RAW ואז JPEG)",
  settingsPairViewNote:
    "איך המסך המלא עובר על זוג שמוצג כמשבצת אחת ברשת. כשמוצג צד אחד, ★, ⚑, ✕ ומחיקה במסך מלא חלים על שני הקבצים של הזוג. כששניהם מוצגים, הם חלים רק על הקובץ שמסתכלים עליו. גם הלחצנים RAW / JPEG במסך מלא משנים זאת. אם הזוגות אינם מאוחדים ברשת, שני הקבצים מוצגים.",
  viewerPairSide: "איזה קובץ מהזוג להציג (נשמר גם בהגדרות)",
  settingsTransition: "במעבר לתמונה הבאה",
  settingsSlideshowTransition: "מעבר במצגת",
  transitionNone: "ללא",
  transitionSlide: "החלקה",
  transitionFade: "עמעום",
  settingsTransitionNote:
    "איך המסך המלא עובר לתמונה הבאה כשמדפדפים (מקשי החצים, הלחצנים, הרצועה שבתחתית, או מעבר אחרי הערכה), וכשמצגת ממשיכה. ההחלקה הולכת בכיוון הדפדוף. אם המערכת שלכם מוגדרת להפחתת תנועה, התמונות מתחלפות בלי תנועה, מה שלא תבחרו כאן.",
  settingsAutoplay: "כשמחברים זיכרון USB או כרטיס SD",
  settingsAutoplayNote:
    `כש-pictkura מותקן בעזרת תוכנית ההתקנה או מ-Microsoft Store, האפשרות ״pictkura で写真を取り込む״ (ייבוא תמונות בעזרת pictkura) מופיעה בין האפשרויות כשמחברים כונן (שם האפשרות ביפנית); הגרסה הניידת אינה מציעה אותה. שום דבר לא מתחיל עד שבוחרים בה. אחרי שבוחרים בה, Windows זוכר אותה כברירת המחדל לסוג המדיה הזה (כרטיסי זיכרון וכונני USB מוגדרים בנפרד), ופותח את חלון הייבוא בכל פעם שמחברים מדיה מאותו סוג. את ברירת המחדל משנים בהגדרות ״הפעלה אוטומטית״ של Windows. בתוכנית ההתקנה MSI באנגלית, האפשרות נקראת ״Import photos with pictkura״.`,
  settingsAutoplayOpenWindows: "פתיחת הגדרות ההפעלה האוטומטית של Windows",
  settingsImportWaitingFrom: (path: string) =>
    `ייבוא מ-${iso(path)} ממתין. הוא ייפתח כשתסגרו את ההגדרות.`,
  settingsImportWaiting: "ייבוא ממתין. הוא ייפתח כשתסגרו את ההגדרות.",
  settingsImportWaitingOpen: "סגירה וייבוא",
  settingsAbout: "אודות",
  settingsAboutLicense: "מופץ ברישיון MIT.",
  settingsManual: "מדריך",
  settingsOssLicenses: "קוד פתוח שבשימוש",
  settingsDocNotBundled: "(לא כלול בגרסת פיתוח)",
  settingsLog: "פתיחת היומן",
  settingsLogNone: "(עדיין לא נרשם דבר)",
  settingsOpenFailed: "לא ניתן היה לפתוח.",
  settingsLogNote: "שורה נכתבת רק כשמשהו נכשל, והיא נשארת במחשב הזה. שום דבר לא נשלח.",
  settingsLanguage: "שפה",
  settingsLanguageSystem: "לפי המערכת",
  settingsLanguageNote:
    "ההחלפה טוענת את החלון מחדש. ההעתקה עצמה ממשיכה ברקע, אבל אשף הייבוא נסגר ולא תוכלו לראות את ההתקדמות שלו — לכן כדאי לחכות שהייבוא יסתיים לפני ההחלפה.",
  settingsLanguageMacNote:
    "חלונות של macOS, כמו חלון השמירה וחלונות האישור, יעברו לשפה הזו בפעם הבאה שתפעילו את pictkura.",
  settingsTheme: "ערכת נושא",
  themeSystem: "לפי המערכת",
  themeLight: "בהירה",
  themeDark: "כהה",
  settingsEditors: "אפליקציות עריכה",
  settingsEditorsNote: `אפליקציות שבחרתם ב״פתיחה באמצעות אפליקציה אחרת…״.`,
  settingsForgetEditor: "הסרה מהרשימה",
  calendarEmpty: "אין תמונות",
  filterEmptyTitle: (conds: string) => `אין תמונות שתואמות לסינון הזה: ${conds}`,
  filterEmptyMessage: "נקו את החיפוש או את הסינון שבצד כדי לראות את השאר.",
  filterEmptyStillIndexing: "תמונות עדיין מיובאות או נוספות לאינדקס, ולכן ייתכן שיופיעו עוד.",
  filterCondQuery: (q: string) => `החיפוש ״${iso(q)}״`,
  filterCondCamera: (name: string) => `צולמו ב-${iso(name)}`,
  filterEmptyClearSearch: "ניקוי החיפוש",
  errGoogleOverlapsRoot: "אי אפשר לבחור תיקיית ספרייה, תיקייה שבתוכה, או תיקייה שמכילה אותה.",
  errGoogleInsideSync:
    "אי אפשר לבחור מקום בתוך תיקיית סנכרון (OneDrive, Google Drive, Dropbox, iCloud Drive וכדומה).",
  errGoogleNoHardLinks: "הכונן הזה לא יכול להכיל אותה (ב-exFAT, ב-FAT ובדומיהם אי אפשר ליצור קישורים קשיחים).",
  errGoogleWholeVolume:
    "יעד הייבוא הוא כונן שלם, ולכן אין בו מקום לתיקיית השליחה. שנו את יעד הייבוא.",
  errGoogleNoName: "אי אפשר לבחור את הכונן עצמו. בחרו תיקייה בתוכו.",
  errGoogleLinkInTheWay: "תיקיית השליחה היא קישור סמלי. בחרו מקום אחר.",
  errGoogleIo: "לא ניתן היה לקרוא מתיקיית השליחה או לכתוב אליה.",
  errGoogleOtherDrive: "בחרו תיקייה באותו כונן שבו נמצא יעד הייבוא.",
  errNotFound: "התמונה הזו כבר לא באינדקס.",
  errRotateNoOrientation: "אין לתמונה הזו מידע על כיוון, ולכן אי אפשר לסובב אותה.",
  errRotateXmp:
    "זה יתנגש בכיוון שתוכנות עריכת תמונות קוראות מ-XMP, ולכן אי אפשר לסובב את התמונה הזו.",
  errRotateCloudOnly: "התמונה הזו נמצאת רק בענן, ולכן אי אפשר לסובב אותה. הורידו אותה ונסו שוב.",
  errRotateCameraCard:
    "תמונות בכרטיס המצלמה נשמרות כמו שהן, כי מייבאים משם. ייבאו את התמונה ואז סובבו אותה.",
  errRotateNotReady: "התמונה הזו עדיין נקראת. חכו רגע ונסו שוב.",
  errRotateChanged: "הקובץ השתנה ממקום אחר בזמן הסיבוב. נסו שוב.",
  errRotateBusy: "בודק את התיקיות שלכם. חכו רגע ונסו שוב.",
  errRotateWrite:
    "לא ניתן היה לכתוב לקובץ, ולכן התמונה לא סובבה. ודאו שהקובץ לא פתוח באפליקציה אחרת ושאינו לקריאה בלבד.",
  errRotateLeftChanged:
    "משהו השתבש אחרי שהקובץ נכתב מחדש, ולא ניתן היה להחזיר אותו למצבו. ייתכן שהתמונה עדיין מסובבת. פרטים ביומן.",
  errRotateCannotRestore:
    "כבר אי אפשר להחזיר את התמונה הזו לכיוון שהיה לה לפני ש-pictkura סובב אותה, למשל כי אפליקציה אחרת שינתה אותה.",
  errDb: "לא ניתן היה לקרוא מהאינדקס או לכתוב אליו.",
  errConfigIo: "לא ניתן היה לקרוא את קובץ ההגדרות או לכתוב אליו.",
  errConfigFormat: "לא ניתן היה להבין את קובץ ההגדרות.",
  errNoDestination: "עדיין לא נבחרה תיקיית יעד.",
  errRootManaged: "אי אפשר להוסיף ספרייה מנוהלת כתיקייה — מה שבתוכה שייך לאותה אפליקציה.",
  errDestManaged: "ספרייה מנוהלת לא יכולה להיות היעד — מה שבתוכה שייך לאותה אפליקציה.",
  errNoImageToExtract: "אין בקובץ הזה תמונה שאפשר להוציא.",
  errSourceUnreadable: "לא ניתן היה לקרוא את התיקייה הזו.",
  errSourceManaged:
    "אי אפשר לייבא מתוך ספרייה מנוהלת. הסירו את הסיומת (‎.photoslibrary וכדומה) משם התיקייה ובחרו אותה שוב.",
  errExportDest: "לא ניתן היה ליצור את תיקיית היעד הזו.",
  errExportManaged: "אי אפשר לכתוב לשם — אפליקציה אחרת מנהלת את החבילה הזו.",
  errFolderMissing: "התיקייה הזו לא נמצאת.",
  fileOpBusy: "חכו שהפעולה הנוכחית תסתיים, ואז נסו שוב.",
  errFolderNotAbsolute: "הזינו את הנתיב המלא של התיקייה.",
  errTrashFailed: "לא ניתן היה להעביר לפח האשפה.",
  errTrashUnreachable: "אין גישה לכונן או לתיקייה, ולכן שום דבר לא הועבר לפח האשפה.",
  errTrashPartly: (n: number) => `הועברו לפח האשפה: ${num(n)}; את השאר לא ניתן היה להעביר.`,
  errNoLogYet: "עדיין לא נרשם דבר.",
  errOutgoingFolderMissing:
    "תיקיית השליחה לא נמצאת. הפעלת תיקיית השליחה יוצרת אותה (אם היא כבר פועלת, כבו והפעילו אותה מחדש).",
  errOpenOutgoingFailed: `לא ניתן היה לפתוח את תיקיית השליחה. השתמשו ב״העתקת הנתיב״ ופתחו אותה משם.`,
  errNotBundled: "לא כלול בגרסה הזו.",
  errNoStoreLink: "אין דף להפנות אליו במערכת הזו.",
  errBadKind: "לא היה מה לפתוח.",
  errTooManyIds: (n: number) => `לא יותר מ-${num(n)} בכל פעם.`,
};
