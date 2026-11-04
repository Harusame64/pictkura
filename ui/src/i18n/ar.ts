/**
 * アラビア語の辞書（現代標準アラビア語 MSA）。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **地域の方言ではなく、Apple・Microsoft のアラビア語版と同じ標準語で書く。** 語は発明せず、
 * 既にアラビア語圏で使われているものへ合わせる（独・西・仏・葡・韓と同じ方針）:
 *
 * - **右から左へ書く言語。** 画面の鏡は `<html dir>` と CSS が受け持つ（`direction.ts`）。辞書が
 *   受け持つのは矢印だけ——右→左では ← が「次」なので、`viewerPrev` は「→」、`viewerNext` は「←」。
 *   ショートカット一覧の「← / →」は文字列のまま置き、説明を「次 / 前」の順にする（右→左の行では
 *   左右の並びが逆に描かれ、右から読むと ← が先に来るので、先に読む ← が「次」と揃う）
 * - **⚑ / ✕ は `انتقاء`（拾う）/ `رفض`（退ける）**。⚑ の印の付いた写真は `منتقاة`、✕ は `مرفوضة`。
 *   **⚑ に `تحديد` を使わない**——複数選択が `تحديد`（Apple の Select All は `تحديد الكل`）なので、
 *   西語の `indicador`・葡語の `escolher` と同じ理由で ⚑ 側は `انتقاء` に限る。⚑ と ✕ の両方を覆う
 *   文字列（`judgeUnflag`、ショートカットの U の行）は、日本語の「判定」と同じ中立の `التقييم` にする
 * - **アプリの蔵書は `المكتبة`、写真アプリの蔵書は `مكتبة تطبيق الصور`**。Apple は Photo Library を
 *   `مكتبة الصور` と訳すが、そのままだとアプリ側の `المكتبة` と見分けが付かない（「ライブラリの
 *   フォルダが写真アプリのライブラリそのもの」の文で同じ語がぶつかる）ので、アプリの名前を挟む。
 *   写真アプリの外の管理されたライブラリは `مكتبة مُدارة`
 * - **送り出しは `مجلد الإرسال`、書き出しは `تصدير`**——ja が「送り出し」と「書き出し」を分けるのと同じく割る
 * - **ゴミ箱は Apple の `سلة المهملات`**。Windows の ar は `سلة المحذوفات` だが、どちらの OS の人にも
 *   通じる一般の語の方を1つに決める（英語の `trash` と同じく、OS の固有名ではなく普通名詞として書く）
 * - **OSの用語は引いてくる**。Apple の ar（loctable で確認）: `إلغاء`・`حذف`・`نسخ`・`حفظ`・
 *   `تحديد الكل`・`تدوير إلى اليسار` / `تدوير إلى اليمين`・`طباعة…`・`الإعدادات…`・
 *   `إظهار في فايندر`、macOS の `إعدادات النظام ← الخصوصية والأمن`。Windows の ar:
 *   `التشغيل التلقائي`（自動再生）・`إظهار المزيد من الخيارات`（Windows 11 のエクスプローラー）・
 *   `مستكشف الملفات`・`ارتباطات ثابتة`（ハードリンク）・Microsoft Store の `امتدادات صور HEIF` /
 *   `امتدادات فيديو HEVC`
 * - **金額は通貨で言わない**（`مدفوعة وزهيدة الثمن`）。アラビア語の利用者は多くの国に居るので、
 *   ja の「数百円」を特定の通貨に直さない
 * - **確認画面の取消は `إلغاء`**（AppKit の Cancel。違うと macOS で Esc が効かない）
 * - **自動再生の項目名は日本語のまま引く**（Windows に登録してあるのは日本語だけ。葡・韓と同じ）
 * - キーの文字（P・X・U・F・I…）と製品名はラテン文字のまま。文に差し込むパス・名前・検索語・理由は
 *   下の `iso()` で向きを閉じる（要素で囲える所は画面の側の `.path` / `.name` が囲う）
 *
 * **数は `arabic()` の6形**（`plural.ts`）。名詞の形が数で変わる:
 * - **0**: 画面の数の札として `0 صورة`（数字＋単数）。「ありません」と文で言う所は文で言う
 * - **1**: 数字を付けずに `صورة واحدة`（名詞＋「1つの」）
 * - **2**: 数字を付けずに双数 `صورتان`。**格で語尾が変わる**——主語（主格）なら `صورتان`、
 *   動作名詞の目的語や前置詞の後（属格・対格）なら `صورتين`。下の名詞の関数は `obl`（斜格）で選ぶ
 * - **3〜10**: 複数の属格（`3 صور`）
 * - **11〜99**: 単数の対格（`11 صورة`）——数字の後ろなので語尾の母音は書かない
 * - **100〜**: 単数の属格（`100 صورة`）
 *
 * **動詞と形容詞の一致を避ける書き方にする。** 数えた名詞に動詞を後ろから合わせると、性（`صورة` は
 * 女性、`ملف`・`فيديو`・`مجلد`・`عنصر` は男性）と数（単・双・複）で6通りに割れる。多くの文を
 * 「`تم` ＋動作名詞＋数えた物」（`تم نقل 3 صور`＝3枚の移動がなされた）か、動詞を前に置く形
 * （動詞が前なら数に関わらず単数で、性だけ合わせればよい）で書き、形の組み合わせを減らす。
 */
import { isolate } from "../direction.ts";
import { folderExample } from "./folderExample.ts";
import { arabic, num, year } from "./plural.ts";
import type { Dict } from "./ja.ts";

/**
 * 数えた名詞を数に合わせて組む。`obl` は斜格（動作名詞の目的語・前置詞の後）——双数の語尾だけが変わる
 * （主格 `-ان`、斜格 `-ين`）。1 と 2 は数字を付けない
 */
const counted =
  (one: string, twoNom: string, twoObl: string, few: string, many: string, other = many) =>
  (n: number, obl = true): string =>
    arabic(n, {
      zero: `${num(n)} ${other}`,
      one,
      two: obl ? twoObl : twoNom,
      few: `${num(n)} ${few}`,
      // 11〜99 は単数の対格（男性名詞は語尾に ًا が付く: `11 ملفًا`）、100〜 と 0 は単数の属格（`100 ملف`）
      many: `${num(n)} ${many}`,
      other: `${num(n)} ${other}`,
    });

/** パス・名前・検索語・失敗の理由は、向きを閉じて差し込む（`direction.ts` の `isolate`） */
const iso = isolate;

/** 写真（女性） */
const photos = counted("صورة واحدة", "صورتان", "صورتين", "صور", "صورة");
/** ファイル（男性） */
const files = counted("ملف واحد", "ملفان", "ملفين", "ملفات", "ملفًا", "ملف");
/** 動画（男性。`فيديو` は外来語で、複数は `فيديوهات`） */
// `فيديو` は語尾が変わらない（外来語）ので、11〜99 と 100〜 は同じ形
const videos = counted("فيديو واحد", "فيديوهان", "فيديوهين", "فيديوهات", "فيديو");
/** 項目（男性。写真と動画が混ざる物） */
const items = counted("عنصر واحد", "عنصران", "عنصرين", "عناصر", "عنصرًا", "عنصر");
/** 連写の1枚（女性） */
const shots = counted("لقطة واحدة", "لقطتان", "لقطتين", "لقطات", "لقطة");

export const ar: Dict = {
  appName: "pictkura",
  viewThumbnails: "الصور",
  viewCalendar: "التقويم",
  searchPlaceholder: "ابحث عن ملف أو مجلد أو كاميرا أو 2019-08 أو year:2019",
  searchClear: "مسح البحث (Esc)",
  commandPalette: "لوحة الأوامر",
  // 「من USB」は改行しない空白でつなぐ——狭い窓で3行に折れて、上の帯からはみ出した（win の実測、960 幅）
  importFromUsb: "استيراد من\u00a0USB",
  rescan: "إعادة الفحص",
  size: "الحجم",
  itemsCount: (n: number) => items(n, false),
  navPlaces: "الأماكن",
  navAllPhotos: "كل الصور",
  navFavorites: "★ المفضلة",
  navPicked: "⚑ المنتقاة",
  navOutgoing: "مجلد الإرسال",
  outgoingGuide:
    "اختر مجلد الإرسال هذا في تطبيق خدمة الصور لديك كمجلد للنسخ الاحتياطي (المزامنة). بعد ذلك، كلما استوردت صورًا تُوضع هنا وتلتقطها الخدمة. إذا كنت تستخدم صور Google: أضف هذا المجلد إلى النسخ الاحتياطي للمجلدات في صور Google.",
  navKinds: "النوع",
  kindPhoto: "الصور",
  kindRaw: "RAW",
  kindVideo: "الفيديوهات",
  shortcutsTitle: "اختصارات لوحة المفاتيح (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "عرض اختصارات لوحة المفاتيح",
  shortcutGroups: [
    {
      title: "الشبكة",
      keys: [
        ["Ctrl+K / ⌘K", "لوحة الأوامر (الانتقال إلى تاريخ أو كاميرا، والبحث، والاستيراد)"],
        ["Ctrl+, / ⌘,", "فتح الإعدادات (اللوحة نفسها التي يفتحها الترس في الشريط)"],
        ["Ctrl+A / ⌘A", "تحديد كل ما يطابق البحث والتصفية الحاليين"],
        ["Shift + نقر", "تحديد كل ما بين آخر مربع نقرته وهذا المربع"],
        ["Ctrl + نقر", "إضافة صورة واحدة أو إزالتها (⌘ + نقر على macOS)"],
        ["النقر على عنوان التاريخ", "تحديد اليوم كله (انقر مرة أخرى للإلغاء)"],
        ["Esc", "إنهاء التحديد"],
      ],
    },
    {
      title: "العارض",
      keys: [
        // 右→左の行では「← / →」が右から ←・→ の順に描かれる。先に読む ← が「次」
        ["← / →", "الصورة التالية / السابقة"],
        ["P", "انتقاء الصورة (⚑). افتراضيًا ينتقل إلى الصورة التالية"],
        ["X", "رفض الصورة (✕). تُنقل الصور المرفوضة إلى سلة المهملات عند إغلاق العارض"],
        ["U", "إلغاء التقييم على هذه الصورة (إزالة ⚑ و✕)"],
        ["Ctrl+C / ⌘C", "نسخ الصورة المعروضة إلى الحافظة"],
        ["Ctrl+S / ⌘S", "حفظ الصورة المعروضة في ملف"],
        ["Ctrl+P / ⌘P", "طباعة الصورة المعروضة"],
        ["F", "إضافة إلى المفضلة أو إزالة منها (★)"],
        ["I", "تفاصيل الملف والتصوير (الكاميرا والعدسة وفتحة العدسة والغالق وISO والبعد البؤري وGPS)"],
        ["مسافة", "عرض الشرائح. في الفيديو: تشغيل / إيقاف مؤقت"],
        ["1 / 0", "1 يبدّل بين الحجم الفعلي 100% وملاءمة الشاشة؛ 0 يلائم الشاشة دائمًا"],
        ["F11", "ملء الشاشة"],
        ["Esc", "إغلاق"],
      ],
    },
    {
      title: "الفأرة (في العارض)",
      keys: [
        ["نقر مزدوج", "الحجم الفعلي 100% ⇔ ملاءمة الشاشة"],
        ["العجلة", "تكبير / تصغير"],
        ["السحب", "التنقل أثناء التكبير"],
        ["النقر بالزر الأيمن", "القائمة (فتح، نسخ، تدوير، طباعة، معلومات، حذف وغير ذلك)"],
        ["النقر على الشريط السفلي", "الانتقال إلى تلك الصورة"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "الكاميرات والوسائط",
  navLibraryFolders: "مجلدات المكتبة",
  navDrives: "محركات الأقراص",
  navAddFolder: "إضافة مجلد",
  add: "إضافة",
  browse: "استعراض…",
  addFolderPlaceholder: folderExample("مثال: ", "user"),
  pickLibraryFolder: "اختر مجلدًا لإضافته إلى المكتبة",
  showMore: (n: number) => `${num(n)} أخرى`,
  collapse: "عرض أقل",
  photosCount: (n: number) => `${num(n)}`,
  memoriesTitle: (years: number) =>
    `في مثل هذا اليوم ${arabic(years, {
      zero: "من هذا العام",
      one: "قبل سنة",
      two: "قبل سنتين",
      few: `قبل ${num(years)} سنوات`,
      many: `قبل ${num(years)} سنة`,
      other: `قبل ${num(years)} سنة`,
    })}`,
  viewerFavorite: "المفضلة (F)",
  viewerPick: "انتقاء (P)",
  viewerUnpick: "إلغاء الانتقاء (U)",
  viewerPicked: "صورة منتقاة",
  judgeFav: "مفضلة",
  judgeUnfav: "أُزيلت من المفضلة",
  judgePick: "منتقاة",
  judgeUnflag: "أُلغي التقييم",
  viewerReject: "رفض (X)",
  viewerRejected: "مرفوضة",
  favoriteMark: "مفضلة",
  viewerMutedHint: "الصوت مكتوم — انقر لتشغيله",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "مراجعة الصور المرفوضة",
  rejectGateTitle: (n: number) => `نقل ${photos(n)} إلى سلة المهملات`,
  rejectGateNote: "يمكن استعادتها من سلة المهملات (وتعود إلى المكتبة أيضًا).",
  rejectGateRestore: "إبقاء",
  rejectGateBack: "رجوع",
  rejectGateDiscard: "إغلاق دون حذف",
  rejectGateConfirm: (n: number) => `نقل ${photos(n)} إلى سلة المهملات`,
  rejectGateTrashing: (done: number, total: number) =>
    `جارٍ النقل… (${num(done)} / ${num(total)})`,
  updateFound: (v: string) => `يتوفر إصدار جديد${v ? ` (${iso(v)})` : ""}`,
  updateOpenPage: "فتح صفحة التنزيل",
  updateLater: "لاحقًا",
  updateCheckNow: "التحقق من وجود تحديثات",
  updateChecking: "جارٍ التحقق…",
  updateUpToDate: "لديك أحدث إصدار",
  updateFailed: "تعذّر التحقق",
  updateOnStart: "التحقق من وجود تحديثات عند بدء التشغيل",
  updateOnStartNote:
    "يسأل GitHub عن اسم أحدث إصدار (مرة في اليوم). لا تُرسل أي صور ولا أسماء ملفات. إذا أوقفته، فلن يخرج أي شيء من هذا الجهاز إلا عند الضغط على زر “التحقق من وجود تحديثات”.",
  viewerSlideshow: "عرض الشرائح (مسافة)",
  extractSave: (key: string) => `حفظ هذه الصورة في ملف (${key})`,
  extractCopy: (key: string) => `نسخ هذه الصورة إلى الحافظة (${key})`,
  extractSaveTitle: "حفظ الصورة باسم",
  extractFilter: "صورة",
  extractSaved: "تم الحفظ",
  extractCopied: "تم النسخ",
  extractCopying: "جارٍ النسخ…",
  extractFailed: "تعذّر استخراج هذه الصورة",
  extractSameFile: "لا يمكن الحفظ فوق الملف الأصلي",
  viewerExif: "معلومات الصورة (I)",
  viewerFullscreen: "ملء الشاشة (F11)",
  viewerClose: "إغلاق (Esc)",
  // 右→左の画面では → が「前」、← が「次」（direction.ts の stepOfArrow）
  viewerPrev: "السابقة (→)",
  viewerNext: "التالية (←)",
  viewerFitToScreen: "ملاءمة الشاشة (0)",
  viewerActualSize: "الحجم الفعلي 100% (1) — أو انقر نقرًا مزدوجًا",
  actualSizeBadge: "1:1",
  videoUnsupported: "لا يمكن تشغيل هذا التنسيق في التطبيق",
  fileMissing: "هذا الملف غير موجود (ربما فُصلت ذاكرة USB أو بطاقة SD، أو نُقل الملف أو حُذف)",
  fileUnreachable: "لا يمكن فتح هذا الملف (محرك الأقراص غير جاهز، أو الوصول غير مسموح)",
  fileNotShown: "لا يمكن عرض هذا الملف (الملف تالف، أو لا يستطيع pictkura قراءة هذا التنسيق)",
  fileNotDownloaded: "لم يُنزَّل هذا الملف بعد (افتحه مرة أخرى عند الاتصال بالإنترنت)",
  videoCloudOnly: "هذا الفيديو موجود في السحابة",
  videoCloudOnlyNote:
    "تشغيله هنا يبدأ التنزيل ولا يعرض شيئًا حتى ينتهي. فتحه في التطبيق الافتراضي يتيح لك متابعة تقدّم التنزيل.",
  videoFailed: "تعذّر تشغيل هذا الفيديو",
  videoOpenExternal: "فتح في التطبيق الافتراضي",
  videoCodecNote:
    "تُسجَّل فيديوهات iPhone والكاميرات المشابهة بتنسيق HEVC (H.265). يحتاج تشغيلها إلى برنامج فك ترميز في النظام؛ وفي Windows هو امتداد مدفوع وزهيد الثمن من Microsoft Store.",
  videoCodecNoteMac:
    "يفك macOS ترميز HEVC من دون إضافات، لذا فالأرجح أن هذا تنسيق تسجيل لا يدعمه.",
  videoCodecNoteOther: "قد لا يتوفر في نظامك برنامج فك ترميز لتنسيق التسجيل هذا.",
  videoCodecHelp: "الحصول على امتدادات فيديو HEVC (مدفوعة)",
  loading: "جارٍ التحميل…",
  exifTitle: "معلومات الصورة",
  exifCamera: "الكاميرا",
  exifLens: "العدسة",
  exifAperture: "فتحة العدسة",
  exifShutter: "الغالق",
  exifIso: "ISO",
  exifFocal: "البعد البؤري",
  exifLocation: "الموقع",
  exifNone: "لا توجد بيانات EXIF",
  paletteInput: "تاريخ أو كاميرا أو كلمة أو أمر…",
  paletteNoResults: "لا نتائج",
  paletteGroupJumpDate: "الانتقال إلى تاريخ",
  paletteGroupRecentDays: "الأيام الأخيرة",
  paletteGroupCameras: "التصفية حسب الكاميرا",
  paletteGroupSearch: "البحث",
  paletteGroupActions: "الإجراءات",
  paletteSearchFor: (q: string) => `البحث عن “${iso(q)}”`,
  paletteSearchHint: "اسم الملف، المجلد، الكاميرا",
  paletteSelect: "اختيار",
  paletteRun: "تنفيذ",
  paletteCloseHint: "إغلاق",
  actionShowFavorites: "عرض المفضلة فقط",
  actionShowPicked: "عرض المنتقاة فقط",
  actionShowAll: "عرض كل الصور",
  actionCalendar: "عرض التقويم",
  actionThumbnails: "شبكة الصور",
  indexBuilding: "🔍 جارٍ إنشاء فهرس البحث… ",
  bgScanning: "جارٍ تحديث المكتبة…",
  bgThumbs: (n: number) => `جارٍ قراءة الصور (المتبقي ${num(n)})`,
  bgScanningShort: "جارٍ التحديث",
  bgThumbsShort: (n: number) => `المتبقي ${num(n)}`,
  cameraScanning: "📷 جارٍ قراءة معلومات الكاميرا… ",
  indexIncompleteWarning:
    "⚠ توقفت فهرسة البحث — قد تكون النتائج ناقصة (تُستأنف عند التشغيل التالي)",
  indexProgressSuffix: "% — قد تكون النتائج ناقصة حتى ينتهي هذا",
  removeRoot: (path: string) => `إزالة ${iso(path)} من المكتبة`,
  rootMissingNotice: (name: string, count: number) =>
    `المجلد “${iso(name)}” غير موجود${count > 0 ? ` (يتعذّر فتح ${photos(count)} فيه)` : ""}. إذا كان على ذاكرة USB أو بطاقة SD، فوصِّلها واضغط على “إعادة الفحص”.`,
  rootsMissingNotice: (names: string, count: number) =>
    `بعض المجلدات غير موجودة: ${iso(names)}${count > 0 ? ` (يتعذّر فتح ${photos(count)} فيها)` : ""}. إذا كانت على ذاكرة USB أو بطاقة SD، فأعد توصيلها واضغط على “إعادة الفحص”. للتوقف عن استخدام مجلد، أزِله من المكتبة بزر ✕ في القائمة الجانبية (لا تُحذف ملفات الصور).`,
  rootRemoveFromLibrary: "إزالة من المكتبة",
  rootRemoveKeepsFiles: "يزيل المجلد من المكتبة فقط. لا تُحذف ملفات الصور",
  rootRemoveConfirm: (name: string) =>
    `هل تريد إزالة “${iso(name)}” من المكتبة؟\nلا تُحذف ملفات الصور، لكن علامات ★ و⚑ على صوره تُحذف.`,
  rootTempConfirm: (path: string) =>
    `المجلد “${iso(path)}” داخل مجلد مؤقت.\nقد يحذف النظام أو تطبيقات أخرى الملفات فيه دون علمك. هل تريد إضافته إلى المكتبة على أي حال؟`,
  rootTempConfirmOk: "إضافة على أي حال",
  destTempConfirm: (path: string) =>
    `المجلد “${iso(path)}” داخل مجلد مؤقت.\nقد يحذف النظام أو تطبيقات أخرى الصور المستوردة إليه دون علمك. وبعد مسح البطاقة لن تبقى في أي مكان آخر. هل تريد استخدامه وجهةً على أي حال؟`,
  destTempConfirmOk: "استخدامه وجهةً",
  destTempWarning: "⚠ الوجهة داخل مجلد مؤقت. قد تختفي الصور المستوردة دون إنذار.",
  rootTempTip: (path: string) =>
    `${iso(path)} — داخل مجلد مؤقت. قد يحذف النظام أو تطبيقات أخرى الملفات فيه دون علمك`,
  rootMissingTempTip: (path: string) =>
    `${iso(path)} — غير موجود. كان داخل مجلد مؤقت، فربما حذفه النظام أو تطبيق آخر`,
  rootMissingTip: (path: string) =>
    `${iso(path)} — غير موجود. إذا كان على ذاكرة USB أو بطاقة SD، فوصِّلها واضغط على “إعادة الفحص”`,
  importFrom: (path: string) => `استيراد من ${iso(path)}`,
  filterByCamera: (name: string) => `عرض الصور الملتقطة بـ ${iso(name)} فقط`,
  jumpToYear: (y: number) => `الانتقال إلى ${year(y)}`,
  importing: (done: number, total: number) => `جارٍ الاستيراد… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `اكتمل الاستيراد: نُسخ ${num(copied)}، وتُخطّي ${num(skipped)}`,
  importFailed: (n: number) => `، وفشل ${num(n)}`,
  importIncomplete: " ⚠ تعذّرت قراءة بعض المجلدات — لا تمسح البطاقة بعد",
  syncDone: (added: number, changed: number, removed: number) =>
    `أُضيف ${num(added)}، وتغيّر ${num(changed)}، وأُزيل ${num(removed)}`,
  drivesReturnedScanned: (added: number, changed: number) =>
    `أُعيدت قراءة المجلدات على محرك الأقراص الموصول (أُضيف ${num(added)}، وتغيّر ${num(changed)})`,
  pickSource: "اختر المجلد الذي تستورد منه (USB / DCIM)",
  pickDestination: "اختر مجلد الوجهة",
  wizardTitle: "استيراد",
  wizardSources: "المصدر",
  wizardOtherFolder: "مجلد آخر…",
  wizardRefresh: "إعادة فحص محركات الأقراص",
  wizardRemovable: "قابلة للإزالة",
  wizardNoDrives: "لم يُعثر على محركات أقراص",
  emptyTitle: "لا توجد صور بعد",
  emptyTitleFailed: "تعذّر عرض القائمة",
  emptyTitleChecking: "جارٍ التحقق",
  emptyTitleStartupFailed: "لم تكتمل المزامنة عند بدء التشغيل",
  emptyStartupFailed:
    "لم تكتمل المزامنة التي تجري عند بدء التشغيل. قد توجد صور لم تُضَف بعد. اضغط على “إعادة الفحص”؛ وإن لم يُجدِ ذلك، فأعد فتح التطبيق.",
  emptyTitleMissing: "بعض الأماكن غير موجودة",
  emptyTitleUnreadable: "تعذّر فتح بعض الأماكن",
  emptyNoRoots:
    "لم يُضبط أي مجلد للمكتبة بعد. استورد من بطاقة، أو اختر مجلدًا فيه صور.",
  emptyMissing: (names: string) =>
    `هذه الأماكن غير موجودة: ${iso(names)}. إذا كانت على ذاكرة USB أو بطاقة SD، فأعد توصيلها واضغط على “إعادة الفحص”.`,
  emptyUnreadableMac: (names: string) =>
    `تعذّر فتح هذه الأماكن: ${iso(names)}. امنح pictkura حق الوصول إلى ذلك المجلد (سطح المكتب، المستندات، قرص خارجي) من إعدادات النظام ← الخصوصية والأمن. وإذا كان على الشبكة، فتأكد من أنه متصل واضغط على “إعادة الفحص”.`,
  emptyUnreadableWin: (names: string) =>
    `تعذّر فتح هذه الأماكن: ${iso(names)}. تحقق من أذونات المجلد. وإذا كان محرك أقراص على الشبكة، فتأكد من أنه متصل واضغط على “إعادة الفحص”.`,
  emptyUnreadableOther: (names: string) =>
    `تعذّر فتح هذه الأماكن: ${iso(names)}. تأكد من أن لديك إذنًا بقراءتها، ثم اضغط على “إعادة الفحص”.`,
  listSeparator: "، ",
  listLastSeparator: " ",
  andMore: (n: number) => `و${num(n)} غيرها`,
  emptyRootIsPackage:
    "أحد مجلدات المكتبة هو مكتبة تطبيق الصور نفسها. لا يقرأ pictkura داخلها عن قصد، لذا لن يأتي منها شيء. اختر مجلدًا فيه صور، أو استورد من بطاقة.",
  emptyPhotoLibrary:
    "لم يُعثر إلا على مكتبة تطبيق الصور. لا يقرأ pictkura داخل تطبيق الصور افتراضيًا — فمعظم الملفات الأصلية موجودة في iCloud وليست على هذا الجهاز. استورد من بطاقة، أو اختر مجلدًا فيه صور.",
  emptyManagedLibrary:
    "لم يُعثر إلا على مكتبات مُدارة لتطبيقات الصور (الصور أو iPhoto أو Aperture). لا يقرأ pictkura داخلها عن قصد — فلو فعل لفهرس محتواها مرة واحدة ولن يرى تغيّرها بعد ذلك. استورد من بطاقة، أو اختر مجلدًا فيه صور.",
  emptyRootIsManagedLibrary:
    "أحد مجلدات المكتبة هو مكتبة مُدارة لتطبيق صور (الصور أو iPhoto أو Aperture). لا يقرأ pictkura داخلها عن قصد، لذا لن يأتي منها شيء. اختر مجلدًا فيه صور، أو استورد من بطاقة.",
  emptyAllExcluded: (names: string) =>
    `كل ما عُثر عليه تتخطاه أنماط الاستبعاد (مثل ${iso(names)}). يمكنك تغييرها في pictkura.toml داخل مجلد الإعدادات.`,
  emptyNothingHere:
    "لم يُعثر بعد على صور يستطيع pictkura قراءتها. استورد من بطاقة، أو اختر مجلدًا فيه صور.",
  calendarChecking: "جارٍ التحقق…",
  emptyTitleStalled: "بعض الأماكن لا تستجيب",
  emptyStalled: (names: string) =>
    `هذه الأماكن لا تستجيب: ${iso(names)}. إذا كان أحدها على الشبكة، فتأكد من أنه ما زال متصلًا واضغط على “إعادة الفحص”. وإذا زال نهائيًا، فأزِله من مجلدات المكتبة ليظهر الباقي.`,
  emptyChecking:
    "ما زال البحث في المجلدات جاريًا. إذا كان أحدها على الشبكة، فتأكد من أنه متصل واضغط على “إعادة الفحص”.",
  emptyLoadFailed: "تعذّر تحميل القائمة. اضغط على “إعادة الفحص”، أو أعد فتح التطبيق.",
  wizardPickFolderHint: "اختر مجلدًا من القائمة لترى الصور التي فيه",
  wizardNoImages: "لا توجد صور في هذا المجلد",
  wizardUnreadable: "تعذّرت قراءة هذا المجلد (ربما أُزيل)",
  wizardCounting: "جارٍ التحميل…",
  wizardSelectAll: "تحديد الكل",
  wizardSelectNew: "تحديد الجديد فقط",
  wizardClearSelection: "إلغاء التحديد",
  wizardSelected: (n: number) => `المحدد: ${num(n)}`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "مستوردة من قبل (الملف نفسه موجود في الوجهة)",
  wizardUnsureBadge: "?",
  wizardUnsureTitle:
    "هذا الاسم موجود على البطاقة أكثر من مرة. لا يُعرف إن كان الملف في الوجهة هو هذه الصورة حتى يُقرأ المحتوى — والاستيراد يقرؤه، ولا ينشئ نسخة ثانية من صورة موجودة",
  wizardDestination: "الوجهة",
  wizardChangeDestination: "تغيير",
  wizardStructure: "الترتيب",
  wizardImportButton: (n: number) => `استيراد (${num(n)})`,
  wizardImportAll: "استيراد هذا المجلد كله (مع المجلدات الفرعية)",
  wizardImportAllShort: "المجلد كله",
  wizardDeep: "تضمين المجلدات الفرعية",
  wizardDeepHint: "يمسح الوسائط كلها فلا تحتاج إلى معرفة مكان الصور",
  wizardScanning: "جارٍ البحث في الوسائط…",
  wizardTruncated: (n: number) =>
    `يُعرض أول ${num(n)} فقط. استخدم “المجلد كله” لاستيراد كل شيء`,
  wizardScanIncomplete: "⚠ تعذّرت قراءة بعض المجلدات (قد تنقص بعض الصور)",
  decoderHeifNotice: (n: number) =>
    `⚠ يتعذّر هنا عرض صور مصغّرة أو فتح ${photos(n)} بتنسيق HEIC/HEIF. يلزم “امتدادات صور HEIF” المجانية، ومعها “امتدادات فيديو HEVC” المدفوعة وزهيدة الثمن التي تفك ترميز الصورة`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ يتعذّر هنا عرض صور مصغّرة أو فتح ${photos(n)} بتنسيق HEIC/HEIF`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ يتعذّر هنا عرض صور مصغّرة أو فتح ${photos(n)} بتنسيق HEIC/HEIF. قد لا يتوفر في نظامك برنامج فك ترميز لـ HEIC/HEVC`,
  decoderHeifHow: "امتدادات صور HEIF (مجانية)",
  decoderHevcHow: "امتدادات فيديو HEVC (مدفوعة)",
  decoderNoticeDismiss: "عدم الإظهار مرة أخرى",
  wizardOfflineTitle: "هذا الملف موجود في السحابة (لا معاينة هنا؛ الاستيراد سيُنزّله)",
  wizardHideImported: "إخفاء المستورد من قبل",
  wizardAllImported: "لا جديد هنا (كل ما في هذا المجلد مستورد من قبل)",
  wizardHiddenCount: (n: number) => `مخفي لأنه مستورد من قبل: ${num(n)}`,
  wizardEtaSeconds: (n: number) => `يتبقى نحو ${num(n)} ث`,
  wizardEtaMinutes: (n: number) => `يتبقى نحو ${num(n)} د`,
  wizardEtaCalculating: "جارٍ تقدير الوقت المتبقي…",
  wizardCapped: (n: number) => `${num(n)}+`,
  wizardMoreFiles: (n: number) => `${num(n)} أخرى (مرّر للتحميل)`,
  menuOpen: "فتح",
  menuOpenWith: (name: string) => `فتح باستخدام ${iso(name)}`,
  menuOpenWithOther: "فتح باستخدام تطبيق آخر…",
  menuReveal: "إظهار في المجلد",
  menuMoreOptions: "إظهار المزيد من الخيارات",
  menuCopyImage: "نسخ الصورة إلى الحافظة",
  menuRotateRight: "تدوير إلى اليمين",
  menuRotateLeft: "تدوير إلى اليسار",
  menuRotateRestore: "استعادة الاتجاه الأصلي",
  menuPrint: "طباعة…",
  printTitle: "طباعة",
  printFitContain: "ملاءمة الصفحة",
  printFitContainHint: "تظهر الصورة كاملة. إذا اختلف شكلها عن الورقة، تبقى هوامش.",
  printFitCover: "قص لملء الصفحة",
  printFitCoverHint: "بلا هوامش، لكن تُقص أطراف الصورة التي لا تتسع.",
  printFitStretch: "مط لملء الصفحة (تجاهل النسب)",
  printFitStretchHint: "بلا هوامش ولا قص، لكن الصورة تُمطّ.",
  printBorderless: "للطباعة بلا حواف، اختر حجم ورق بلا حواف في نافذة الطباعة التالية.",
  printGo: "طباعة…",
  printPreparing: "جارٍ التحضير للطباعة…",
  printSheetTitle: (n: number) => `طباعة ${photos(n)}`,
  printPerPage: "الصور في كل صفحة",
  printCaptions: "إضافة اسم الملف وتاريخ التصوير",
  printTooMany: (max: number) =>
    `يمكن طباعة ${photos(max)} على الأكثر دفعة واحدة بهذا الحجم. اختر عددًا أكبر في كل صفحة، أو حدّد عددًا أقل.`,
  printPreparingN: (done: number, total: number) => `جارٍ التحضير للطباعة… ${num(done)}/${num(total)}`,
  printSkipped: (n: number) =>
    `تعذّرت طباعة ${items(n)} (فيديوهات وما شابه)، فتم استبعادها.`,
  errPrint: "تعذّر تحضير الصورة للطباعة.",
  errShellMenu: "تعذّر عرض قائمة المستكشف.",
  menuDelete: "حذف (نقل إلى سلة المهملات)",
  menuInfo: "معلومات",
  fileInfoTitle: "الملف",
  fileInfoName: "الاسم",
  fileInfoFolder: "المجلد",
  fileInfoSize: "الحجم",
  fileInfoTaken: "التصوير",
  fileInfoTakenUnknown: "غير معروف",
  fileInfoModified: "التعديل",
  fileInfoDimensions: "الأبعاد",
  fileInfoDuration: "المدة",
  fileInfoType: "النوع",
  selectionTotal: (size: string) => `${size} إجمالًا`,
  menuFavoriteOn: "إضافة إلى المفضلة",
  menuFavoriteOff: "إزالة من المفضلة",
  pickEditor: "اختر تطبيقًا للتحرير",
  deleteConfirm: (n: number) =>
    n === 1 ? "هل تريد نقل هذه الصورة إلى سلة المهملات؟" : `هل تريد نقل ${photos(n)} إلى سلة المهملات؟`,
  deleted: (n: number) => `تم نقل ${photos(n)} إلى سلة المهملات`,
  deletedSomeLeft: (n: number, left: number) =>
    `تم نقل ${photos(n)} إلى سلة المهملات (لم يُعثر على ${num(left)} فتُركت كما هي)`,
  deletedMissing: (n: number, missing: number) =>
    `تم نقل ${photos(n)} إلى سلة المهملات (${num(missing)} لم تعد في المكتبة، ولم تُنقل)`,
  selectItem: "تحديد",
  selectedCount: (n: number) => `المحدد: ${num(n)}`,
  selectAll: "تحديد الكل",
  clearSelection: "إلغاء التحديد (Esc)",
  selectDay: "تحديد هذا اليوم كله",
  bulkFavoriteOn: "إضافة إلى المفضلة",
  bulkFavoriteOff: "إزالة من المفضلة",
  bulkDelete: "نقل إلى سلة المهملات",
  bulkCopy: "نسخ إلى مجلد",
  bulkMove: "نقل إلى مجلد",
  bulkViewer: "عرض المحدد",
  pickExportFolder: "اختر مجلدًا للتصدير إليه",
  pickMoveFolder: "اختر مجلدًا للنقل إليه",
  moveConfirm: (n: number) =>
    n === 1
      ? "هل تريد نقل هذه الصورة إلى مجلد تختاره بعد ذلك؟\nستغادر مكانها الحالي وتخرج من المكتبة.\nلا تُنقل علامات ★ و⚑."
      : `هل تريد نقل ${photos(n)} إلى مجلد تختاره بعد ذلك؟\nستغادر مكانها الحالي وتخرج من المكتبة.\nلا تُنقل علامات ★ و⚑.`,
  confirmCancel: "إلغاء",
  deleteConfirmOk: "نقل إلى سلة المهملات",
  moveConfirmOk: "اختيار مكان النقل",
  rootRemoveConfirmOk: "إزالة",
  exporting: (done: number, total: number, name: string) =>
    `جارٍ التصدير… ${num(done)}/${num(total)} ${iso(name)}`,
  exportDone: (done: number, skipped: number, failed: number, missing: number) => {
    const parts: string[] = [];
    // 書き出すものが無かったときに「0枚を書き出しました」から始めない（pictkura-dev #33）
    if (done > 0 || skipped + failed + missing === 0) parts.push(`تم تصدير ${photos(done)}`);
    if (skipped > 0) parts.push(`${num(skipped)} موجودة من قبل`);
    if (failed > 0) parts.push(`فشل ${num(failed)}`);
    if (missing > 0) parts.push(`${num(missing)} لم تعد في المكتبة، ولم تُصدَّر`);
    return parts.join(". ") + ".";
  },
  moving: (done: number, total: number, name: string) =>
    `جارٍ النقل… ${num(done)}/${num(total)} ${iso(name)}`,
  moveDone: (moved: number, skipped: number, failed: number, leftBehind: number, missing: number) => {
    const parts: string[] = [];
    if (moved > 0 || skipped + failed + leftBehind + missing === 0) parts.push(`تم نقل ${photos(moved)}`);
    if (skipped > 0) parts.push(`${num(skipped)} موجودة من قبل، فبقيت في مكانها`);
    if (failed > 0) parts.push(`تعذّر نقل ${num(failed)} فبقيت في مكانها`);
    if (leftBehind > 0) parts.push(`نُسخ ${num(leftBehind)} لكن تعذّرت إزالتها من مكانها الأصلي`);
    if (missing > 0) parts.push(`${num(missing)} لم تعد في المكتبة، ولم تُنقل`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "انتقاء",
  bulkPickOff: "إلغاء الانتقاء",
  bulkPickDone: (n: number) => `تم انتقاء ${photos(n)}`,
  bulkUnpickDone: (n: number) => `تم إلغاء انتقاء ${photos(n)}`,
  bulkFavoriteDone: (n: number) => `تمت إضافة ${photos(n)} إلى المفضلة`,
  bulkUnfavoriteDone: (n: number) => `تمت إزالة ${photos(n)} من المفضلة`,
  settings: "الإعدادات",
  close: "إغلاق",
  settingsTitle: "الإعدادات",
  settingsImportStructure: "بنية مجلدات الاستيراد",
  settingsImportStructureNote:
    "كيف تُرتَّب الصور المستوردة حسب تاريخ التصوير. يمس هذا آلاف الملفات، وتغييره لاحقًا يتطلب جهدًا كبيرًا. حين يحمل اسم المجلد تاريخًا، يُكتب بالسنة أولًا في كل اللغات، حتى يطابق الترتيب بالاسم الترتيب بالزمن.",
  settingsDestination: "الوجهة",
  settingsDestinationUnset: "(غير مضبوطة — ستختارها عند أول استيراد)",
  settingsFlatExample: "IMG_0001.JPG (بلا مجلدات فرعية)",
  settingsCustomPattern: "مخصص",
  settingsCustomPatternNote:
    "تُستبدل {year} و{month} و{day} بالتاريخ. استخدم / للتداخل. تُحذف تلقائيًا الأحرف غير الصالحة والانتقال إلى المجلد الأعلى (..).",
  settingsCustomPatternResult: "المجلد الناتج",
  settingsGrid: "شبكة الصور",
  settingsStackRawJpegToggle: "تجميع أزواج RAW+JPEG في مربع واحد",
  settingsStackRawJpegNote:
    "يظهر ملف RAW وملف JPEG بالاسم نفسه في المجلد نفسه كمربع واحد هو JPEG. في الشبكة، تنطبق ★ و⚑ والحذف والتحديد على الاثنين. أيّ الملفين يُعرض في ملء الشاشة يُضبط في “عند عرض صورة بملء الشاشة”.",
  stackRawChipTitle: (files_: number) => `RAW وJPEG مجمّعان في مربع واحد (${files(files_, false)})`,
  settingsStackBurstsToggle: "تجميع اللقطات المتتابعة في مربع واحد",
  settingsStackBurstsNote:
    "تظهر الصور الملتقطة بتتابع سريع بالكاميرا نفسها كمربع واحد. لا تُجمَّع إلا الصور التي سُجّل وقت تصويرها بأجزاء من الثانية (بعض الكاميرات لا تسجله). في الشبكة، تنطبق ★ و⚑ من قائمة النقر بالزر الأيمن على صورة الغلاف وحدها؛ والحذف والتحديد على المجموعة كلها. لا يجمع ملء الشاشة اللقطات المتتابعة؛ بل يمر على كل لقطة.",
  settingsBurstGap: "أطول فاصل داخل التتابع",
  settingsOutgoing: "مجلد الإرسال",
  settingsGoogleToggle: "وضع الصور المستوردة في مجلد الإرسال أيضًا",
  settingsGoogleNote:
    "كل استيراد يضع الصور في مجلد الإرسال أيضًا (دون أن يشغل مساحة إضافية على القرص). من أزواج RAW+JPEG يدخل JPEG وحده. إذا كنت تستخدم صور Google: أضف هذا المجلد إلى النسخ الاحتياطي للمجلدات في صور Google.",
  settingsGoogleFolder: "المجلد",
  settingsGoogleOpenFolder: "فتح المجلد",
  settingsGoogleCopyPath: "نسخ المسار",
  settingsGoogleCopied: "تم النسخ",
  settingsGoogleCopyFailed: "تعذّر النسخ. حدّد المسار وانسخه.",
  settingsGoogleFolderNote:
    "يقع على محرك الأقراص نفسه الذي عليه وجهة الاستيراد. اختر مكانًا خارج أي مجلد تراقبه نسخة احتياطية أخرى، مثل NAS أو Amazon Photos.",
  settingsGoogleFoldersNote:
    "مجلدات الإرسال التي وضع pictkura فيها صورًا (يُنشأ واحد لكل محرك أقراص). إذا كنت تستخدم صور Google: أضف كل مجلد تستخدمه إلى النسخ الاحتياطي للمجلدات.",
  settingsGoogleVideo: "وضع الفيديوهات أيضًا",
  settingsGoogleRawOnly: "الصور الملتقطة بـ RAW فقط",
  googleRawOnlyNone: "استبعاد",
  googleRawOnlyEmbedded: "وضع JPEG المضمّن",
  settingsGoogleRawOnlyNote:
    "ما يُفعل بملفات RAW الملتقطة بلا JPEG. “وضع JPEG المضمّن” يستخرج صورة المعاينة التي خزنتها الكاميرا داخل RAW ويضعها مع تاريخ التصوير. وملف JPEG المستخرج منفصل، فيشغل مساحة على القرص.",
  settingsGoogleOneDrive: "وضع الصور الموجودة داخل OneDrive أيضًا",
  settingsGoogleOneDriveNote: "إن فعلت، فلن يتمكن OneDrive من جعل تلك الصور متاحة عبر الإنترنت فقط.",
  settingsGoogleAfterNote:
    "حين تنقل صورة إلى سلة المهملات في pictkura، تُزال من مجلد الإرسال أيضًا. إذا كنت تستخدم صور Google: تبقى في صور Google. يتولى الرفعَ موقع صور Google (photos.google.com)، فلا يتقدم إلا وهذه الصفحة مفتوحة في المتصفح.",
  pickGoogleFolder: "اختيار مجلد الإرسال",
  importGoogle: (n: number) => ` · في مجلد الإرسال: ${num(n)}`,
  importGoogleFailed: (n: number) => `تعذّر وضع ${num(n)} في مجلد الإرسال (راجع السجل)`,
  importGoogleError: (why: string) => `لم يُوضع شيء في مجلد الإرسال: ${iso(why)}`,
  bulkSendGoogle: "إضافة إلى مجلد الإرسال",
  bulkRemoveGoogle: "إزالة من مجلد الإرسال",
  calendarSendMonth: "إضافة هذا الشهر إلى مجلد الإرسال",
  calendarSendYear: (y: string) => `إضافة عام ${year(y)} كله إلى مجلد الإرسال`,
  googleSendSummary: (photoCount: number, videoCount: number, rawOnly: number, size: string) => {
    const parts: string[] = [];
    if (photoCount > 0) parts.push(photos(photoCount, false));
    if (videoCount > 0) parts.push(videos(videoCount, false));
    const list: string[] = [];
    if (parts.length > 0) list.push(`${parts.join("، ")}${size ? ` (${size})` : ""}`);
    // 形容詞を数に合わせない形（双数なら مستخرجتان になる）: 「…から取り出した JPEG: N」
    if (rawOnly > 0) list.push(`صور JPEG مستخرجة من صور RAW فقط: ${num(rawOnly)}`);
    return `إلى مجلد الإرسال: ${list.join("؛ ")}.`;
  },
  googleSendLeftOut: (n: number) =>
    `يُستبعد ${items(n, false)} مما اخترته (ملف RAW من زوج RAW+JPEG، والملفات داخل OneDrive وما شابه).`,
  googleSendFolders: (list: string) => `يوضع في: ${iso(list)}`,
  googleSendNewFolder: (path: string) => `سيُنشأ ${iso(path)}.`,
  googleSendGoogleNote:
    "إذا كنت تستخدم صور Google: يتولى الرفعَ موقع صور Google (photos.google.com)، فيتقدم ما دامت هذه الصفحة مفتوحة في المتصفح.",
  googleSendGoogleNoteNew:
    "إذا كنت تستخدم صور Google: أضف كل مجلد جديد إلى النسخ الاحتياطي للمجلدات في صور Google أيضًا. يتولى الرفعَ موقع صور Google (photos.google.com)، فيتقدم ما دامت هذه الصفحة مفتوحة في المتصفح.",
  googleFolderCreated: (path: string) =>
    `أُنشئ ${iso(path)}. إذا كنت تستخدم صور Google: أضف هذا المجلد إلى النسخ الاحتياطي للمجلدات أيضًا.`,
  googleSendNothing: "لا يوجد هنا ما يمكن وضعه.",
  googleSendConfirmOk: "وضع",
  googleSent: (total: number, already: number) =>
    `في مجلد الإرسال: ${num(total)}${already > 0 ? ` (${num(already)} موجودة من قبل)` : ""}`,
  googleSentCloudOnly: (n: number) =>
    `${num(n)} في السحابة فقط، فتعذّر وضعها. نزّلها أولًا ثم أرسلها مرة أخرى.`,
  googleRemoved: (n: number) => `أُزيل من مجلد الإرسال: ${num(n)}`,
  googleRemoveFailed: (n: number) =>
    `تعذّرت إزالة ${num(n)} الآن (ربما فُصل محرك الأقراص). سيحاول pictkura مرة أخرى عند تشغيله التالي.`,
  googleSentLeftOut: (n: number) =>
    `استُبعد ${items(n, false)} (ملف RAW من زوج RAW+JPEG، والملفات داخل OneDrive وما شابه).`,
  burstGapOption: (seconds: number) => `${num(seconds)} ث`,
  burstChip: (frames: number) => `▤ تتابع ${num(frames)}`,
  burstChipShort: (frames: number) => `▤ ${num(frames)}`,
  spreadStack: "توسيع لرؤية كل واحدة",
  foldStack: "طي",
  burstTitle: (frames: number, spanMs: number, fileCount: number) =>
    `تتابع من ${shots(frames)} خلال ${num(Math.round(spanMs / 100) / 10)} ث (${files(fileCount, false)})`,
  burstTitleRawJpeg: (frames: number, spanMs: number, fileCount: number) =>
    `تتابع من ${shots(frames)} خلال ${num(Math.round(spanMs / 100) / 10)} ث، RAW+JPEG (${files(fileCount, false)})`,
  settingsViewer: "عند عرض صورة بملء الشاشة",
  settingsAutoAdvanceToggle: "الانتقال إلى الصورة التالية بعد P / U",
  settingsAutoAdvanceNote:
    "في ملء الشاشة، يضع P علامة ⚑ على الصورة (رفّ منفصل عن مفضلة ★) ويزيلها U. مع تشغيل هذا، تأتي الصورة التالية فورًا، فيكفي مفتاح واحد لكل صورة عند الانتقاء. مع إيقافه، تبقى على الصورة نفسها.",
  settingsPairView: "ما يُعرض من زوج RAW+JPEG",
  pairViewJpeg: "JPEG فقط",
  pairViewRaw: "RAW فقط",
  pairViewBoth: "الاثنان (RAW ثم JPEG)",
  settingsPairViewNote:
    "كيف يمر ملء الشاشة على زوج يظهر كمربع واحد في الشبكة. عند عرض جانب واحد، تنطبق ★ و⚑ و✕ والحذف في ملء الشاشة على ملفي الزوج. وعند عرض الاثنين، تنطبق على الملف المعروض وحده. وتغيّره أيضًا أزرار RAW / JPEG في ملء الشاشة. إذا لم تُجمَّع الأزواج في الشبكة، يُعرض الملفان.",
  viewerPairSide: "أيّ ملفي الزوج يُعرض (ويُحفظ في الإعدادات أيضًا)",
  settingsTransition: "عند الانتقال إلى الصورة التالية",
  settingsSlideshowTransition: "انتقال عرض الشرائح",
  transitionNone: "بلا",
  transitionSlide: "انزلاق",
  transitionFade: "تلاشٍ",
  settingsTransitionNote:
    "كيف ينتقل ملء الشاشة إلى الصورة التالية حين تتنقل (بمفاتيح الأسهم أو الأزرار أو الشريط السفلي أو الانتقال بعد التقييم)، وحين ينتقل عرض الشرائح. يتبع الانزلاق اتجاه تنقلك. إذا كان نظامك مضبوطًا على تقليل الحركة، تتبدل الصور دون حركة أيًّا كان اختيارك هنا.",
  settingsAutoplay: "عند توصيل ذاكرة USB أو بطاقة SD",
  settingsAutoplayNote:
    "إذا ثُبّت pictkura بالمثبّت أو من Microsoft Store، يظهر “pictkura で写真を取り込む” (استيراد الصور باستخدام pictkura) بين الخيارات عند توصيل وسيط (اسم الخيار باليابانية)؛ ولا تقدّمه النسخة المحمولة. لا يبدأ شيء حتى تختاره. بعد اختياره، يتذكره Windows إجراءً افتراضيًا لذلك النوع من الوسائط (بطاقات الذاكرة ومحركات USB تُضبط كلٌّ على حدة) ويفتح نافذة الاستيراد كلما وصّلت النوع نفسه. غيّر الإجراء الافتراضي من إعدادات “التشغيل التلقائي” في Windows. ومع مثبّت MSI الإنجليزي يظهر الخيار بعنوان “Import photos with pictkura”.",
  settingsAutoplayOpenWindows: "فتح إعدادات التشغيل التلقائي في Windows",
  settingsImportWaitingFrom: (path: string) =>
    `استيراد من ${iso(path)} في الانتظار. يُفتح عند إغلاق الإعدادات.`,
  settingsImportWaiting: "استيراد في الانتظار. يُفتح عند إغلاق الإعدادات.",
  settingsImportWaitingOpen: "إغلاق واستيراد",
  settingsAbout: "حول",
  settingsAboutLicense: "يُوزَّع بموجب رخصة MIT.",
  settingsManual: "الدليل",
  settingsOssLicenses: "البرمجيات مفتوحة المصدر المستخدمة",
  settingsDocNotBundled: "(غير مضمّن في نسخة التطوير)",
  settingsLog: "فتح السجل",
  settingsLogNone: "(لم يُسجَّل شيء بعد)",
  settingsOpenFailed: "تعذّر فتحه.",
  settingsLogNote: "لا يُكتب سطر إلا عند فشل شيء، ويبقى على هذا الجهاز. لا يُرسل أي شيء.",
  settingsLanguage: "اللغة",
  settingsLanguageSystem: "مطابقة النظام",
  settingsLanguageNote:
    "يُعيد التبديل تحميل النافذة. يستمر النسخ نفسه في الخلفية، لكن معالج الاستيراد يُغلق فتفقد رؤية تقدّمه — لذا انتظر انتهاء الاستيراد قبل التبديل.",
  settingsLanguageMacNote:
    "تتحول نوافذ macOS مثل لوحة الحفظ ونوافذ التأكيد إلى هذه اللغة عند تشغيل pictkura في المرة التالية.",
  settingsTheme: "المظهر",
  themeSystem: "مطابقة النظام",
  themeLight: "فاتح",
  themeDark: "داكن",
  settingsEditors: "تطبيقات التحرير",
  settingsEditorsNote: "التطبيقات التي اخترتها من “فتح باستخدام تطبيق آخر…”.",
  settingsForgetEditor: "إزالة من القائمة",
  calendarEmpty: "لا صور",
  filterEmptyTitle: (conds: string) => `لا توجد صور تطابق هذه الشروط: ${conds}`,
  filterEmptyMessage: "امسح البحث أو عوامل التصفية في القائمة الجانبية لرؤية البقية.",
  filterEmptyStillIndexing: "ما زالت الصور تُستورد أو تُفهرس، فقد يظهر المزيد.",
  filterCondQuery: (q: string) => `البحث “${iso(q)}”`,
  filterCondCamera: (name: string) => `الملتقطة بـ ${iso(name)}`,
  filterEmptyClearSearch: "مسح البحث",
  errGoogleOverlapsRoot: "لا يمكن اختيار مجلد مكتبة، ولا مجلد داخله، ولا مجلد يحتويه.",
  errGoogleInsideSync:
    "لا يمكن اختيار مكان داخل مجلد مزامنة (OneDrive وGoogle Drive وDropbox وiCloud Drive وما شابه).",
  errGoogleNoHardLinks: "لا يمكن وضعه على محرك الأقراص هذا (لا يمكن إنشاء ارتباطات ثابتة على exFAT وFAT وما شابه).",
  errGoogleWholeVolume:
    "وجهة الاستيراد محرك أقراص كامل، فلا مكان عليه لمجلد الإرسال. غيّر وجهة الاستيراد.",
  errGoogleNoName: "لا يمكن اختيار محرك الأقراص نفسه. اختر مجلدًا عليه.",
  errGoogleLinkInTheWay: "مجلد الإرسال ارتباط رمزي. اختر مكانًا آخر.",
  errGoogleIo: "تعذّرت القراءة من مجلد الإرسال أو الكتابة فيه.",
  errGoogleOtherDrive: "اختر مجلدًا على محرك الأقراص نفسه الذي عليه وجهة الاستيراد.",
  errNotFound: "لم تعد تلك الصورة في الفهرس.",
  errRotateNoOrientation: "لا تحمل هذه الصورة معلومات الاتجاه، فلا يمكن تدويرها.",
  errRotateXmp: "سيتعارض ذلك مع الاتجاه الذي تقرؤه برامج تحرير الصور من XMP، فلا يمكن تدوير هذه الصورة.",
  errRotateCloudOnly: "هذه الصورة في السحابة فقط، فلا يمكن تدويرها. نزّلها ثم حاول مرة أخرى.",
  errRotateCameraCard:
    "تُترك الصور على بطاقة الكاميرا كما هي، لأنك تستورد منها. استورد الصورة ثم دوّرها.",
  errRotateNotReady: "ما زالت قراءة هذه الصورة جارية. انتظر قليلًا ثم حاول مرة أخرى.",
  errRotateChanged: "تغيّر الملف من مكان آخر أثناء تدويره. حاول مرة أخرى.",
  errRotateBusy: "جارٍ فحص مجلداتك. انتظر قليلًا ثم حاول مرة أخرى.",
  errRotateWrite:
    "تعذّرت الكتابة في الملف، فلم تُدوَّر الصورة. تأكد من أن الملف غير مفتوح في تطبيق آخر وأنه ليس للقراءة فقط.",
  errRotateLeftChanged:
    "حدث خطأ بعد إعادة كتابة الملف، وتعذّرت إعادته كما كان. قد تبقى الصورة مُدوَّرة. راجع السجل للتفاصيل.",
  errRotateCannotRestore:
    "لم يعد ممكنًا إعادة هذه الصورة إلى اتجاهها قبل أن يدوّرها pictkura، لأن تطبيقًا آخر غيّرها مثلًا.",
  errDb: "تعذّرت القراءة من الفهرس أو الكتابة فيه.",
  errConfigIo: "تعذّرت القراءة من ملف الإعدادات أو الكتابة فيه.",
  errConfigFormat: "تعذّر فهم ملف الإعدادات.",
  errNoDestination: "لم يُختر مجلد الوجهة بعد.",
  errRootManaged: "لا يمكن إضافة مكتبة مُدارة كمجلد — فما بداخلها ملك لذلك التطبيق.",
  errDestManaged: "لا يمكن أن تكون مكتبة مُدارة هي الوجهة — فما بداخلها ملك لذلك التطبيق.",
  errNoImageToExtract: "لا توجد في هذا الملف صورة يمكن استخراجها.",
  errSourceUnreadable: "تعذّرت قراءة ذلك المجلد.",
  errSourceManaged:
    "لا يمكن الاستيراد من داخل مكتبة مُدارة. احذف الامتداد (.photoslibrary وما شابه) من اسم المجلد ثم اختره مرة أخرى.",
  errExportDest: "تعذّر إنشاء مجلد الوجهة ذاك.",
  errExportManaged: "لا يمكن الكتابة هناك — فتطبيق آخر يدير تلك الحزمة.",
  errFolderMissing: "ذلك المجلد غير موجود.",
  fileOpBusy: "انتظر حتى تنتهي العملية الحالية، ثم حاول مرة أخرى.",
  errFolderNotAbsolute: "أدخل المسار الكامل للمجلد.",
  errTrashFailed: "تعذّر نقله إلى سلة المهملات.",
  errTrashUnreachable: "تعذّر الوصول إلى محرك الأقراص أو المجلد، فلم يُنقل شيء إلى سلة المهملات.",
  errTrashPartly: (n: number) => `تم نقل ${photos(n)} إلى سلة المهملات؛ وتعذّر نقل الباقي.`,
  errNoLogYet: "لم يُسجَّل شيء بعد.",
  errOutgoingFolderMissing:
    "مجلد الإرسال غير موجود. تشغيل مجلد الإرسال يُنشئه (وإن كان مشغّلًا، فأوقفه ثم شغّله مرة أخرى).",
  errOpenOutgoingFailed: "تعذّر فتح مجلد الإرسال. استخدم “نسخ المسار” وافتحه من هناك.",
  errNotBundled: "غير مضمّن في هذه النسخة.",
  errNoStoreLink: "لا توجد صفحة لهذا النظام.",
  errBadKind: "لم يكن هناك ما يُفتح.",
  errTooManyIds: (n: number) => `لا يمكن أكثر من ${num(n)} في المرة الواحدة.`,
};
