/**
 * 韓国語の辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **韓国の標準語（ko-KR）で書く。** 語は発明せず、既に韓国で使われているものへ合わせる
 * （独・西・仏・葡と同じ方針）:
 *
 * - **文体は 합니다체**（Apple・Microsoft の韓国語 UI の説明文と同じ）。釦とメニューの項目は
 *   名詞・動作名詞の短い形（`삭제`・`취소`・`저장`・`휴지통으로 이동`）、見出しは名詞
 * - **⚑ / ✕ / U は Lightroom Classic の韓国語版の型に合わせる**——⚑ は `플래그`
 *   （`플래그 지정` / `플래그 지정됨`）、✕ は `제외`（ボツの候補は `제외 후보`）、U は `플래그 해제`。
 *   **⚑ に `선택` を使わない**——複数選択が `선택` / `선택 해제` なので、西語の `indicador`・
 *   葡語の `escolher` と同じ理由で ⚑ 側は `플래그` に限る。⚑ と ✕ の両方を覆う文字列
 *   （`judgeUnflag`、ショートカットの U の行）は、日本語の「判定」と同じ中立の `판정` にする
 * - **アプリの蔵書は `라이브러리`、写真アプリの蔵書は Apple の語の `사진 보관함`**（`사진` 앱）。
 *   Apple は Library を `보관함` と訳すので、アプリ側まで `보관함` にすると
 *   「ライブラリのフォルダが写真アプリのライブラリそのもの」の文で同じ語がぶつかる
 * - **OSの用語は引いてくる**。Apple の ko（loctable で確認）: `취소`・`삭제`・`휴지통`・
 *   `휴지통으로 이동`・`설정…`・`왼쪽으로 회전` / `오른쪽으로 회전`・`저장`・`전체 선택`・
 *   `데스크탑`・`Finder에서 보기`・macOS の `시스템 설정 → 개인정보 보호 및 보안`。
 *   Windows の ko: `자동 실행`（自動再生）・`추가 옵션 표시`（Windows 11 のエクスプローラー）・
 *   `파일 탐색기`・`하드 링크`・Microsoft Store の `HEIF 이미지 확장` / `HEVC 비디오 확장`
 * - **送り出しは `보내기 폴더`、書き出しは `내보내기`**——ja が「送り出し」と「書き出し」を分けるのと同じく
 *   割る。Outgoing を `내보내기 폴더` にすると、書き出し（`내보낼 폴더 선택`）と同じ語になる
 * - **ゴミ箱は `휴지통`**——Windows も macOS も同じ語なので、OSで割れない
 * - **印刷は Windows の `인쇄`**。Apple の ko は `프린트…` だが、配布の主戦場が Windows で、
 *   韓国の利用者には `인쇄` がいちばん普通の語（macOS の人にも通じる）。1つの語に揃える
 * - **金額は言わない**（`유료`・`저렴한 유료`）。ストアの値段は国と時期で動くので、
 *   ja の「数百円」を円からウォンに直さず、「有料・安い」までにする
 * - **確認画面の取消は `취소`**（AppKit の Cancel。違うと macOS で Esc が効かない）
 *
 * **単複は無い**（ja・zh と同じ）。`one()` を使わない。数には助数詞を付ける
 * （写真 `장`、件 `개`、動画 `개`、ファイル `개`）——数字のすぐ後ろに助詞が来ないので、
 * 助詞の形が数字の読みに左右されない。
 *
 * **助詞は前の語の終わり（パッチム）で形が変わる**（은/는・이/가・을/를・으로/로）。
 * 変数（アプリ名・カメラ名）の直後に「〜で・〜へ」を置く所は、下の `euro()` を通す（試験が、ほかの助詞を
 * 変数の直後に書いていないことを見る）。
 * できるだけ「“名前” 폴더를」のように、**変数の後ろに名詞を置いて助詞をそちらに付ける**言い方にする。
 */
import { folderExample } from "./folderExample.ts";
import { num } from "./plural.ts";
import type { Dict } from "./ja.ts";

/** 語の終わりにパッチムがあるか。ハングルの字から読む。それ以外（ラテン文字・数字）は分からないので null */
function batchim(word: string): { has: boolean; rieul: boolean } | null {
  // 助詞は括弧の前の語に付く（`확장자(.photoslibrary 등)를`）ので、末尾の括弧書きは丸ごと外して読む。
  // 閉じの引用符も外す
  const head = word.replace(/\s*[(（][^()（）]*[)）]\s*$/u, "");
  const ch = head.replace(/[\s\]」』"'”’»]+$/u, "").slice(-1);
  if (!ch) return null;
  const c = ch.charCodeAt(0);
  if (c >= 0xac00 && c <= 0xd7a3) {
    const jong = (c - 0xac00) % 28;
    return { has: jong !== 0, rieul: jong === 8 };
  }
  // **数字で終わる語は読みが割れる**——製品名は英語で読まれることが多い（`Windows 11` は 일레븐、
  // `Photoshop 2025` は 이천이십오 とも twenty-twenty-five とも）。数は助数詞（장・개）を付けて
  // 助詞を数字に直に付けないので、ここに来るのは名前だけ。ラテン文字と同じく両方書く（ゲート2）
  return null;
}


/** 「〜で・〜へ」の 으로/로。ㄹ で終わる語とパッチムの無い語は 로 */
const euro = (word: string): string => {
  const b = batchim(word);
  return word + (b === null ? "(으)로" : b.has && !b.rieul ? "으로" : "로");
};

export const ko: Dict = {
  appName: "pictkura",
  viewThumbnails: "썸네일",
  viewCalendar: "캘린더",
  searchPlaceholder: "검색 (파일 이름, 폴더, 카메라, 2019-08, year:2019)",
  searchClear: "검색어 지우기 (Esc)",
  commandPalette: "명령 팔레트",
  importFromUsb: "USB에서 가져오기",
  rescan: "다시 스캔",
  size: "크기",
  itemsCount: (n: number) => `${num(n)}개`,
  navPlaces: "이미지 위치",
  navAllPhotos: "모든 이미지",
  navFavorites: "★ 즐겨찾기",
  navPicked: "⚑ 플래그 지정됨",
  navOutgoing: "보내기 폴더",
  outgoingGuide:
    "이 보내기 폴더를 사진 서비스 앱에서 백업(동기화)할 폴더로 지정하십시오. 그러면 가져올 때마다 여기에 놓인 사진을 서비스가 가져갑니다. Google 포토를 사용하는 경우: 이 폴더를 Google 포토의 ‘폴더 백업’에 등록하십시오.",
  navKinds: "종류",
  kindPhoto: "이미지",
  kindRaw: "RAW",
  kindVideo: "동영상",
  shortcutsTitle: "단축키 (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "단축키 보기",
  shortcutGroups: [
    {
      title: "목록",
      keys: [
        ["Ctrl+K / ⌘K", "명령 팔레트 (날짜·카메라로 이동, 검색, 가져오기)"],
        ["Ctrl+, / ⌘,", "설정 열기 (도구 막대의 톱니바퀴와 같음)"],
        ["Ctrl+A / ⌘A", "현재 검색과 필터에 맞는 항목을 모두 선택"],
        ["Shift + 클릭", "마지막으로 누른 타일부터 여기까지 한꺼번에 선택"],
        ["Ctrl + 클릭", "한 장을 선택하거나 해제 (macOS는 ⌘ + 클릭)"],
        ["날짜 제목 클릭", "그날의 사진을 모두 선택 (한 번 더 누르면 해제)"],
        ["Esc", "선택 해제"],
      ],
    },
    {
      title: "사진 크게 보기",
      keys: [
        ["← / →", "이전·다음 사진"],
        ["P", "플래그 지정 (⚑). 기본으로 바로 다음 사진으로 넘어감"],
        ["X", "제외 후보로 표시 (✕). 닫을 때 한꺼번에 휴지통으로"],
        ["U", "이 사진의 판정 취소 (⚑와 ✕를 해제)"],
        ["Ctrl+C / ⌘C", "지금 보이는 이미지를 클립보드에 복사"],
        ["Ctrl+S / ⌘S", "지금 보이는 이미지를 파일로 저장"],
        ["Ctrl+P / ⌘P", "지금 보이는 사진 인쇄"],
        ["F", "즐겨찾기 (★) 추가·해제"],
        ["I", "파일 정보와 촬영 정보 (카메라·렌즈·조리개·셔터·ISO·초점 거리·GPS)"],
        ["Space", "슬라이드쇼. 동영상이면 재생/일시 정지"],
        ["1 / 0", "1은 실제 크기 100% ⇔ 화면에 맞춤 전환, 0은 항상 화면에 맞춤"],
        ["F11", "전체 화면"],
        ["Esc", "닫기"],
      ],
    },
    {
      title: "마우스 (사진을 크게 보고 있을 때)",
      keys: [
        ["더블클릭", "실제 크기 100% ⇔ 화면에 맞춤"],
        ["휠", "확대·축소"],
        ["드래그", "확대한 상태에서 위치 이동"],
        ["오른쪽 클릭", "메뉴 열기 (열기·복사·회전·인쇄·정보·삭제 등)"],
        ["아래 띠 클릭", "그 사진으로 바로 이동"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "카메라와 미디어",
  navLibraryFolders: "라이브러리 폴더",
  navDrives: "드라이브",
  navAddFolder: "폴더 추가",
  add: "추가",
  browse: "찾아보기…",
  addFolderPlaceholder: folderExample("예: ", "사용자"),
  pickLibraryFolder: "라이브러리에 추가할 폴더 선택",
  showMore: (n: number) => `${num(n)}대 더보기`,
  collapse: "접기",
  photosCount: (n: number) => `${num(n)}장`,
  memoriesTitle: (years: number) => `${num(years)}년 전 오늘`,
  viewerFavorite: "즐겨찾기 (F)",
  viewerPick: "플래그 지정 (P)",
  viewerUnpick: "플래그 해제 (U)",
  viewerPicked: "플래그 지정된 사진",
  judgeFav: "즐겨찾기",
  judgeUnfav: "즐겨찾기 해제됨",
  judgePick: "플래그 지정됨",
  judgeUnflag: "판정 취소됨",
  viewerReject: "제외 (X)",
  viewerRejected: "제외 후보",
  favoriteMark: "즐겨찾기",
  viewerMutedHint: "음소거 중 — 누르면 소리가 납니다",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "제외 후보 확인",
  rejectGateTitle: (n: number) => `${num(n)}장을 휴지통으로 이동합니다`,
  rejectGateNote: "휴지통에서 되돌릴 수 있습니다 (되돌리면 목록에도 다시 나타납니다).",
  rejectGateRestore: "되돌리기",
  rejectGateBack: "돌아가기",
  rejectGateDiscard: "넣지 않고 닫기",
  rejectGateConfirm: (n: number) => `${num(n)}장을 휴지통으로`,
  rejectGateTrashing: (done: number, total: number) =>
    `이동 중… (${num(done)} / ${num(total)})`,
  // 版の文字列は空で来ることがある（`latest ?? ""`）。助詞が浮かないよう、版を後ろに回す
  updateFound: (v: string) => `새 버전이 나왔습니다${v ? ` (${v})` : ""}`,
  updateOpenPage: "다운로드 페이지 열기",
  updateLater: "나중에",
  updateCheckNow: "업데이트 확인",
  updateChecking: "확인 중…",
  updateUpToDate: "최신 버전입니다",
  updateFailed: "확인하지 못했습니다",
  updateOnStart: "시작할 때 새 버전 확인",
  updateOnStartNote:
    "GitHub에 최신 버전을 물어봅니다(하루 1회). 사진도 파일 이름도 보내지 않습니다. 끄면 위의 ‘업데이트 확인’ 버튼을 누를 때 말고는 전혀 통신하지 않습니다",
  viewerSlideshow: "슬라이드쇼 (Space)",
  extractSave: (key: string) => `이 이미지를 파일로 저장 (${key})`,
  extractCopy: (key: string) => `이 이미지를 클립보드에 복사 (${key})`,
  extractSaveTitle: "이미지 저장 위치",
  extractFilter: "이미지",
  extractSaved: "저장했습니다",
  extractCopied: "복사했습니다",
  extractCopying: "복사하는 중…",
  extractFailed: "꺼내지 못했습니다",
  extractSameFile: "원본 파일에는 덮어쓸 수 없습니다",
  viewerExif: "촬영 정보 (I)",
  viewerFullscreen: "전체 화면 (F11)",
  viewerClose: "닫기 (Esc)",
  viewerPrev: "이전 (←)",
  viewerNext: "다음 (→)",
  viewerFitToScreen: "화면에 맞춤 (0)",
  viewerActualSize: "실제 크기 100%로 보기 (1) · 더블클릭으로도 전환",
  actualSizeBadge: "100%",
  videoUnsupported: "이 형식은 앱 안에서 재생할 수 없습니다",
  fileMissing: "이 파일을 찾을 수 없습니다 (USB 메모리나 SD 카드가 빠졌거나, 파일이 이동·삭제된 것 같습니다)",
  fileUnreachable: "이 파일을 열 수 없습니다 (드라이브가 준비되지 않았거나, 접근이 허용되지 않았습니다)",
  fileNotShown: "이 파일을 표시할 수 없습니다 (파일이 손상되었거나, pictkura가 읽을 수 없는 형식입니다)",
  fileNotDownloaded: "이 파일은 아직 다운로드되지 않았습니다 (온라인이 된 뒤 다시 열어 주십시오)",
  videoCloudOnly: "이 동영상은 클라우드에 있습니다",
  videoCloudOnlyNote:
    "앱 안에서 재생하면 다운로드가 시작되고, 끝날 때까지 아무것도 나오지 않습니다. 기본 앱으로 열면 진행 상황을 보면서 받을 수 있습니다.",
  videoFailed: "이 동영상을 재생하지 못했습니다",
  videoOpenExternal: "기본 앱으로 열기",
  videoCodecNote:
    "iPhone 등의 동영상은 HEVC(H.265)로 기록됩니다. 재생하려면 OS의 디코더가 필요하며, Windows에서는 Microsoft Store의 저렴한 유료 확장 기능입니다.",
  videoCodecNoteMac:
    "macOS는 처음부터 HEVC를 재생할 수 있으므로, 지원하지 않는 기록 방식일 수 있습니다.",
  videoCodecNoteOther:
    "이 기록 방식을 지원하는 디코더가 이 환경에 설치되어 있지 않은 것일 수 있습니다.",
  videoCodecHelp: "HEVC 비디오 확장 보기 (유료)",
  loading: "불러오는 중…",
  exifTitle: "촬영 정보",
  exifCamera: "카메라",
  exifLens: "렌즈",
  exifAperture: "조리개",
  exifShutter: "셔터",
  exifIso: "ISO",
  exifFocal: "초점 거리",
  exifLocation: "촬영 장소",
  exifNone: "EXIF 정보가 없습니다",
  paletteInput: "날짜·카메라·키워드·작업…",
  paletteNoResults: "후보가 없습니다",
  paletteGroupJumpDate: "날짜로 이동",
  paletteGroupRecentDays: "최근 날짜",
  paletteGroupCameras: "카메라로 필터",
  paletteGroupSearch: "검색",
  paletteGroupActions: "작업",
  paletteSearchFor: (q: string) => `“${q}” 검색`,
  paletteSearchHint: "파일 이름·폴더·카메라",
  paletteSelect: "선택",
  paletteRun: "실행",
  paletteCloseHint: "닫기",
  actionShowFavorites: "즐겨찾기만 보기",
  actionShowPicked: "플래그 지정된 사진만 보기",
  actionShowAll: "모든 이미지 보기",
  actionCalendar: "캘린더로 보기",
  actionThumbnails: "썸네일로 보기",
  indexBuilding: "🔍 검색 색인을 만드는 중… ",
  bgScanning: "라이브러리를 업데이트하는 중…",
  bgThumbs: (n: number) => `사진을 불러오는 중 (${num(n)}장 남음)`,
  bgScanningShort: "업데이트 중",
  bgThumbsShort: (n: number) => `${num(n)}장 남음`,
  cameraScanning: "📷 카메라 정보를 읽는 중… ",
  indexIncompleteWarning:
    "⚠ 검색 색인 만들기가 중단되었습니다 / 검색 결과가 일부 빠져 있습니다 (다음 실행 때 이어서 다시 만듭니다)",
  indexProgressSuffix: "% / 완료될 때까지 검색 결과가 일부 빠져 있습니다",
  removeRoot: (path: string) => `라이브러리에서 ${path} 제거`,
  rootMissingNotice: (name: string, count: number) =>
    `“${name}” 폴더를 찾을 수 없습니다${count > 0 ? ` (안의 사진 ${num(count)}장을 열 수 없습니다)` : ""}. USB 메모리나 SD 카드에 있다면 꽂은 뒤 ‘다시 스캔’을 누르십시오.`,
  rootsMissingNotice: (names: string, count: number) =>
    `찾을 수 없는 폴더가 있습니다: ${names}${count > 0 ? ` (모두 합쳐 사진 ${num(count)}장을 열 수 없습니다)` : ""}. USB 메모리나 SD 카드에 있다면 꽂은 뒤 ‘다시 스캔’을 누르십시오. 쓰지 않는 폴더는 왼쪽 목록의 ✕로 라이브러리에서 제거할 수 있습니다 (사진 파일은 지워지지 않습니다).`,
  rootRemoveFromLibrary: "라이브러리에서 제거",
  rootRemoveKeepsFiles: "라이브러리 목록에서 빼기만 합니다. 사진 파일은 지우지 않습니다",
  rootRemoveConfirm: (name: string) =>
    `“${name}” 폴더를 라이브러리에서 제거하시겠습니까?\n사진 파일은 지워지지 않지만, 안의 사진에 붙인 ★와 ⚑ 표시는 사라집니다.`,
  rootTempConfirm: (path: string) =>
    `“${path}” 폴더는 임시 폴더 안에 있습니다.\nOS나 다른 앱이 이 안의 파일을 모르는 사이에 지울 수 있습니다. 그래도 라이브러리에 추가하시겠습니까?`,
  rootTempConfirmOk: "추가",
  destTempConfirm: (path: string) =>
    `“${path}” 폴더는 임시 폴더 안에 있습니다.\n여기로 가져오면 OS나 다른 앱이 모르는 사이에 사진을 지울 수 있습니다. 카드를 지운 뒤에는 그 사진이 어디에도 남지 않습니다. 그래도 복사 위치로 지정하시겠습니까?`,
  destTempConfirmOk: "복사 위치로 지정",
  destTempWarning:
    "⚠ 복사 위치가 임시 폴더 안에 있습니다. 가져온 사진이 모르는 사이에 사라질 수 있습니다.",
  rootTempTip: (path: string) =>
    `${path} — 임시 폴더 안에 있습니다. OS나 다른 앱이 이 안의 파일을 모르는 사이에 지울 수 있습니다`,
  rootMissingTempTip: (path: string) =>
    `${path} — 찾을 수 없습니다. 임시 폴더 안에 있었으므로, OS나 다른 앱이 지웠을 수 있습니다`,
  rootMissingTip: (path: string) =>
    `${path} — 찾을 수 없습니다. USB 메모리나 SD 카드에 있다면 꽂은 뒤 ‘다시 스캔’을 누르십시오`,
  importFrom: (path: string) => `${path}에서 가져오기`,
  filterByCamera: (name: string) => `${euro(name)} 찍은 사진만 보기`,
  jumpToYear: (year: number) => `${year}년으로`,
  importing: (done: number, total: number) => `가져오는 중… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `가져오기 완료: 복사 ${num(copied)}, 건너뜀 ${num(skipped)}`,
  importFailed: (n: number) => `, 실패 ${num(n)}`,
  importIncomplete: " ⚠ 읽을 수 없는 폴더가 있습니다 (카드를 아직 지우지 마십시오)",
  syncDone: (added: number, changed: number, removed: number) =>
    `추가 ${num(added)}, 변경 ${num(changed)}, 삭제 ${num(removed)}`,
  drivesReturnedScanned: (added: number, changed: number) =>
    `연결된 드라이브의 폴더를 다시 읽었습니다 (추가 ${num(added)}, 변경 ${num(changed)})`,
  pickSource: "가져올 원본 폴더 선택 (USB/DCIM)",
  pickDestination: "복사 위치 폴더 선택",
  wizardTitle: "가져오기",
  wizardSources: "가져올 곳",
  wizardOtherFolder: "다른 폴더…",
  wizardRefresh: "드라이브 다시 찾기",
  wizardRemovable: "이동식",
  wizardNoDrives: "드라이브를 찾을 수 없습니다",
  emptyTitle: "아직 사진이 없습니다",
  emptyTitleFailed: "목록을 표시하지 못했습니다",
  emptyTitleChecking: "확인하고 있습니다",
  emptyTitleStartupFailed: "시작할 때의 동기화가 완료되지 않았습니다",
  emptyStartupFailed:
    "시작할 때의 동기화가 완료되지 않았습니다. 사진이 있어도 아직 목록에 들어오지 않았을 수 있습니다. ‘다시 스캔’을 누르십시오. 그래도 해결되지 않으면 앱을 다시 여십시오.",
  emptyTitleMissing: "찾을 수 없는 위치가 있습니다",
  emptyTitleUnreadable: "열 수 없는 위치가 있습니다",
  emptyNoRoots:
    "라이브러리 폴더가 아직 설정되지 않았습니다. 카드에서 가져오거나, 사진이 있는 폴더를 선택하십시오.",
  emptyMissing: (names: string) =>
    `다음 위치를 찾을 수 없습니다: ${names}. USB 메모리나 SD 카드에 있다면 꽂은 뒤 ‘다시 스캔’을 누르십시오.`,
  emptyUnreadableMac: (names: string) =>
    `다음 위치를 열지 못했습니다: ${names}. ‘시스템 설정 → 개인정보 보호 및 보안’에서 pictkura가 그 폴더(데스크탑·문서·외장 드라이브 등)에 접근하도록 허용하십시오. 네트워크 폴더라면 연결되어 있는지 확인한 뒤 ‘다시 스캔’을 누르십시오.`,
  emptyUnreadableWin: (names: string) =>
    `다음 위치를 열지 못했습니다: ${names}. 폴더의 접근 권한을 확인하십시오. 네트워크 드라이브라면 연결되어 있는지 확인한 뒤 ‘다시 스캔’을 누르십시오.`,
  emptyUnreadableOther: (names: string) =>
    `다음 위치를 열지 못했습니다: ${names}. 읽기 권한이 있는지 확인한 뒤 ‘다시 스캔’을 누르십시오.`,
  listSeparator: ", ",
  // `외 N개` は前の名詞に直に付く（`A, B 외 2개`）。読点を挟まない（Gemini）
  listLastSeparator: " ",
  andMore: (n: number) => `외 ${num(n)}개`,
  emptyRootIsPackage:
    "라이브러리 폴더로 ‘사진’ 앱의 사진 보관함 자체가 지정되어 있습니다. pictkura는 그 안을 다루지 않으므로, 여기서는 아무것도 나오지 않습니다. 사진이 있는 폴더를 다시 선택하거나, 카드에서 가져오십시오.",
  emptyPhotoLibrary:
    "‘사진’ 앱의 사진 보관함 말고는 다룰 수 있는 것을 찾지 못했습니다. pictkura는 기본적으로 ‘사진’ 앱 안을 다루지 않습니다 (원본 대부분은 iCloud에 있고, 이 컴퓨터에는 없습니다). 카드에서 가져오거나, 사진이 있는 폴더를 선택하십시오.",
  emptyManagedLibrary:
    "사진을 관리하는 앱의 보관함(‘사진’ 앱·iPhoto·Aperture 등) 말고는 다룰 수 있는 것을 찾지 못했습니다. pictkura는 그 안을 다루지 않습니다 (안을 색인해도 나중의 변경이 반영되지 않아 오래된 상태로 남기 때문입니다). 카드에서 가져오거나, 사진이 있는 폴더를 선택하십시오.",
  emptyRootIsManagedLibrary:
    "라이브러리 폴더로 사진을 관리하는 앱의 보관함(‘사진’ 앱·iPhoto·Aperture 등) 자체가 지정되어 있습니다. pictkura는 그 안을 다루지 않으므로, 여기서는 아무것도 나오지 않습니다. 사진이 있는 폴더를 다시 선택하거나, 카드에서 가져오십시오.",
  emptyAllExcluded: (names: string) =>
    `찾은 것은 모두 제외 설정으로 건너뛰었습니다 (예: ${names}). 설정 폴더의 pictkura.toml에서 바꿀 수 있습니다.`,
  emptyNothingHere:
    "다룰 수 있는 이미지를 아직 찾지 못했습니다. 카드에서 가져오거나, 사진이 있는 폴더를 선택하십시오.",
  calendarChecking: "확인하고 있습니다…",
  emptyTitleStalled: "응답이 없는 위치가 있습니다",
  emptyStalled: (names: string) =>
    `다음 위치에서 응답이 없습니다: ${names}. 네트워크 폴더라면 연결되어 있는지 확인하고 ‘다시 스캔’을 누르십시오. 계속 빼 둘 거라면 라이브러리 폴더에서 제거하면 나머지 위치의 결과가 나옵니다.`,
  emptyChecking:
    "폴더를 확인하는 중입니다. 네트워크 폴더라면 연결되어 있는지 확인한 뒤 ‘다시 스캔’을 누르십시오.",
  emptyLoadFailed:
    "목록을 불러오지 못했습니다. ‘다시 스캔’을 누르거나 앱을 다시 여십시오.",
  wizardPickFolderHint: "왼쪽에서 폴더를 선택하면 그 안의 이미지가 나타납니다",
  wizardNoImages: "이 폴더에는 이미지가 없습니다",
  wizardUnreadable: "폴더를 읽지 못했습니다 (분리되었을 수 있습니다)",
  wizardCounting: "불러오는 중…",
  wizardSelectAll: "모두 선택",
  wizardSelectNew: "가져오지 않은 것만 선택",
  wizardClearSelection: "선택 해제",
  wizardSelected: (n: number) => `${num(n)}장 선택됨`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "가져오기 완료 (복사 위치에 같은 파일이 있습니다)",
  wizardUnsureBadge: "?",
  wizardUnsureTitle:
    "카드에 같은 이름이 두 개 이상 있습니다. 복사 위치에 있는 것이 이 사진인지는 내용을 읽기 전까지 알 수 없습니다 (가져올 때 확인합니다. 같은 것이면 늘어나지 않습니다)",
  wizardDestination: "복사 위치",
  wizardChangeDestination: "변경",
  wizardStructure: "분류",
  wizardImportButton: (n: number) => `${num(n)}장 가져오기`,
  wizardImportAll: "이 폴더를 통째로 가져오기 (하위 폴더 포함)",
  wizardImportAllShort: "폴더 전체",
  wizardDeep: "하위 폴더 포함",
  wizardDeepHint: "미디어 안을 모두 뒤져서 보여 줍니다 (어디에 들어 있는지 몰라도 됩니다)",
  wizardScanning: "미디어 안을 찾고 있습니다…",
  wizardTruncated: (n: number) =>
    `너무 많아서 처음 ${num(n)}장만 표시합니다. 모두 가져오려면 ‘폴더 전체’를 쓰십시오`,
  wizardScanIncomplete: "⚠ 읽을 수 없는 폴더가 있었습니다 (빠진 사진이 있을 수 있습니다)",
  decoderHeifNotice: (n: number) =>
    `⚠ HEIC/HEIF ${num(n)}장은 이 환경에서 미리보기를 만들 수 없습니다 (열어도 표시되지 않습니다). 무료 ‘HEIF 이미지 확장’에 더해, 화소를 풀어내는 저렴한 유료 ‘HEVC 비디오 확장’이 필요합니다`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ HEIC/HEIF ${num(n)}장은 이 환경에서 미리보기를 만들 수 없습니다 (열어도 표시되지 않습니다)`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ HEIC/HEIF ${num(n)}장은 이 환경에서 미리보기를 만들 수 없습니다 (열어도 표시되지 않습니다). HEIC/HEVC를 지원하는 디코더가 설치되어 있지 않은 것일 수 있습니다`,
  decoderHeifHow: "HEIF 이미지 확장 (무료)",
  decoderHevcHow: "HEVC 비디오 확장 (유료)",
  decoderNoticeDismiss: "다시 표시하지 않기",
  wizardOfflineTitle:
    "클라우드에 있는 파일입니다 (여기서는 이미지를 표시하지 않습니다. 가져오면 다운로드됩니다)",
  wizardHideImported: "가져온 것 숨기기",
  wizardAllImported: "새 사진이 없습니다 (이 폴더는 모두 가져왔습니다)",
  wizardHiddenCount: (n: number) => `가져온 ${num(n)}장은 숨겼습니다`,
  wizardEtaSeconds: (n: number) => `약 ${num(n)}초 남음`,
  wizardEtaMinutes: (n: number) => `약 ${num(n)}분 남음`,
  wizardEtaCalculating: "남은 시간을 계산하고 있습니다…",
  wizardCapped: (n: number) => `${num(n)}+장`,
  wizardMoreFiles: (n: number) => `${num(n)}장 더 있음 (스크롤하면 표시)`,
  menuOpen: "열기",
  menuOpenWith: (name: string) => `${euro(name)} 열기`,
  menuOpenWithOther: "다른 앱으로 열기…",
  menuReveal: "파일 위치 열기",
  menuMoreOptions: "추가 옵션 표시",
  menuCopyImage: "이미지를 클립보드에 복사",
  menuRotateRight: "오른쪽으로 회전",
  menuRotateLeft: "왼쪽으로 회전",
  menuRotateRestore: "방향을 원래대로",
  menuPrint: "인쇄…",
  printTitle: "인쇄",
  printFitContain: "용지에 맞춰 확대",
  printFitContainHint: "사진 전체가 들어갑니다. 사진과 용지의 가로세로 비율이 다르면 여백이 남습니다.",
  printFitCover: "자르기 (여백 없음)",
  printFitCoverHint: "여백은 생기지 않지만, 용지 밖으로 나가는 부분은 잘립니다.",
  printFitStretch: "용지 가득 (가로세로 비율 무시)",
  printFitStretchHint: "여백도 없고 사진도 잘리지 않지만, 사진이 세로나 가로로 늘어납니다.",
  printBorderless: "테두리 없이 인쇄하려면, 다음 인쇄 화면에서 테두리 없는 용지를 선택하십시오.",
  printGo: "인쇄 화면으로",
  printPreparing: "인쇄를 준비하고 있습니다…",
  printSheetTitle: (n: number) => `${num(n)}장 인쇄`,
  printPerPage: "한 페이지의 장수",
  printCaptions: "파일 이름과 촬영 날짜 넣기",
  printTooMany: (max: number) =>
    `이 장수로는 한 번에 ${num(max)}장까지 인쇄할 수 있습니다. 한 페이지의 장수를 늘리거나 고른 사진을 줄이십시오.`,
  printPreparingN: (done: number, total: number) =>
    `인쇄를 준비하고 있습니다… ${num(done)}/${num(total)}`,
  printSkipped: (n: number) => `${num(n)}개는 인쇄할 수 없어서 (동영상 등) 뺐습니다.`,
  errPrint: "인쇄용 이미지를 준비하지 못했습니다.",
  errShellMenu: "파일 탐색기의 메뉴를 열지 못했습니다.",
  menuDelete: "삭제 (휴지통으로)",
  menuInfo: "정보",
  fileInfoTitle: "파일",
  fileInfoName: "이름",
  fileInfoFolder: "위치",
  fileInfoSize: "크기",
  fileInfoTaken: "촬영 일시",
  fileInfoTakenUnknown: "알 수 없음",
  fileInfoModified: "수정 일시",
  fileInfoDimensions: "화소 수",
  fileInfoDuration: "길이",
  fileInfoType: "형식",
  selectionTotal: (size: string) => `합계 ${size}`,
  menuFavoriteOn: "즐겨찾기에 추가",
  menuFavoriteOff: "즐겨찾기 해제",
  pickEditor: "편집에 쓸 앱 선택",
  deleteConfirm: (n: number) =>
    n === 1
      ? "이 사진을 휴지통으로 이동하시겠습니까?"
      : `사진 ${num(n)}장을 휴지통으로 이동하시겠습니까?`,
  deleted: (n: number) => `${num(n)}장을 휴지통으로 이동했습니다`,
  deletedSomeLeft: (n: number, left: number) =>
    `${num(n)}장을 휴지통으로 이동했습니다 (${num(left)}장은 찾지 못해 그대로 두었습니다)`,
  deletedMissing: (n: number, missing: number) =>
    `${num(n)}장을 휴지통으로 이동했습니다 (${num(missing)}장은 라이브러리에서 사라져 있어서 이동하지 않았습니다)`,
  selectItem: "선택",
  selectedCount: (n: number) => `${num(n)}장 선택됨`,
  selectAll: "모두 선택",
  clearSelection: "선택 해제 (Esc)",
  selectDay: "이날을 한꺼번에 선택",
  bulkFavoriteOn: "즐겨찾기에 추가",
  bulkFavoriteOff: "즐겨찾기 해제",
  bulkDelete: "휴지통으로",
  bulkCopy: "폴더로 복사",
  bulkMove: "폴더로 이동",
  bulkViewer: "선택한 사진 보기",
  pickExportFolder: "내보낼 폴더 선택",
  pickMoveFolder: "이동할 폴더 선택",
  moveConfirm: (n: number) =>
    n === 1
      ? "이 사진을 다음에 고를 폴더로 이동하시겠습니까?\n원래 위치에서는 없어지고, 라이브러리에서도 빠집니다.\n★와 ⚑ 표시는 옮겨 가지 않습니다."
      : `사진 ${num(n)}장을 다음에 고를 폴더로 이동하시겠습니까?\n원래 위치에서는 없어지고, 라이브러리에서도 빠집니다.\n★와 ⚑ 표시는 옮겨 가지 않습니다.`,
  confirmCancel: "취소",
  deleteConfirmOk: "휴지통으로 이동",
  moveConfirmOk: "이동할 곳 선택",
  rootRemoveConfirmOk: "제거",
  exporting: (done: number, total: number, name: string) =>
    `내보내는 중… ${num(done)}/${num(total)} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, missing: number) => {
    const parts: string[] = [];
    if (done > 0 || skipped + failed + missing === 0) parts.push(`${num(done)}장을 내보냈습니다`);
    if (skipped > 0) parts.push(`${num(skipped)}장은 같은 것이 이미 있었습니다`);
    if (failed > 0) parts.push(`${num(failed)}장은 실패했습니다`);
    if (missing > 0) parts.push(`${num(missing)}장은 라이브러리에서 사라져 있어서 내보내지 않았습니다`);
    return parts.join(". ");
  },
  moving: (done: number, total: number, name: string) =>
    `이동하는 중… ${num(done)}/${num(total)} ${name}`,
  moveDone: (moved: number, skipped: number, failed: number, leftBehind: number, missing: number) => {
    const parts: string[] = [];
    if (moved > 0 || skipped + failed + leftBehind + missing === 0) parts.push(`${num(moved)}장을 이동했습니다`);
    if (skipped > 0) parts.push(`${num(skipped)}장은 이동할 곳에 같은 것이 이미 있어서 원래 위치에 두었습니다`);
    if (failed > 0) parts.push(`${num(failed)}장은 이동하지 못해 원래 위치에 남아 있습니다`);
    if (leftBehind > 0) parts.push(`${num(leftBehind)}장은 이동할 곳에 복사했지만 원본을 지우지 못했습니다`);
    if (missing > 0) parts.push(`${num(missing)}장은 라이브러리에서 사라져 있어서 이동하지 않았습니다`);
    return parts.join(". ");
  },
  bulkPickOn: "플래그 지정",
  bulkPickOff: "플래그 해제",
  bulkPickDone: (n: number) => `${num(n)}장에 플래그를 지정했습니다`,
  bulkUnpickDone: (n: number) => `${num(n)}장의 플래그를 해제했습니다`,
  bulkFavoriteDone: (n: number) => `${num(n)}장을 즐겨찾기에 추가했습니다`,
  bulkUnfavoriteDone: (n: number) => `${num(n)}장의 즐겨찾기를 해제했습니다`,
  settings: "설정",
  close: "닫기",
  settingsTitle: "설정",
  settingsImportStructure: "가져올 때의 폴더 구성",
  settingsImportStructureNote:
    "USB에서 가져온 사진을 촬영 날짜로 어떻게 나눌지 정합니다. 사진 수천 장에 적용되므로, 나중에 바꾸려면 손이 많이 갑니다. 폴더 이름에 날짜를 넣을 때는 언어와 관계없이 연-월-일 순서로 합니다 — 이름순으로 정렬했을 때 시간순이 되도록 하기 위해서입니다.",
  settingsDestination: "복사 위치",
  settingsDestinationUnset: "(미설정: 처음 가져올 때 선택합니다)",
  settingsFlatExample: "IMG_0001.JPG (분류하지 않음)",
  settingsCustomPattern: "직접 지정",
  settingsCustomPatternNote:
    "{year} {month} {day}가 날짜로 바뀝니다. /로 하위 폴더가 됩니다. 쓸 수 없는 문자나 상위 폴더로의 이동(..)은 자동으로 뺍니다.",
  settingsCustomPatternResult: "만들어지는 폴더",
  settingsGrid: "목록",
  settingsStackRawJpegToggle: "RAW와 JPEG 쌍을 한 장으로 겹치기",
  settingsStackRawJpegNote:
    "같은 폴더에 있는 같은 이름의 RAW와 JPEG을 목록에서는 JPEG 한 장으로 묶습니다. 목록에서의 ★·⚑·삭제·선택은 쌍의 양쪽에 적용됩니다. 사진을 크게 볼 때 어느 쪽을 보여 줄지는 ‘사진을 크게 볼 때’ 설정에서 고를 수 있습니다.",
  stackRawChipTitle: (files: number) =>
    `RAW와 JPEG을 한 장으로 겹쳤습니다 (파일 ${num(files)}개)`,
  settingsStackBurstsToggle: "연속 촬영을 한 장으로 겹치기",
  settingsStackBurstsNote:
    "같은 카메라로 연달아 찍은 사진을 목록에서는 한 장으로 묶습니다. 촬영 시각이 초보다 잘게 기록된 사진만 대상입니다 (기록하지 않는 카메라도 있습니다). 목록의 오른쪽 클릭으로 붙이는 ★·⚑는 대표 사진 한 장에만 붙고, 삭제와 선택은 연속 촬영 전체에 적용됩니다. 사진을 크게 볼 때는 연속 촬영도 묶지 않고 한 장씩 넘깁니다.",
  settingsBurstGap: "연속 촬영으로 보는 간격",
  settingsOutgoing: "보내기 폴더",
  settingsGoogleToggle: "가져온 사진을 보내기 폴더에도 놓기",
  settingsGoogleNote:
    "가져올 때마다 사진을 보내기 폴더에도 놓습니다 (디스크를 더 쓰지 않습니다). RAW와 JPEG 쌍은 JPEG만 놓습니다. Google 포토를 사용하는 경우: 이 폴더를 Google 포토의 ‘폴더 백업’에 등록하십시오.",
  settingsGoogleFolder: "폴더",
  settingsGoogleOpenFolder: "폴더 열기",
  settingsGoogleCopyPath: "경로 복사",
  settingsGoogleCopied: "복사했습니다",
  settingsGoogleCopyFailed: "복사하지 못했습니다. 경로를 선택해서 옮겨 적으십시오.",
  settingsGoogleFolderNote:
    "가져올 위치와 같은 드라이브에 둡니다. NAS나 Amazon Photos 등 다른 백업이 지켜보는 폴더 밖을 고르십시오.",
  settingsGoogleFoldersNote:
    "pictkura가 지금까지 사진을 놓은 보내기 폴더입니다 (드라이브마다 만듭니다). Google 포토를 사용하는 경우: 쓰는 폴더는 모두 ‘폴더 백업’에 등록하십시오.",
  settingsGoogleVideo: "동영상도 놓기",
  settingsGoogleRawOnly: "RAW만 있는 사진",
  googleRawOnlyNone: "놓지 않음",
  googleRawOnlyEmbedded: "내장 JPEG 놓기",
  settingsGoogleRawOnlyNote:
    "JPEG을 함께 찍지 않은 RAW를 어떻게 할지입니다. ‘내장 JPEG’는 카메라가 RAW 안에 넣어 둔 표시용 이미지를 꺼내, 촬영 일시를 붙여 놓습니다. 꺼낸 JPEG는 별도의 파일이므로 그만큼 디스크를 씁니다.",
  settingsGoogleOneDrive: "OneDrive 안의 사진도 놓기",
  settingsGoogleOneDriveNote: "놓으면 그 사진은 OneDrive에서 ‘온라인 전용’으로 만들 수 없게 됩니다.",
  settingsGoogleAfterNote:
    "pictkura에서 사진을 휴지통에 넣으면 보내기 폴더에서도 뺍니다. Google 포토를 사용하는 경우: Google 포토에서는 지워지지 않습니다. 업로드는 Google 포토의 웹 페이지(photos.google.com)가 하므로, 브라우저에서 그 페이지를 열어 둔 동안에만 진행됩니다.",
  pickGoogleFolder: "보내기 폴더 선택",
  importGoogle: (n: number) => ` · 보내기 폴더에 ${num(n)}개`,
  importGoogleFailed: (n: number) =>
    `보내기 폴더에 ${num(n)}개를 놓지 못했습니다 (로그 참고)`,
  importGoogleError: (why: string) => `보내기 폴더에 놓지 못했습니다: ${why}`,
  bulkSendGoogle: "보내기 폴더로",
  bulkRemoveGoogle: "보내기 폴더에서 빼기",
  calendarSendMonth: "이 달을 보내기 폴더로",
  calendarSendYear: (year: string) => `${year}년 전체를 보내기 폴더로`,
  googleSendSummary: (photos: number, videos: number, rawOnly: number, size: string) => {
    const parts: string[] = [];
    if (photos > 0) parts.push(`사진 ${num(photos)}장`);
    if (videos > 0) parts.push(`동영상 ${num(videos)}개`);
    const items: string[] = [];
    if (parts.length > 0) items.push(`${parts.join(", ")}${size ? ` (${size})` : ""}`);
    if (rawOnly > 0) items.push(`RAW에서 꺼내는 JPEG ${num(rawOnly)}장`);
    return `보내기 폴더에 놓습니다: ${items.join("; ")}.`;
  },
  googleSendLeftOut: (n: number) =>
    `고른 것 가운데 ${num(n)}개는 놓지 않습니다 (RAW와 JPEG 쌍의 RAW, OneDrive 안의 파일 등).`,
  googleSendFolders: (list: string) => `놓을 곳: ${list}`,
  googleSendNewFolder: (path: string) => `${path} 폴더를 새로 만듭니다.`,
  googleSendGoogleNote:
    "Google 포토를 사용하는 경우: 업로드는 Google 포토의 웹 페이지(photos.google.com)가 하므로, 브라우저에서 그 페이지를 열어 둔 동안 차례로 진행됩니다.",
  googleSendGoogleNoteNew:
    "Google 포토를 사용하는 경우: 새 폴더는 모두 Google 포토의 ‘폴더 백업’에 등록하십시오. 업로드는 Google 포토의 웹 페이지(photos.google.com)가 하므로, 브라우저에서 그 페이지를 열어 둔 동안 차례로 진행됩니다.",
  googleFolderCreated: (path: string) =>
    `${path} 폴더를 새로 만들었습니다. Google 포토를 사용하는 경우: 이 폴더도 ‘폴더 백업’에 등록하십시오.`,
  googleSendNothing: "놓을 수 있는 것이 없습니다.",
  googleSendConfirmOk: "놓기",
  googleSent: (total: number, already: number) =>
    `보내기 폴더에 ${num(total)}개를 놓았습니다${already > 0 ? ` (그중 ${num(already)}개는 이미 있었습니다)` : ""}`,
  googleSentCloudOnly: (n: number) =>
    `${num(n)}개는 클라우드에만 있어서 놓지 못했습니다. 이 컴퓨터로 받은 뒤 다시 보내십시오.`,
  googleRemoved: (n: number) => `보내기 폴더에서 ${num(n)}개를 뺐습니다`,
  googleRemoveFailed: (n: number) =>
    `${num(n)}개는 지금 빼지 못했습니다 (드라이브가 연결되어 있지 않은 경우 등). 다음에 pictkura를 실행할 때 다시 뺍니다.`,
  googleSentLeftOut: (n: number) =>
    `${num(n)}개는 놓지 않았습니다 (RAW와 JPEG 쌍의 RAW, OneDrive 안의 파일 등).`,
  burstGapOption: (seconds: number) => `${num(seconds)}초 이내`,
  burstChip: (frames: number) => `▤ 연속 촬영 ${num(frames)}`,
  burstChipShort: (frames: number) => `▤ ${num(frames)}`,
  spreadStack: "펼쳐서 한 장씩 보기",
  foldStack: "접기",
  burstTitle: (frames: number, spanMs: number, files: number) =>
    `연속 촬영 ${num(frames)}장 · ${num(Math.round(spanMs / 100) / 10)}초 동안 (파일 ${num(files)}개)`,
  burstTitleRawJpeg: (frames: number, spanMs: number, files: number) =>
    `연속 촬영 ${num(frames)}장 · ${num(Math.round(spanMs / 100) / 10)}초 동안 · RAW+JPEG (파일 ${num(files)}개)`,
  settingsViewer: "사진을 크게 볼 때",
  settingsAutoAdvanceToggle: "P / U로 판정하면 다음 사진으로 넘어가기",
  settingsAutoAdvanceNote:
    "전체 화면에서 P를 누르면 ⚑ 플래그가 붙고(★ 즐겨찾기와는 별개입니다), U로 해제됩니다. 이 설정을 켜 두면 바로 다음 사진이 나옵니다 (고르는 작업이 한 장에 한 번의 조작으로 진행됩니다). 끄면 그 사진에 머뭅니다.",
  settingsPairView: "RAW+JPEG 쌍에서 보여 줄 것",
  pairViewJpeg: "JPEG만",
  pairViewRaw: "RAW만",
  pairViewBoth: "둘 다 (RAW → JPEG 순)",
  settingsPairViewNote:
    "목록에서 한 장으로 묶은 쌍을 크게 볼 때 어떻게 넘길지입니다. 한쪽만 보여 줄 때는 거기서 붙인 ★·⚑·✕와 삭제가 쌍의 양쪽에 적용됩니다. 둘 다일 때는 보고 있는 파일에만 적용됩니다. 크게 보는 화면의 RAW / JPEG 버튼으로도 바꿀 수 있습니다. 목록에서 쌍을 묶지 않을 때는 둘 다 나옵니다.",
  viewerPairSide: "쌍의 어느 쪽을 볼지 (설정에도 남습니다)",
  settingsTransition: "다음 사진으로 넘길 때의 효과",
  settingsSlideshowTransition: "슬라이드쇼 전환",
  transitionNone: "없음",
  transitionSlide: "슬라이드",
  transitionFade: "페이드",
  settingsTransitionNote:
    "크게 보는 화면에서 다음 사진으로 넘어갈 때(화살표 키·넘기기 버튼·아래 띠·판정 뒤의 자동 넘김)와 슬라이드쇼가 다음으로 넘어갈 때의 효과입니다. 슬라이드는 누른 방향으로 흐릅니다. OS 설정에서 애니메이션을 줄여 두었다면 무엇을 골라도 움직이지 않습니다.",
  settingsAutoplay: "USB나 SD 카드를 꽂았을 때",
  settingsAutoplayNote:
    "설치 프로그램판 pictkura와 Microsoft Store판은 꽂았을 때의 선택지에 “pictkura で写真を取り込む”(pictkura로 사진 가져오기)가 나옵니다 (항목 이름은 일본어로 표시됩니다) (휴대용판은 나오지 않습니다). 고르기 전에는 실행되지 않습니다. 한 번 고르면 Windows가 그 종류(메모리 카드와 USB 드라이브는 따로)의 기본 동작으로 기억하고, 다음부터는 같은 종류를 꽂을 때마다 가져오기 화면이 열립니다. 꽂았을 때의 기본 동작은 Windows의 ‘자동 실행’ 설정에서 바꿀 수 있습니다.",
  settingsAutoplayOpenWindows: "Windows 자동 실행 설정 열기",
  settingsImportWaitingFrom: (path: string) =>
    `${path}에서 가져오기가 기다리고 있습니다. 설정을 닫으면 열립니다.`,
  settingsImportWaiting: "가져오기가 기다리고 있습니다. 설정을 닫으면 열립니다.",
  settingsImportWaitingOpen: "닫고 가져오기",
  settingsAbout: "이 앱에 대하여",
  settingsAboutLicense: "MIT 라이선스로 배포합니다.",
  settingsManual: "사용 설명서",
  settingsOssLicenses: "사용 중인 오픈 소스",
  settingsDocNotBundled: "(개발 중인 실행에는 포함되어 있지 않습니다)",
  settingsLog: "로그 열기",
  settingsLogNone: "(아직 기록이 없습니다)",
  settingsOpenFailed: "열지 못했습니다.",
  settingsLogNote: "실패했을 때만 이 컴퓨터 안에 한 줄씩 남깁니다. 전송하지 않습니다.",
  settingsLanguage: "언어",
  settingsLanguageSystem: "OS에 맞춤",
  settingsLanguageNote:
    "고르면 화면을 다시 불러옵니다. 복사 자체는 뒤에서 계속되지만, 가져오기 마법사가 닫혀 진행 상황이 보이지 않게 되므로, 가져오는 중이라면 끝난 뒤에 바꾸십시오.",
  settingsLanguageMacNote:
    "저장 패널이나 확인 대화상자 같은 macOS의 창은 다음에 pictkura를 실행할 때부터 이 언어가 됩니다.",
  settingsTheme: "테마",
  themeSystem: "시스템에 맞춤",
  themeLight: "라이트",
  themeDark: "다크",
  settingsEditors: "편집에 쓸 앱",
  settingsEditorsNote: "‘다른 앱으로 열기…’에서 고른 앱을 기억합니다.",
  settingsForgetEditor: "목록에서 빼기",
  calendarEmpty: "사진이 없습니다",
  filterEmptyTitle: (conds: string) => `이 필터에 맞는 사진이 없습니다: ${conds}`,
  filterEmptyMessage: "검색어나 왼쪽 목록에서 고른 필터를 해제하면 다른 사진이 나타납니다.",
  filterEmptyStillIndexing: "아직 가져오기·색인 중이므로 나중에 나타날 수 있습니다.",
  filterCondQuery: (q: string) => `검색어 “${q}”`,
  filterCondCamera: (name: string) => `${euro(name)} 찍은 것`,
  filterEmptyClearSearch: "검색어 지우기",
  errGoogleOverlapsRoot: "라이브러리 폴더 안이나, 그것을 포함하는 폴더는 고를 수 없습니다.",
  errGoogleInsideSync:
    "동기화 폴더(OneDrive·Google 드라이브·Dropbox·iCloud Drive 등) 안은 고를 수 없습니다.",
  errGoogleNoHardLinks: "이 드라이브에는 놓을 수 없습니다 (exFAT·FAT 등은 하드 링크를 만들 수 없습니다).",
  errGoogleWholeVolume:
    "가져올 위치가 드라이브 전체라서 보내기 폴더를 둘 곳이 없습니다. 가져올 위치를 바꾸십시오.",
  errGoogleNoName: "드라이브 자체는 고를 수 없습니다. 안의 폴더를 고르십시오.",
  errGoogleLinkInTheWay: "보내기 폴더가 심볼릭 링크로 되어 있습니다. 다른 위치를 고르십시오.",
  errGoogleIo: "보내기 폴더를 읽거나 쓰지 못했습니다.",
  errGoogleOtherDrive: "가져올 위치와 같은 드라이브의 폴더를 고르십시오.",
  errNotFound: "그 사진의 기록을 찾을 수 없습니다.",
  errRotateNoOrientation: "이 사진에는 방향 정보가 없어서 회전할 수 없습니다.",
  errRotateXmp: "RAW 현상 프로그램이 사용하는 방향 정보(XMP)와 어긋나서 회전할 수 없습니다.",
  errRotateCloudOnly: "사진이 클라우드에만 있어서 회전할 수 없습니다. 다운로드한 뒤 다시 시도하십시오.",
  errRotateCameraCard:
    "카메라 카드 안의 사진은 가져올 원본으로 보호하고 있어서 회전할 수 없습니다. 가져온 뒤 회전하십시오.",
  errRotateNotReady: "사진 정보를 불러오고 있습니다. 잠시 기다린 뒤 다시 시도하십시오.",
  errRotateChanged: "회전하는 동안 다른 곳에서 파일이 바뀌었습니다. 다시 시도하십시오.",
  errRotateBusy: "폴더를 확인하고 있습니다. 잠시 기다린 뒤 다시 시도하십시오.",
  errRotateWrite:
    "파일에 쓰지 못해 회전하지 못했습니다. 다른 앱에서 열려 있지 않은지, 읽기 전용이 아닌지 확인하십시오.",
  errRotateLeftChanged:
    "다시 쓴 뒤 문제가 생겨 되돌리지 못했으므로, 사진이 회전된 채로 남았을 수 있습니다. 자세한 내용은 로그를 확인하십시오.",
  errRotateCannotRestore:
    "다른 앱에서의 변경 등으로, pictkura로 회전하기 전의 방향으로 되돌릴 수 없게 되었습니다.",
  errDb: "색인을 읽거나 쓰지 못했습니다.",
  errConfigIo: "설정 파일을 읽거나 쓰지 못했습니다.",
  errConfigFormat: "설정 파일의 내용을 읽지 못했습니다.",
  errNoDestination: "복사 위치 폴더가 정해지지 않았습니다.",
  errRootManaged:
    "앱이 관리하는 보관함은 폴더로 등록할 수 없습니다 (그 안은 앱의 내부 파일입니다).",
  errDestManaged:
    "앱이 관리하는 보관함은 복사 위치로 지정할 수 없습니다 (그 안은 앱의 내부 파일입니다).",
  errNoImageToExtract: "이 사진에서 꺼낼 수 있는 이미지가 없습니다.",
  errSourceUnreadable: "가져올 원본 폴더를 읽을 수 없습니다.",
  errSourceManaged:
    "앱이 관리하는 보관함 안은 가져올 수 없습니다. 폴더 이름에서 확장자(.photoslibrary 등)를 뺀 뒤 다시 고르십시오.",
  errExportDest: "내보낼 폴더를 만들 수 없습니다.",
  errExportManaged: "이 위치로는 내보낼 수 없습니다 (앱이 관리하는 보관함입니다).",
  errFolderMissing: "폴더를 찾을 수 없습니다.",
  fileOpBusy: "이전 작업이 끝난 뒤 다시 누르십시오.",
  errFolderNotAbsolute: "폴더의 전체 경로를 입력하십시오.",
  errTrashFailed: "휴지통으로 옮기지 못했습니다.",
  errTrashUnreachable: "드라이브나 폴더를 찾을 수 없어서 휴지통으로 옮기지 못했습니다.",
  errTrashPartly: (n: number) =>
    `${num(n)}장은 휴지통으로 옮겼습니다. 나머지는 옮기지 못했습니다.`,
  errNoLogYet: "아직 기록이 없습니다.",
  errOutgoingFolderMissing:
    "보내기 폴더를 찾을 수 없습니다. 보내기 폴더를 켜면(이미 켜져 있다면 한 번 껐다가 다시 켜면) 만들어집니다.",
  errOpenOutgoingFailed:
    "보내기 폴더를 열지 못했습니다. ‘경로 복사’로 위치를 옮겨 적어 여십시오.",
  errNotBundled: "이 실행 환경에는 포함되어 있지 않습니다.",
  errNoStoreLink: "이 OS에서는 안내할 수 있는 입수처가 없습니다.",
  errBadKind: "여는 방법을 알 수 없었습니다.",
  errTooManyIds: (n: number) => `한 번에 다룰 수 있는 것은 ${num(n)}개까지입니다.`,
};

// 助詞の関数は辞書の外から試験できるように出しておく（`scripts/` が読む）
export { euro as koEuro };
